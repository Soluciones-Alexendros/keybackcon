//! Resolución segura de rutas XDG (absolutas, sin vacíos ni relativos).

use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// `O_NOFOLLOW` en Linux. El componente final no se sigue.
const O_NOFOLLOW: i32 = 0o400_000;

/// Valida que `raw` sea una ruta absoluta usable; `None` si falta, está vacía
/// o es relativa.
pub fn absolute_env(raw: Result<String, std::env::VarError>) -> Option<PathBuf> {
    let s = raw.ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let p = PathBuf::from(trimmed);
    if !p.is_absolute() {
        return None;
    }
    Some(p)
}

/// `$XDG_RUNTIME_DIR` absoluto, o directorio privado por UID bajo `/tmp`.
pub fn runtime_base() -> PathBuf {
    if let Some(p) = absolute_env(std::env::var("XDG_RUNTIME_DIR")) {
        return p;
    }
    let uid = libc_geteuid();
    PathBuf::from(format!("/tmp/keybackcon-{uid}"))
}

/// `$XDG_STATE_HOME` absoluto, o `~/.local/state`, o error si no hay `HOME`.
pub fn state_base() -> io::Result<PathBuf> {
    if let Some(p) = absolute_env(std::env::var("XDG_STATE_HOME")) {
        return Ok(p);
    }
    let home = absolute_env(std::env::var("HOME")).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME ausente o no absoluto; no se puede resolver XDG_STATE_HOME",
        )
    })?;
    Ok(home.join(".local/state"))
}

/// Crea `dir` con modo `0700` si no existe. Un directorio ya presente tiene
/// que ser de este UID, no un enlace y no accesible por grupo ni por otros.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(meta) => validate_private_dir(dir, &meta),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(|err| {
                    io::Error::new(
                        err.kind(),
                        format!("no se pudo crear {}: {err}", dir.display()),
                    )
                })?;
            validate_private_dir(dir, &fs::symlink_metadata(dir)?)
        }
        Err(e) => Err(io::Error::new(
            e.kind(),
            format!("no se pudo inspeccionar {}: {e}", dir.display()),
        )),
    }
}

fn validate_private_dir(dir: &Path, meta: &fs::Metadata) -> io::Result<()> {
    if meta.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} es un enlace simbólico; se rechaza", dir.display()),
        ));
    }
    if !meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} existe y no es un directorio", dir.display()),
        ));
    }
    if meta.uid() != libc_geteuid() || meta.mode() & 0o777 != 0o700 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} debe pertenecer a este usuario y ser modo 0700",
                dir.display()
            ),
        ));
    }
    Ok(())
}

/// Abre (o crea) un fichero regular con modo `0600`, sin seguir el componente final.
pub fn open_lock_file(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        ensure_private_dir(parent)?;
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} es un enlace; se rechaza", path.display()),
            ));
        }
        if !meta.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} no es un fichero regular", path.display()),
            ));
        }
    }
    let existed = path.exists();
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("no se pudo abrir {}: {e}", path.display()),
            )
        })?;
    if !existed {
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("no se pudo fijar el modo de {}: {e}", path.display()),
                )
            })?;
    }
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != libc_geteuid() || meta.mode() & 0o777 != 0o600 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} debe ser un fichero regular de este usuario y modo 0600",
                path.display()
            ),
        ));
    }
    Ok(file)
}

// SAFETY: `geteuid` es libc estable y no tiene efectos laterales.
unsafe extern "C" {
    fn geteuid() -> u32;
}

fn libc_geteuid() -> u32 {
    // SAFETY: geteuid no toma argumentos y no falla.
    unsafe { geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_env_rechaza_vacio_y_relativo() {
        assert!(absolute_env(Ok(String::new())).is_none());
        assert!(absolute_env(Ok("   ".into())).is_none());
        assert!(absolute_env(Ok("relativo/foo".into())).is_none());
        assert_eq!(
            absolute_env(Ok("/tmp/abs".into())).as_deref(),
            Some(Path::new("/tmp/abs"))
        );
    }

    #[test]
    fn runtime_base_cae_a_tmp_por_uid_si_xdg_invalido() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: TEST_ENV_LOCK serializa el entorno de tests.
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
            std::env::set_var("XDG_RUNTIME_DIR", "relativo/no-vale");
        }
        let base = runtime_base();
        let s = base.to_string_lossy();
        assert!(
            s.starts_with("/tmp/keybackcon-"),
            "esperado fallback por UID, obtuvo {s}"
        );
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = PathBuf::from(format!("/tmp/keybackcon-xdg-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .expect("scratch");
        dir
    }

    #[test]
    fn t05_directorio_ajeno_o_abierto_se_rechaza() {
        let err = ensure_private_dir(Path::new("/usr")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");

        let dir = scratch("abierto");
        let abierto = dir.join("ancho");
        fs::DirBuilder::new().mode(0o755).create(&abierto).unwrap();
        fs::set_permissions(&abierto, fs::Permissions::from_mode(0o755)).unwrap();
        let err = ensure_private_dir(&abierto).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");

        let link = dir.join("enlace");
        std::os::unix::fs::symlink(&abierto, &link).unwrap();
        let err = ensure_private_dir(&link).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        assert!(err.to_string().contains("enlace"), "{err}");

        let propio = dir.join("privado");
        ensure_private_dir(&propio).unwrap();
        let meta = fs::symlink_metadata(&propio).unwrap();
        assert_eq!(meta.mode() & 0o777, 0o700);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn t06_open_lock_file_rechaza_enlace_y_modo() {
        let dir = scratch("fichero");
        let privado = dir.join("privado");
        ensure_private_dir(&privado).unwrap();

        let enlace = privado.join("candado");
        fs::write(privado.join("real"), b"x").unwrap();
        std::os::unix::fs::symlink(privado.join("real"), &enlace).unwrap();
        let err = open_lock_file(&enlace).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");
        assert!(err.to_string().contains("enlace"), "{err}");

        let ancho = privado.join("ancho");
        let creado = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&ancho)
            .unwrap();
        creado
            .set_permissions(fs::Permissions::from_mode(0o644))
            .unwrap();
        drop(creado);
        let err = open_lock_file(&ancho).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{err}");

        let bueno = open_lock_file(&privado.join("bueno")).unwrap();
        assert_eq!(bueno.metadata().unwrap().mode() & 0o777, 0o600);
        drop(bueno);
        let _ = fs::remove_dir_all(&dir);
    }
}
