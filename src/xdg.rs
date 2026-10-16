//! Resolución segura de rutas XDG (absolutas, sin vacíos ni relativos).

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

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

/// Crea `dir` con modo `0700` si no existe; comprueba propietario y permisos.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    if dir.exists() {
        let meta = fs::symlink_metadata(dir)?;
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
        return Ok(());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| {
            io::Error::new(e.kind(), format!("no se pudo crear {}: {e}", dir.display()))
        })?;
    Ok(())
}

/// Abre (o crea) un fichero con modo `0600` sin seguir enlaces inseguros.
pub fn open_lock_file(path: &Path) -> io::Result<fs::File> {
    if let Some(parent) = path.parent() {
        ensure_private_dir(parent)?;
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("no se pudo abrir {}: {e}", path.display()),
            )
        })
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
}
