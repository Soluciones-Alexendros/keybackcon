//! Candado único de escritura HID (`/run/keybackcon/writer.lock`, ADR 13).
//!
//! Quien no puede abrir esa ruta de forma segura no abre el dispositivo.
//! No hay fallback a `/var/tmp` ni a un fichero `0666`.

use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

// SAFETY: flock y getgrnam son libc con ABI estable.
unsafe extern "C" {
    fn flock(fd: i32, op: i32) -> i32;
    fn getgrnam(name: *const i8) -> *mut Group;
}

const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;
/// `O_NOFOLLOW` en Linux (`asm-generic/fcntl.h`).
const O_NOFOLLOW: i32 = 0o400_000;

#[repr(C)]
struct Group {
    _gr_name: *mut i8,
    _gr_passwd: *mut i8,
    gr_gid: u32,
    _gr_mem: *mut *mut i8,
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
    PathBuf::from("/run/keybackcon/writer.lock")
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

/// El directorio instalado por `tmpfiles.d`: `root:keybackcon` y modo `0770`.
pub fn validate_installed_parent(parent: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(parent).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("no hay directorio de candado {}: {e}", parent.display()),
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
        return Err(denied(format!("{} debe ser modo 0770", parent.display())));
    }
    let gid = keybackcon_gid()?;
    if meta.gid() != gid {
        return Err(denied(format!(
            "{} no pertenece al grupo keybackcon",
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
        .map_err(|e| {
            io::Error::new(
                if e.kind() == io::ErrorKind::NotFound {
                    io::ErrorKind::NotFound
                } else {
                    io::ErrorKind::PermissionDenied
                },
                format!("no se pudo abrir el candado {}: {e}", path.display()),
            )
        })?;
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
            "{} debe ser root y modo 0660",
            path.display()
        )));
    }
    if meta.gid() != keybackcon_gid()? {
        return Err(denied(format!(
            "{} no pertenece al grupo keybackcon",
            path.display()
        )));
    }
    Ok(file)
}

fn keybackcon_gid() -> io::Result<u32> {
    let name = c"keybackcon";
    // SAFETY: el puntero es un literal C válido; el resultado no se retiene.
    let ptr = unsafe { getgrnam(name.as_ptr()) };
    if ptr.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "el grupo keybackcon no existe; instala el paquete (sysusers.d)",
        ));
    }
    // SAFETY: getgrnam devolvió un grupo; solo leemos gr_gid.
    Ok(unsafe { (*ptr).gr_gid })
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
        let dir = PathBuf::from(format!(
            "/tmp/keybackcon-writer-{}-{tag}",
            std::process::id()
        ));
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
        let dir = PathBuf::from(format!(
            "/tmp/keybackcon-writer-link-{}",
            std::process::id()
        ));
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
}
