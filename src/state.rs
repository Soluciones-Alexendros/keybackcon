//! Estado persistente versionado en `$XDG_STATE_HOME/keybackcon/state`,
//! con lectura heredada de `kbd-rgb/state` y del formato plano `<hex> <pct>`.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::color::{self, Rgb};
use crate::lamp::Lamp;
use crate::xdg;

/// Versión del esquema de persistencia.
pub const SCHEMA_VERSION: u32 = 1;

/// Instantánea persistida (preferencias + modo de ejecución conocido).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedState {
    pub base_color: Rgb,
    pub brightness: u32,
    /// `on` | `off`
    pub power: String,
    /// `fijar` | `breathe` | `rainbow` | `firmware` | `unknown`
    pub mode: String,
    /// `user` | `never` — política de restauración al login.
    pub restore_policy: String,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            base_color: (255, 255, 255),
            brightness: 100,
            power: "on".into(),
            mode: "fijar".into(),
            restore_policy: "user".into(),
        }
    }
}

fn state_base() -> io::Result<PathBuf> {
    xdg::state_base()
}

/// Ruta del estado nuevo. Falla si no hay `HOME` ni `XDG_STATE_HOME` absolutos.
pub fn state_file() -> io::Result<PathBuf> {
    Ok(state_base()?.join("keybackcon/state"))
}

fn legacy_state_file() -> io::Result<PathBuf> {
    Ok(state_base()?.join("kbd-rgb/state"))
}

fn parse_legacy_line(raw: &str) -> Option<(Rgb, u32)> {
    let mut it = raw.split_whitespace();
    let c = it.next().and_then(|s| color::parse_color(s).ok())?;
    let p = it
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100u32)
        .clamp(0, 100);
    Some((c, p))
}

fn parse_versioned(raw: &str) -> Option<PersistedState> {
    let mut st = PersistedState::default();
    let mut saw_version = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "schema_version" => {
                    let ver: u32 = v.trim().parse().ok()?;
                    if ver == 0 || ver > SCHEMA_VERSION {
                        return None;
                    }
                    saw_version = true;
                }
                "base_color" => st.base_color = color::parse_color(v.trim()).ok()?,
                "brightness" => {
                    st.brightness = v.trim().parse::<u32>().ok()?.clamp(0, 100);
                }
                "power" => st.power = v.trim().to_string(),
                "mode" => st.mode = v.trim().to_string(),
                "restore_policy" => st.restore_policy = v.trim().to_string(),
                _ => {}
            }
        }
    }
    saw_version.then_some(st)
}

/// Carga el estado completo; migra el formato plano si hace falta.
///
/// Un fichero ausente es el estado por defecto. Un entorno sin ruta XDG
/// absoluta es un error: no se usa `/var/tmp`.
pub fn load_persisted() -> io::Result<PersistedState> {
    let primary = state_file()?;
    let raw = match fs::read_to_string(&primary) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            match fs::read_to_string(legacy_state_file()?) {
                Ok(s) => s,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Ok(PersistedState::default());
                }
                Err(e) => return Err(e),
            }
        }
        Err(e) => return Err(e),
    };
    if let Some(st) = parse_versioned(&raw) {
        return Ok(st);
    }
    if let Some((c, p)) = parse_legacy_line(&raw) {
        return Ok(PersistedState {
            base_color: c,
            brightness: p,
            ..PersistedState::default()
        });
    }
    Ok(PersistedState::default())
}

/// Carga `(color base, pct)` — compatibilidad con el API anterior.
pub fn load_state() -> io::Result<(Rgb, u32)> {
    let st = load_persisted()?;
    Ok((st.base_color, st.brightness))
}

fn serialize(st: &PersistedState) -> String {
    format!(
        "schema_version={}\nbase_color={}\nbrightness={}\npower={}\nmode={}\nrestore_policy={}\n",
        SCHEMA_VERSION,
        color::to_hex(st.base_color),
        st.brightness,
        st.power,
        st.mode,
        st.restore_policy,
    )
}

/// Persiste el estado de forma atómica. Devuelve error si falla la escritura.
pub fn save_persisted(st: &PersistedState) -> io::Result<()> {
    let p = state_file()?;
    let Some(d) = p.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ruta de estado sin directorio padre",
        ));
    };
    xdg::ensure_private_dir(d)?;
    let tmp = d.join(format!(".state-{}.tmp", std::process::id()));
    let content = serialize(st);
    fs::write(&tmp, &content)
        .and_then(|()| fs::rename(&tmp, &p))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            io::Error::new(
                e.kind(),
                format!("no se pudo guardar el estado en {}: {e}", p.display()),
            )
        })?;
    Ok(())
}

/// Persiste color base y porcentaje (conserva modo/power/restore).
#[allow(dead_code)] // API de tests y de quien enlace el binario como librería de comandos.
pub fn save_state(rgb: Rgb, pct: u32) -> io::Result<()> {
    let mut st = load_persisted()?;
    st.base_color = rgb;
    st.brightness = pct.clamp(0, 100);
    save_persisted(&st)
}

/// Actualiza solo el modo (y power si aplica) tras una operación confirmada.
pub fn set_runtime_mode(mode: &str, power: &str) -> io::Result<()> {
    let mut st = load_persisted()?;
    st.mode = mode.to_string();
    st.power = power.to_string();
    save_persisted(&st)
}

/// Desactiva autónomos, escribe el color escalado y guarda el estado.
pub fn apply(l: &mut Lamp, base: Rgb, pct: u32) -> io::Result<()> {
    l.autonomous(false)?;
    l.color(color::scale(base, pct))?;
    let mut st = load_persisted()?;
    st.base_color = base;
    st.brightness = pct.clamp(0, 100);
    st.power = "on".into();
    st.mode = "fijar".into();
    save_persisted(&st)?;
    Ok(())
}

/// Indica si la restauración al login debe ejecutarse.
pub fn should_restore_on_login() -> io::Result<bool> {
    let st = load_persisted()?;
    Ok(st.restore_policy != "never")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_migra_desde_legado() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = PathBuf::from(format!("/tmp/keybackcon-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("kbd-rgb")).unwrap();
        fs::write(tmp.join("kbd-rgb/state"), "ff7800 60\n").unwrap();
        // SAFETY: TEST_ENV_LOCK serializa el entorno.
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &tmp);
        }
        let (c, p) = load_state().unwrap();
        assert_eq!((c, p), ((255, 120, 0), 60));
        let _ = fs::remove_dir_all(&tmp);
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }

    #[test]
    fn save_state_es_atómico_y_versionado() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = PathBuf::from(format!("/tmp/keybackcon-state-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &tmp);
        }
        save_state((255, 120, 0), 60).unwrap();
        assert_eq!(load_state().unwrap(), ((255, 120, 0), 60));
        let raw = fs::read_to_string(tmp.join("keybackcon/state")).unwrap();
        assert!(raw.contains("schema_version=1"));
        assert!(raw.contains("base_color=ff7800"));
        set_runtime_mode("breathe", "on").unwrap();
        let st = load_persisted().unwrap();
        assert_eq!(st.mode, "breathe");
        let _ = fs::remove_dir_all(&tmp);
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }

    #[test]
    fn t07_sin_xdg_no_hay_fallback_compartido() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prev_home = std::env::var_os("HOME");
        let prev_xdg = std::env::var_os("XDG_STATE_HOME");
        // SAFETY: TEST_ENV_LOCK serializa el entorno; se restaura al salir.
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
            std::env::set_var("HOME", "relativo/no-absoluto");
        }
        let err = state_file().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound, "{err}");
        assert!(
            !err.to_string().contains("/var/tmp"),
            "no debe aparecer un fallback compartido: {err}"
        );
        unsafe {
            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match prev_xdg {
                Some(v) => std::env::set_var("XDG_STATE_HOME", v),
                None => std::env::remove_var("XDG_STATE_HOME"),
            }
        }
    }
}
