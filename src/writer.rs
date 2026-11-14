//! Candado único de escritura HID (`/run/ilutex/writer.lock`, ADR 13).
//!
//! Quien no puede abrir esa ruta de forma segura no abre el dispositivo.
//! No hay fallback a `/var/tmp` ni a un fichero `0666`.

use std::ffi::c_char;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

// SAFETY: flock/getgrnam/getgroups son libc con ABI estable.
unsafe extern "C" {
    fn flock(fd: i32, op: i32) -> i32;
    // c_char: en aarch64 es u8; un *const i8 no compila.
    fn getgrnam(name: *const c_char) -> *mut Group;
    fn getegid() -> u32;
    fn getgid() -> u32;
    fn getgroups(size: i32, list: *mut u32) -> i32;
}

const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;
/// `O_NOFOLLOW` en Linux (`asm-generic/fcntl.h`).
const O_NOFOLLOW: i32 = 0o400_000;

#[repr(C)]
struct Group {
    _gr_name: *mut c_char,
    _gr_passwd: *mut c_char,
    gr_gid: u32,
    _gr_mem: *mut *mut c_char,
}

/// `flock` exclusivo sobre el candado de escritura. Al soltar el valor se libera.
#[derive(Debug)]
pub struct WriterGuard {
    file: File,
}

impl WriterGuard {
    /// Toma el candado sin esperar. Si otro proceso lo tiene, falla y no escribe.
    pub fn acquire() -> io::Result<Self> {
        let path = writer_lock_path();
        let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("candado sin directorio: {}", path.display()),
            ));
        };
        if test_override() {
            reject_symlink_dir(parent)?;
        } else {
            validate_installed_parent(parent)?;
        }
        let file = open_writer_file(&path, test_override())?;
        // SAFETY: fd del fichero que acabamos de abrir; LOCK_EX|LOCK_NB.
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
        if rc != 0 {
            let e = io::Error::last_os_error();
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "ya hay una escritura en el teclado (candado {}): {e}",
                    path.display()
                ),
            ));
        }
        Ok(Self { file })
    }
}

impl Drop for WriterGuard {
    fn drop(&mut self) {
        // SAFETY: liberar el flock del descriptor propio.
        unsafe {
            let _ = flock(self.file.as_raw_fd(), LOCK_UN);
        }
    }
}

/// Ruta del candado. En tests, `KEYBACKCON_WRITER_LOCK` absoluto sustituye a `/run`.
pub fn writer_lock_path() -> PathBuf {
    #[cfg(test)]
    if let Some(p) = test_lock_override() {
        return p;
    }
    PathBuf::from("/run/ilutex/writer.lock")
}

fn test_override() -> bool {
    #[cfg(test)]
    {
        test_lock_override().is_some()
    }
    #[cfg(not(test))]
    {
        false
    }
}

#[cfg(test)]
fn test_lock_override() -> Option<PathBuf> {
    let raw = std::env::var_os("KEYBACKCON_WRITER_LOCK")?;
    let p = PathBuf::from(raw);
    if p.is_absolute() {
        Some(p)
    } else {
        None
    }
}

/// El directorio instalado por `tmpfiles.d`: `root:ilutex` y modo `0770`.
///
/// Contrato acoplado al fichero `0660` (KBC-01/KBC-02): no cambiar un modo
/// sin el otro ni sin actualizar esta validación y sus pruebas.
pub fn validate_installed_parent(parent: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(parent).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no hay directorio de candado {}: {e}. Reinstala el paquete (tmpfiles.d/sysusers.d)",
                parent.display()
            ),
        )
    })?;
    if meta.file_type().is_symlink() {
        return Err(denied(format!(
            "{} es un enlace; se rechaza el candado",
            parent.display()
        )));
    }
    if !meta.is_dir() {
        return Err(denied(format!("{} no es un directorio", parent.display())));
    }
    if meta.uid() != 0 {
        return Err(denied(format!("{} no pertenece a root", parent.display())));
    }
    if meta.mode() & 0o777 != 0o770 {
        return Err(denied(format!(
            "{} debe ser modo 0770 (contrato tmpfiles + escritor)",
            parent.display()
        )));
    }
    let gid = ilutex_gid()?;
    if meta.gid() != gid {
        return Err(denied(format!(
            "{} no pertenece al grupo ilutex",
            parent.display()
        )));
    }
    Ok(())
}

fn reject_symlink_dir(parent: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(parent).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("no hay directorio de candado {}: {e}", parent.display()),
        )
    })?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(denied(format!(
            "{} no es un directorio seguro",
            parent.display()
        )));
    }
    Ok(())
}

fn open_writer_file(path: &Path, relaxed: bool) -> io::Result<File> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(denied(format!(
                "{} es un enlace; se rechaza",
                path.display()
            )));
        }
        if !meta.file_type().is_file() {
            return Err(denied(format!(
                "{} no es un fichero regular",
                path.display()
            )));
        }
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|e| diagnose_open_failure(path, &e, relaxed))?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(denied(format!(
            "{} no es un fichero regular",
            path.display()
        )));
    }
    let mode = meta.mode() & 0o777;
    if relaxed {
        if mode & 0o007 != 0 {
            return Err(denied(format!(
                "{} es accesible por otros; se rechaza",
                path.display()
            )));
        }
        return Ok(file);
    }
    if meta.uid() != 0 || mode != 0o660 {
        return Err(denied(format!(
            "{} debe ser root:ilutex modo 0660 (contrato tmpfiles + escritor)",
            path.display()
        )));
    }
    if meta.gid() != ilutex_gid()? {
        return Err(denied(format!(
            "{} no pertenece al grupo ilutex",
            path.display()
        )));
    }
    Ok(file)
}

/// Pista unificada cuando `open` del candado falla (instalar ≠ autorizar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockAccessHint {
    GroupMissing,
    NotAuthorized,
    StillDenied,
}

fn lock_access_hint(gid: &io::Result<u32>) -> LockAccessHint {
    match gid {
        Err(_) => LockAccessHint::GroupMissing,
        Ok(g) if user_has_gid(*g) => LockAccessHint::StillDenied,
        Ok(_) => LockAccessHint::NotAuthorized,
    }
}

fn format_open_denied(path: &Path, hint: LockAccessHint, os: &io::Error) -> String {
    match hint {
        LockAccessHint::GroupMissing => format!(
            "no se pudo abrir el candado {}: {os}. El grupo ilutex no existe; \
             instala el paquete (sysusers.d/tmpfiles.d) antes de escribir",
            path.display()
        ),
        LockAccessHint::NotAuthorized => format!(
            "no se pudo abrir el candado {}: permiso denegado. \
             Instalar no autoriza: añade tu usuario al grupo con \
             `sudo usermod -aG ilutex \"$USER\"` y abre una sesión nueva \
             (o `newgrp ilutex`). Error del sistema: {os}",
            path.display()
        ),
        LockAccessHint::StillDenied => format!(
            "no se pudo abrir el candado {}: permiso denegado pese a pertenecer \
             al grupo ilutex. Comprueba que el fichero sea root:ilutex \
             modo 0660 y el directorio /run/ilutex modo 0770. Error: {os}",
            path.display()
        ),
    }
}

fn diagnose_open_failure(path: &Path, e: &io::Error, relaxed: bool) -> io::Error {
    if e.kind() == io::ErrorKind::NotFound {
        return io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "no existe el candado {}. Reinstala el paquete (tmpfiles.d) o ejecuta \
                 `sudo systemd-tmpfiles --create ilutex.conf`. Error: {e}",
                path.display()
            ),
        );
    }
    if relaxed {
        return denied(format!(
            "no se pudo abrir el candado {}: {e}",
            path.display()
        ));
    }
    denied(format_open_denied(path, lock_access_hint(&ilutex_gid()), e))
}

fn ilutex_gid() -> io::Result<u32> {
    let name = c"ilutex";
    // SAFETY: el puntero es un literal C válido; el resultado no se retiene.
    let ptr = unsafe { getgrnam(name.as_ptr()) };
    if ptr.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "el grupo ilutex no existe; instala el paquete (sysusers.d)",
        ));
    }
    // SAFETY: getgrnam devolvió un grupo; solo leemos gr_gid.
    Ok(unsafe { (*ptr).gr_gid })
}

/// ¿El proceso actual tiene el GID (real, efectivo o suplementario)?
fn user_has_gid(gid: u32) -> bool {
    // SAFETY: getgid/getegid no toman argumentos y solo leen credenciales.
    if unsafe { getgid() } == gid || unsafe { getegid() } == gid {
        return true;
    }
    // SAFETY: size 0 consulta la longitud; el segundo getgroups rellena el buffer.
    let n = unsafe { getgroups(0, std::ptr::null_mut()) };
    let Ok(len) = usize::try_from(n) else {
        return false;
    };
    if len == 0 {
        return false;
    }
    let mut buf = vec![0u32; len];
    let filled = unsafe { getgroups(n, buf.as_mut_ptr()) };
    let Ok(filled_len) = usize::try_from(filled) else {
        return false;
    };
    if filled_len == 0 {
        return false;
    }
    buf[..filled_len].contains(&gid)
}

fn denied(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::sync::MutexGuard;

    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        prev: Option<std::ffi::OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: TEST_ENV_LOCK sigue tomado; restauramos el entorno del proceso.
            unsafe {
                match self.prev.take() {
                    Some(v) => std::env::set_var("KEYBACKCON_WRITER_LOCK", v),
                    None => std::env::remove_var("KEYBACKCON_WRITER_LOCK"),
                }
            }
        }
    }

    fn lock_env(tag: &str) -> (PathBuf, EnvGuard) {
        let guard = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = PathBuf::from(format!("/tmp/ilutex-writer-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("writer.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o660)
            .open(&path)
            .unwrap();
        file.set_permissions(fs::Permissions::from_mode(0o660))
            .unwrap();
        let prev = std::env::var_os("KEYBACKCON_WRITER_LOCK");
        // SAFETY: TEST_ENV_LOCK serializa el entorno de tests.
        unsafe {
            std::env::set_var("KEYBACKCON_WRITER_LOCK", &path);
        }
        (dir, EnvGuard { _lock: guard, prev })
    }

    fn hijo(mode: &str, lock: &Path) -> std::process::Output {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "writer::tests::writer_hijo_candado"])
            .env("KEYBACKCON_LOCK_CHILD", mode)
            .env("KEYBACKCON_WRITER_LOCK", lock)
            .env_remove("RUST_TEST_THREADS")
            .output()
            .expect("hijo de candado")
    }

    #[test]
    fn writer_hijo_candado() {
        match std::env::var("KEYBACKCON_LOCK_CHILD").as_deref() {
            Ok("fail") => {
                let busy = WriterGuard::acquire().is_err();
                std::process::exit(if busy { 2 } else { 0 });
            }
            Ok("ok") => {
                let free = WriterGuard::acquire().is_ok();
                std::process::exit(if free { 0 } else { 2 });
            }
            _ => {}
        }
    }

    #[test]
    fn t01_t02_t03_el_segundo_proceso_no_escribe() {
        let (dir, _env) = lock_env("procs");
        let lock = dir.join("writer.lock");
        let held = WriterGuard::acquire().expect("primer escritor");
        // El hijo corre antes de soltar `held`: la exclusión y la sección crítica coinciden.
        let ocupado = hijo("fail", &lock);
        assert_eq!(
            ocupado.status.code(),
            Some(2),
            "T-01/T-03: el segundo proceso debe fallar\n{}",
            String::from_utf8_lossy(&ocupado.stderr)
        );
        drop(held);
        let libre = hijo("ok", &lock);
        assert!(
            libre.status.success(),
            "tras soltar el candado el otro proceso entra\n{}",
            String::from_utf8_lossy(&libre.stderr)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn t06_enlace_en_el_candado_se_rechaza() {
        let (dir, _env) = lock_env("enlace");
        let lock = dir.join("writer.lock");
        fs::remove_file(&lock).unwrap();
        fs::write(dir.join("real"), b"x").unwrap();
        std::os::unix::fs::symlink(dir.join("real"), &lock).unwrap();
        let err = WriterGuard::acquire().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        assert!(err.to_string().contains("enlace"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn candado_demasiado_abierto_se_rechaza() {
        let (dir, _env) = lock_env("abierto");
        let lock = dir.join("writer.lock");
        fs::set_permissions(&lock, fs::Permissions::from_mode(0o666)).unwrap();
        let err = WriterGuard::acquire().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn directorio_de_candado_ajeno_se_rechaza() {
        let err = validate_installed_parent(Path::new("/tmp")).unwrap_err();
        assert!(
            err.kind() == io::ErrorKind::PermissionDenied || err.kind() == io::ErrorKind::NotFound,
            "{err}"
        );
        assert!(!err.to_string().contains("/var/tmp"), "{err}");
    }

    #[test]
    fn directorio_enlace_de_candado_se_rechaza() {
        let dir = PathBuf::from(format!("/tmp/ilutex-writer-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real");
        fs::create_dir(&real).unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = validate_installed_parent(&link).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostico_unificado_instalar_no_autoriza() {
        let path = Path::new("/run/ilutex/writer.lock");
        let os = io::Error::from_raw_os_error(13); // EACCES
        let msg = format_open_denied(path, LockAccessHint::NotAuthorized, &os);
        assert!(msg.contains("Instalar no autoriza"), "{msg}");
        assert!(msg.contains("usermod -aG ilutex"), "{msg}");
        assert!(
            msg.contains("sesión nueva") || msg.contains("newgrp"),
            "{msg}"
        );

        let missing = format_open_denied(path, LockAccessHint::GroupMissing, &os);
        assert!(missing.contains("grupo ilutex no existe"), "{missing}");

        let still = format_open_denied(path, LockAccessHint::StillDenied, &os);
        assert!(still.contains("0660"), "{still}");
        assert!(still.contains("0770"), "{still}");
    }

    #[test]
    fn pista_grupo_ausente_sin_getgrnam() {
        let gid = Err(io::Error::new(io::ErrorKind::NotFound, "sin grupo"));
        let hint = lock_access_hint(&gid);
        assert_eq!(hint, LockAccessHint::GroupMissing);
    }

    #[test]
    fn user_has_gid_reconoce_egid() {
        let egid = unsafe { getegid() };
        assert!(user_has_gid(egid), "el egid del proceso debe coincidir");
        assert!(!user_has_gid(u32::MAX), "GID imposible no debe coincidir");
    }
}
