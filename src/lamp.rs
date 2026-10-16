//! Dispositivo `LampArray`: localiza el hidraw por firma y perfil USB,
//! transporta informes con `HidTransport` (`HidRaw` es el único punto con FFI
//! ioctl) y expone atributos, color y efectos autónomos.

use std::fs;
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::protocol::{
    autonomous_report, color_report, hid_ioctl_request, parse_attrs, ATTRS_REPORT_LEN,
    DEVICE_SIGNATURE, HIDIOCGFEATURE_NR, HIDIOCSFEATURE_NR, REPORT_DESCRIPTOR_SUBPATH,
    REPORT_ID_ATTRS,
};

// SAFETY: `ioctl` es libc con ABI estable.
unsafe extern "C" {
    fn ioctl(fd: i32, request: u64, arg: *mut u8) -> i32;
}

/// Mensaje de error cuando el teclado se desconecta durante la animación.
pub const DEVICE_GONE_MSG: &str = "el teclado parece haberse desconectado";

/// VID/PID del AERO X16 admitido (coinciden con la regla udev).
pub const SUPPORTED_VID: u16 = 0x0414;
pub const SUPPORTED_PID: u16 = 0x8104;

const ENODEV: i32 = 19;
const ENXIO: i32 = 6;
const ENOENT: i32 = 2;

/// Error de informe que conserva el errno original bajo un mensaje legible.
#[derive(Debug)]
struct DeviceReportError {
    code: Option<i32>,
    msg: String,
}

impl std::fmt::Display for DeviceReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for DeviceReportError {}

/// Indica si `e` corresponde a un dispositivo desaparecido
/// (ENODEV/ENXIO/ENOENT), incluso tras envolver el mensaje.
pub fn is_device_gone(e: &io::Error) -> bool {
    if matches!(e.raw_os_error(), Some(ENODEV | ENXIO | ENOENT)) {
        return true;
    }
    e.get_ref()
        .and_then(|r| r.downcast_ref::<DeviceReportError>())
        .is_some_and(|d| matches!(d.code, Some(ENODEV | ENXIO | ENOENT)))
}

fn report_error(path: &Path, action: &str, id: u8, e: &io::Error) -> io::Error {
    let msg = format!(
        "no se pudo {action} el informe {id} de {}: {e}",
        path.display()
    );
    let code = e.raw_os_error();
    io::Error::new(e.kind(), DeviceReportError { code, msg })
}

/// Acceso a feature reports HID; permite probar `Lamp` sin hardware.
pub trait HidTransport {
    /// Lee hasta `len` bytes del informe `id` (`get_feature`).
    fn get_feature(&mut self, id: u8, len: usize) -> io::Result<Vec<u8>>;
    /// Escribe un informe completo (`set_feature`).
    fn set_feature(&mut self, report: &[u8]) -> io::Result<()>;
}

/// Transporte real sobre `/dev/hidrawN` (único punto con ioctl FFI).
pub struct HidRaw {
    file: fs::File,
    path: PathBuf,
}

impl HidRaw {
    /// Abre `path` en lectura/escritura.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "no se pudo abrir {}: {e} (puede que la regla udev uaccess no funcione)",
                        path.display()
                    ),
                )
            })?;
        Ok(HidRaw { file, path })
    }
}

impl HidTransport for HidRaw {
    fn get_feature(&mut self, id: u8, len: usize) -> io::Result<Vec<u8>> {
        let request = hid_ioctl_request(HIDIOCGFEATURE_NR, len)
            .map_err(|e| report_error(&self.path, "leer", id, &e))?;
        let mut buf = vec![0u8; len];
        buf[0] = id;
        // SAFETY: buffer de `len` bytes; la petición codifica ese `len`.
        let r = unsafe { ioctl(self.file.as_raw_fd(), request, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(report_error(
                &self.path,
                "leer",
                id,
                &io::Error::last_os_error(),
            ));
        }
        #[allow(clippy::cast_sign_loss)]
        let got = r as usize;
        if got == 0 || got > len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "longitud de informe inválida ({got}) al leer id {id} de {}",
                    self.path.display()
                ),
            ));
        }
        buf.truncate(got);
        if buf.first().copied() != Some(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "el informe leído de {} no tiene el id {id} esperado",
                    self.path.display()
                ),
            ));
        }
        Ok(buf)
    }

    fn set_feature(&mut self, report: &[u8]) -> io::Result<()> {
        let id = report.first().copied().unwrap_or(0);
        let request = hid_ioctl_request(HIDIOCSFEATURE_NR, report.len())
            .map_err(|e| report_error(&self.path, "escribir", id, &e))?;
        // SAFETY: petición = report.len(); el kernel solo lee el buffer.
        let r = unsafe { ioctl(self.file.as_raw_fd(), request, report.as_ptr().cast_mut()) };
        if r < 0 {
            return Err(report_error(
                &self.path,
                "escribir",
                id,
                &io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
}

/// Dispositivo `LampArray` con su transporte y número de lámparas.
pub struct Lamp<T: HidTransport = HidRaw> {
    dev: T,
    count: u16,
}

/// Lee VID/PID USB del hidraw vía uevent (`HID_ID=bustype:vid:pid`).
fn usb_ids_for_hidraw(hidraw_name: &str) -> Option<(u16, u16)> {
    let uevent = PathBuf::from(format!("/sys/class/hidraw/{hidraw_name}/device/uevent"));
    let text = fs::read_to_string(uevent).ok()?;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("HID_ID=") else {
            continue;
        };
        // Formato típico: 0003:00000414:00008104
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() < 3 {
            continue;
        }
        let vid = u16::try_from(u32::from_str_radix(parts[1], 16).ok()?).ok()?;
        let pid = u16::try_from(u32::from_str_radix(parts[2], 16).ok()?).ok()?;
        return Some((vid, pid));
    }
    None
}

fn matches_supported_profile(hidraw_name: &str) -> bool {
    match usb_ids_for_hidraw(hidraw_name) {
        Some((vid, pid)) => vid == SUPPORTED_VID && pid == SUPPORTED_PID,
        // Sin uevent legible: no escribir informes específicos.
        None => false,
    }
}

impl Lamp<HidRaw> {
    /// Busca el hidraw con firma `LampArray` y VID/PID admitidos.
    pub fn find() -> io::Result<PathBuf> {
        let mut found: Vec<PathBuf> = Vec::new();
        for e in fs::read_dir("/sys/class/hidraw")? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with("hidraw") {
                continue;
            }
            if !matches_supported_profile(&name) {
                continue;
            }
            let desc = e.path().join(REPORT_DESCRIPTOR_SUBPATH);
            if let Ok(d) = fs::read(&desc) {
                if d.windows(DEVICE_SIGNATURE.len())
                    .any(|w| w == DEVICE_SIGNATURE)
                {
                    found.push(PathBuf::from(format!("/dev/{name}")));
                }
            }
        }
        found.sort();
        found.into_iter().next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no se encontró el dispositivo LampArray admitido (VID 0414 / PID 8104); \
                 ¿está cargada la regla udev? mira con 'keybackcon info'",
            )
        })
    }

    /// Localiza, abre y consulta los atributos del dispositivo.
    pub fn open() -> io::Result<Self> {
        let path = Self::find()?;
        let mut lamp = Self::with_transport(HidRaw::open(path)?);
        lamp.count = lamp.attrs()?.0.max(1);
        Ok(lamp)
    }
}

impl<T: HidTransport> Lamp<T> {
    /// Crea un `Lamp` con un transporte inyectado (tests y usos internos).
    pub(crate) fn with_transport(dev: T) -> Self {
        Lamp { dev, count: 1 }
    }

    /// Lee el informe de atributos: `(LampCount, LampArrayKind)`.
    pub fn attrs(&mut self) -> io::Result<(u16, u32)> {
        let b = self.dev.get_feature(REPORT_ID_ATTRS, ATTRS_REPORT_LEN)?;
        if b.len() < ATTRS_REPORT_LEN.min(19) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("informe de atributos demasiado corto ({} bytes)", b.len()),
            ));
        }
        parse_attrs(&b)
    }

    /// Activa o desactiva los efectos autónomos del firmware.
    pub fn autonomous(&mut self, on: bool) -> io::Result<()> {
        self.dev.set_feature(&autonomous_report(on))
    }

    /// Escribe el color de la zona completa con la intensidad máxima.
    pub fn color(&mut self, rgb: crate::color::Rgb) -> io::Result<()> {
        self.dev.set_feature(&color_report(self.count, rgb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        attrs: Vec<u8>,
        writes: Vec<Vec<u8>>,
        get_len: Option<usize>,
    }

    impl Fake {
        fn new(attrs: Vec<u8>) -> Self {
            Fake {
                attrs,
                writes: Vec::new(),
                get_len: None,
            }
        }
    }

    impl HidTransport for Fake {
        fn get_feature(&mut self, _id: u8, len: usize) -> io::Result<Vec<u8>> {
            if let Some(got) = self.get_len {
                if got == 0 || got > len {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "longitud de informe inválida",
                    ));
                }
                let mut b = vec![0u8; got];
                let n = self.attrs.len().min(got);
                b[..n].copy_from_slice(&self.attrs[..n]);
                return Ok(b);
            }
            let mut b = vec![0u8; len];
            let n = self.attrs.len().min(len);
            b[..n].copy_from_slice(&self.attrs[..n]);
            Ok(b)
        }

        fn set_feature(&mut self, report: &[u8]) -> io::Result<()> {
            self.writes.push(report.to_vec());
            Ok(())
        }
    }

    fn attrs_buf(count: u16, kind: u32) -> Vec<u8> {
        let mut b = vec![0u8; 23];
        b[0] = REPORT_ID_ATTRS;
        b[1..3].copy_from_slice(&count.to_le_bytes());
        b[15..19].copy_from_slice(&kind.to_le_bytes());
        b
    }

    #[test]
    fn attrs_consulta_el_informe_1() {
        let mut lamp = Lamp::with_transport(Fake::new(attrs_buf(5, 1)));
        assert_eq!(lamp.attrs().unwrap(), (5, 1));
    }

    #[test]
    fn color_reporta_una_sola_zona() {
        let mut lamp = Lamp::with_transport(Fake::new(attrs_buf(1, 0)));
        lamp.color((0x11, 0x22, 0x33)).unwrap();
        assert_eq!(
            lamp.dev.writes.last().unwrap(),
            &[5, 1, 0, 0, 0, 0, 0x11, 0x22, 0x33, 0xff]
        );
    }

    #[test]
    fn color_usa_count_menos_uno_en_la_ultima_lampara() {
        let mut lamp = Lamp::with_transport(Fake::new(attrs_buf(3, 0)));
        lamp.count = 3;
        lamp.color((0xaa, 0xbb, 0xcc)).unwrap();
        assert_eq!(
            lamp.dev.writes.last().unwrap(),
            &[5, 1, 0, 0, 2, 0, 0xaa, 0xbb, 0xcc, 0xff]
        );
    }

    #[test]
    fn autonomous_escribe_el_reporte_6() {
        let mut lamp = Lamp::with_transport(Fake::new(attrs_buf(1, 0)));
        lamp.autonomous(true).unwrap();
        assert_eq!(lamp.dev.writes.last().unwrap(), &[6, 1]);
        lamp.autonomous(false).unwrap();
        assert_eq!(lamp.dev.writes.last().unwrap(), &[6, 0]);
    }

    #[test]
    fn clasifica_desconexion_por_errno() {
        for code in [19, 6, 2] {
            assert!(is_device_gone(&io::Error::from_raw_os_error(code)));
        }
        assert!(!is_device_gone(&io::Error::from_raw_os_error(13)));
        assert!(!is_device_gone(&io::Error::other("sin errno")));
    }

    #[test]
    fn clasifica_desconexion_tras_envolver_mensaje() {
        let original = io::Error::from_raw_os_error(ENODEV);
        let wrapped = report_error(Path::new("/dev/hidraw0"), "escribir", 5, &original);
        assert!(is_device_gone(&wrapped));
        assert!(wrapped.to_string().contains("hidraw0"));
    }

    #[test]
    fn informe_corto_se_rechaza_en_attrs() {
        let mut fake = Fake::new(attrs_buf(1, 0));
        fake.get_len = Some(5);
        let mut lamp = Lamp::with_transport(fake);
        let err = lamp.attrs().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
