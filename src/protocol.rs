//! Fuente única del protocolo HID `LampArray` en el cable: constantes,
//! construcción de informes y peticiones ioctl. `lamp.rs` solo consume.

use std::io;

use crate::color::Rgb;

/// Firma del descriptor de un `LampArray`: `Usage Page LampArray` (`0x05
/// 0x59`), `Usage LampArray` (`0x09 0x01`) y `Collection Application` (`0xA1
/// 0x01`). USB HID Usage Tables v1.4 §26.
pub const DEVICE_SIGNATURE: &[u8] = &[0x05, 0x59, 0x09, 0x01, 0xA1, 0x01];

/// Subruta sysfs del descriptor de informes del hidraw.
pub const REPORT_DESCRIPTOR_SUBPATH: &str = "device/report_descriptor";

/// ID del informe de atributos (`get_feature`).
pub const REPORT_ID_ATTRS: u8 = 1;
/// ID del informe de color por zona (`set_feature`).
pub const REPORT_ID_COLOR: u8 = 5;
/// ID del informe de efectos autónomos (`set_feature`).
pub const REPORT_ID_AUTONOMOUS: u8 = 6;

/// Longitud del informe de atributos que se solicita.
pub const ATTRS_REPORT_LEN: usize = 23;
/// Longitud mínima aceptada al parsear el informe de atributos.
pub const ATTRS_MIN_LEN: usize = 19;
/// Longitud del informe de color.
pub const COLOR_REPORT_LEN: usize = 10;

/// Offset de `LampCount` (u16 LE) en el informe de atributos.
pub const ATTRS_LAMP_COUNT_OFFSET: usize = 1;
/// Offset de `LampArrayKind` (u32 LE) en el informe de atributos.
pub const ATTRS_LAMP_KIND_OFFSET: usize = 15;

/// Nº de zonas del informe de color (el teclado expone una).
pub const COLOR_ZONE_COUNT: u8 = 1;
/// Intensidad máxima (último byte del informe de color).
pub const COLOR_INTENSITY_MAX: u8 = 0xff;

/// Byte de efectos autónomos activados.
pub const AUTONOMOUS_ON: u8 = 1;
/// Byte de efectos autónomos desactivados.
pub const AUTONOMOUS_OFF: u8 = 0;

/// Número de `HIDIOCGFEATURE` en `include/uapi/linux/hidraw.h`.
pub const HIDIOCGFEATURE_NR: u64 = 0x07;
/// Número de `HIDIOCSFEATURE` en `include/uapi/linux/hidraw.h`.
pub const HIDIOCSFEATURE_NR: u64 = 0x06;
/// Tipo ioctl `'H'` (hidraw).
pub const HID_IOCTL_TYPE: u64 = 0x48;
/// Bits de dirección `_IOC_WRITE | _IOC_READ` de `_IOWR`.
pub const HID_IOCTL_DIR: u64 = 0xC000_0000;
/// Longitud máxima codificable en el campo size del ioctl (14 bits de `_IOC_SIZEBITS`, `include/uapi/asm-generic/ioctl.h`).
pub const HID_IOCTL_MAX_LEN: usize = 0x3FFF;

/// Codifica una petición `_IOWR` de HID con la longitud real del informe;
/// rechaza 0 y longitudes mayores que `HID_IOCTL_MAX_LEN`.
pub fn hid_ioctl_request(nr: u64, len: usize) -> io::Result<u64> {
    if len == 0 || len > HID_IOCTL_MAX_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("longitud de informe inválida: {len}"),
        ));
    }
    Ok(HID_IOCTL_DIR | ((len as u64) << 16) | (HID_IOCTL_TYPE << 8) | (nr & 0xff))
}

/// Extrae `LampCount` y `LampArrayKind` del informe de atributos.
pub(crate) fn parse_attrs(buf: &[u8]) -> io::Result<(u16, u32)> {
    if buf.len() < ATTRS_MIN_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("informe de atributos demasiado corto: {} bytes", buf.len()),
        ));
    }
    Ok((
        u16::from_le_bytes([
            buf[ATTRS_LAMP_COUNT_OFFSET],
            buf[ATTRS_LAMP_COUNT_OFFSET + 1],
        ]),
        u32::from_le_bytes([
            buf[ATTRS_LAMP_KIND_OFFSET],
            buf[ATTRS_LAMP_KIND_OFFSET + 1],
            buf[ATTRS_LAMP_KIND_OFFSET + 2],
            buf[ATTRS_LAMP_KIND_OFFSET + 3],
        ]),
    ))
}

/// Informe de color para la zona completa `0..=count-1`.
pub(crate) fn color_report(count: u16, (r, g, b): Rgb) -> [u8; COLOR_REPORT_LEN] {
    let first = 0u16.to_le_bytes();
    let last = count.saturating_sub(1).to_le_bytes();
    [
        REPORT_ID_COLOR,
        COLOR_ZONE_COUNT,
        first[0],
        first[1],
        last[0],
        last[1],
        r,
        g,
        b,
        COLOR_INTENSITY_MAX,
    ]
}

/// Informe de efectos autónomos `[6, on]`.
pub(crate) fn autonomous_report(on: bool) -> [u8; 2] {
    [
        REPORT_ID_AUTONOMOUS,
        if on { AUTONOMOUS_ON } else { AUTONOMOUS_OFF },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generador xorshift64 determinista: sustituye a `proptest` sin dependencias.
    struct Xorshift64(u64);

    impl Xorshift64 {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn next_u8(&mut self) -> u8 {
            u8::try_from(self.next() >> 56).unwrap()
        }
    }

    fn attrs_buf(count: u16, kind: u32) -> Vec<u8> {
        let mut b = vec![0u8; ATTRS_REPORT_LEN];
        b[ATTRS_LAMP_COUNT_OFFSET..=ATTRS_LAMP_COUNT_OFFSET + 1]
            .copy_from_slice(&count.to_le_bytes());
        b[ATTRS_LAMP_KIND_OFFSET..ATTRS_LAMP_KIND_OFFSET + 4].copy_from_slice(&kind.to_le_bytes());
        b
    }

    #[test]
    fn parse_attrs_lee_campos_le_y_rechaza_corto() {
        assert_eq!(
            parse_attrs(&attrs_buf(1, 0x1122_3344)).unwrap(),
            (1, 0x1122_3344)
        );
        assert_eq!(parse_attrs(&attrs_buf(3, 7)).unwrap(), (3, 7));
        assert!(parse_attrs(&attrs_buf(1, 0)[..10]).is_err());
    }

    #[test]
    fn hid_ioctl_request_codifica_longitud_y_rechaza_invalidas() {
        for len in [1usize, 10, 23, HID_IOCTL_MAX_LEN] {
            let r = hid_ioctl_request(HIDIOCGFEATURE_NR, len).unwrap();
            let expect = u64::try_from(len).unwrap();
            assert_eq!(r >> 30, 0b11);
            assert_eq!((r >> 16) & 0x3fff, expect);
            assert_eq!(
                r,
                HID_IOCTL_DIR | (expect << 16) | (HID_IOCTL_TYPE << 8) | HIDIOCGFEATURE_NR
            );
            assert_eq!((r >> 8) & 0xff, HID_IOCTL_TYPE);
            assert_eq!(r & 0xff, HIDIOCGFEATURE_NR);
        }
        assert!(hid_ioctl_request(HIDIOCSFEATURE_NR, 0).is_err());
        assert!(hid_ioctl_request(HIDIOCSFEATURE_NR, HID_IOCTL_MAX_LEN + 1).is_err());
        assert!(hid_ioctl_request(HIDIOCSFEATURE_NR, 0x1_0000).is_err());
        assert!(hid_ioctl_request(HIDIOCSFEATURE_NR, usize::from(u16::MAX)).is_err());
    }

    #[test]
    fn color_report_invariantes_de_campos() {
        for count in [0u16, 1, 2, 100, u16::MAX] {
            let r = color_report(count, (0x12, 0x34, 0x56));
            assert_eq!(r.len(), COLOR_REPORT_LEN);
            assert_eq!(r[0], REPORT_ID_COLOR);
            assert_eq!(r[1], COLOR_ZONE_COUNT);
            assert_eq!(u16::from_le_bytes([r[2], r[3]]), 0);
            assert_eq!(u16::from_le_bytes([r[4], r[5]]), count.saturating_sub(1));
            assert_eq!(r[6..9], [0x12, 0x34, 0x56]);
            assert_eq!(r[COLOR_REPORT_LEN - 1], COLOR_INTENSITY_MAX);
            assert_eq!(COLOR_INTENSITY_MAX, 0xff);
        }
    }

    #[test]
    fn hid_ioctl_request_codifica_todas_las_longitudes() {
        let mut rng = Xorshift64(0x0123_4567_89ab_cdef);
        for len in 1..=HID_IOCTL_MAX_LEN {
            let nr = u64::from(rng.next_u8());
            let r = hid_ioctl_request(nr, len).unwrap();
            let expect = u64::try_from(len).unwrap();
            assert_eq!(r >> 30, 0b11);
            assert_eq!(
                (r >> 16) & 0x3fff,
                expect,
                "campo size del ioctl para len={len}"
            );
            assert_eq!(
                r,
                HID_IOCTL_DIR | (expect << 16) | (HID_IOCTL_TYPE << 8) | (nr & 0xff)
            );
            assert_eq!((r >> 8) & 0xff, HID_IOCTL_TYPE);
            assert_eq!(r & 0xff, nr & 0xff);
        }
        for len in [1usize, 10, 23] {
            let r = hid_ioctl_request(HIDIOCGFEATURE_NR, len).unwrap();
            assert_eq!((r >> 16) & 0x3fff, u64::try_from(len).unwrap());
        }
        let borde = hid_ioctl_request(HIDIOCGFEATURE_NR, HID_IOCTL_MAX_LEN).unwrap();
        assert_eq!(
            (borde >> 16) & 0x3fff,
            u64::try_from(HID_IOCTL_MAX_LEN).unwrap()
        );
        assert!(hid_ioctl_request(HIDIOCGFEATURE_NR, 0).is_err());
        assert!(hid_ioctl_request(HIDIOCGFEATURE_NR, HID_IOCTL_MAX_LEN + 1).is_err());
        assert!(hid_ioctl_request(HIDIOCGFEATURE_NR, 0x1_0000).is_err());
        assert!(hid_ioctl_request(HIDIOCGFEATURE_NR, usize::from(u16::MAX)).is_err());
    }

    /// Oráculo UAPI Linux independiente de las constantes del crate (PRO-06 /
    /// TEST-03). Valores tomados de `include/uapi/asm-generic/ioctl.h` y
    /// `include/uapi/linux/hidraw.h` (`x86_64` / `aarch64` usan el genérico).
    ///
    /// No reutiliza `HID_IOCTL_*` del módulo bajo prueba: si esas constantes
    /// se desalinean del kernel, este test falla.
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    mod uapi_oracle {
        use super::super::{hid_ioctl_request, HIDIOCGFEATURE_NR, HIDIOCSFEATURE_NR};

        // asm-generic/ioctl.h
        const IOC_NRBITS: u32 = 8;
        const IOC_TYPEBITS: u32 = 8;
        const IOC_SIZEBITS: u32 = 14;
        const IOC_NRSHIFT: u32 = 0;
        const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
        const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
        const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
        const IOC_WRITE: u32 = 1;
        const IOC_READ: u32 = 2;

        fn ioc(dir: u32, type_: u32, nr: u32, size: u32) -> u64 {
            u64::from(
                (dir << IOC_DIRSHIFT)
                    | (type_ << IOC_TYPESHIFT)
                    | (nr << IOC_NRSHIFT)
                    | (size << IOC_SIZESHIFT),
            )
        }

        /// Equivalente a `HIDIOCGFEATURE(len)` / `HIDIOCSFEATURE(len)` del UAPI.
        fn hid_ioc_feature(nr: u32, len: u32) -> u64 {
            ioc(IOC_READ | IOC_WRITE, u32::from(b'H'), nr, len)
        }

        #[test]
        fn hid_ioctl_request_coincide_con_oraculo_uapi() {
            assert_eq!(std::mem::size_of::<u64>(), 8);
            for len in [1u32, 2, 10, 23, 256, 0x3FFF] {
                let g = hid_ioctl_request(HIDIOCGFEATURE_NR, len as usize).unwrap();
                let s = hid_ioctl_request(HIDIOCSFEATURE_NR, len as usize).unwrap();
                assert_eq!(g, hid_ioc_feature(0x07, len), "HIDIOCGFEATURE({len})");
                assert_eq!(s, hid_ioc_feature(0x06, len), "HIDIOCSFEATURE({len})");
                // El request ioctl clásico cabe en 32 bits.
                assert!(u32::try_from(g).is_ok());
                assert!(u32::try_from(s).is_ok());
            }
            // Vectores ancla conocidos del UAPI genérico.
            assert_eq!(hid_ioc_feature(0x07, 23), 0xc017_4807);
            assert_eq!(hid_ioc_feature(0x06, 10), 0xc00a_4806);
        }
    }

    /// Targets sin ABI genérica acreditada: el oráculo no aplica (PRO-06).
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    #[test]
    fn hid_ioctl_request_target_sin_oraculo_uapi() {
        // Documenta la brecha: no fallar CI en arches experimentales, pero
        // deja constancia explícita de que PRO-06 no acredita este target.
        let arch = std::env::consts::ARCH;
        eprintln!("PRO-06: target {arch} sin oráculo UAPI genérico acreditado");
    }
}
