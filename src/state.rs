//! Estado persistente versionado en `$XDG_STATE_HOME/keybackcon/state`,
//! con lectura heredada de `kbd-rgb/state` y del formato plano `<hex> <pct>`.

use std::fs;
use std::io::{self, Write};
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

fn state_base() -> PathBuf {
    xdg::state_base().unwrap_or_else(|_| PathBuf::from("/var/tmp/keybackcon-state-fallback"))
}

/// Ruta del estado nuevo.
pub fn state_file() -> PathBuf {
    state_base().join("keybackcon/state")
}

fn legacy_state_file() -> PathBuf {
    state_base().join("kbd-rgb/state")
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
pub fn load_persisted() -> PersistedState {
    let raw = fs::read_to_string(state_file())
        .or_else(|_| fs::read_to_string(legacy_state_file()))
        .unwrap_or_default();
    if let Some(st) = parse_versioned(&raw) {
        return st;
    }
    if let Some((c, p)) = parse_legacy_line(&raw) {
        return PersistedState {
            base_color: c,
            brightness: p,
            ..PersistedState::default()
        };
    }
    PersistedState::default()
}

/// Carga `(color base, pct)` — compatibilidad con el API anterior.
pub fn load_state() -> (Rgb, u32) {
    let st = load_persisted();
    (st.base_color, st.brightness)
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
    let p = state_file();
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
#[allow(dead_code)] // API pública para GUI/scripts y tests.
pub fn save_state(rgb: Rgb, pct: u32) {
    let mut st = load_persisted();
    st.base_color = rgb;
    st.brightness = pct.clamp(0, 100);
    if let Err(e) = save_persisted(&st) {
        let _ = writeln!(io::stderr(), "aviso: {e}");
    }
}

/// Actualiza solo el modo (y power si aplica) tras una operación confirmada.
pub fn set_runtime_mode(mode: &str, power: &str) -> io::Result<()> {
    let mut st = load_persisted();
    st.mode = mode.to_string();
    st.power = power.to_string();
    save_persisted(&st)
}

/// Desactiva autónomos, escribe el color escalado y guarda el estado.
pub fn apply(l: &mut Lamp, base: Rgb, pct: u32) -> io::Result<()> {
    l.autonomous(false)?;
    l.color(color::scale(base, pct))?;
    let mut st = load_persisted();
    st.base_color = base;
    st.brightness = pct.clamp(0, 100);
    st.power = "on".into();
    st.mode = "fijar".into();
    save_persisted(&st)?;
    Ok(())
}

/// Indica si la restauración al login debe ejecutarse.
pub fn should_restore_on_login() -> bool {
    let st = load_persisted();
    st.restore_policy != "never"
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
        let (c, p) = load_state();
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
        save_state((255, 120, 0), 60);
        assert_eq!(load_state(), ((255, 120, 0), 60));
        let raw = fs::read_to_string(tmp.join("keybackcon/state")).unwrap();
        assert!(raw.contains("schema_version=1"));
        assert!(raw.contains("base_color=ff7800"));
        set_runtime_mode("breathe", "on").unwrap();
        let st = load_persisted();
        assert_eq!(st.mode, "breathe");
        let _ = fs::remove_dir_all(&tmp);
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }
}
