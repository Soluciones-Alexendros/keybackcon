//! Estado persistente versionado en `$XDG_STATE_HOME/keybackcon/state`,
//! con lectura heredada de `kbd-rgb/state` y del formato plano `<hex> <pct>`.
//!
//! Las actualizaciones RMW (brillo, modo) toman un `flock` breve sobre
//! `state.lock` (KBC-03). No se mantiene durante el bucle de animación.
//! El identificador de ejecución de animación vive en el runtime y evita
//! que una limpieza obsoleta pise un `off` nuevo (KBC-05).

use std::fs::{self, File};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::color::{self, Rgb};
use crate::lamp::Lamp;
use crate::xdg;

// SAFETY: flock es libc con ABI estable.
unsafe extern "C" {
    fn flock(fd: i32, op: i32) -> i32;
}

const LOCK_EX: i32 = 2;
const LOCK_UN: i32 = 8;

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

/// Candado breve de transacciones RMW del estado (no del HID).
pub fn state_lock_file() -> io::Result<PathBuf> {
    Ok(state_base()?.join("keybackcon/state.lock"))
}

fn legacy_state_file() -> io::Result<PathBuf> {
    Ok(state_base()?.join("kbd-rgb/state"))
}

fn animation_run_file() -> PathBuf {
    xdg::runtime_base().join("keybackcon/animation.run")
}

/// `flock` exclusivo bloqueante sobre `state.lock`. Secciones críticas cortas.
#[derive(Debug)]
pub struct StateGuard {
    file: File,
}

impl StateGuard {
    /// Toma el candado de estado (bloquea hasta obtenerlo).
    pub fn acquire() -> io::Result<Self> {
        let path = state_lock_file()?;
        let file = xdg::open_lock_file(&path)?;
        // SAFETY: fd propio; LOCK_EX bloqueante para serializar RMW.
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
        if rc != 0 {
            let e = io::Error::last_os_error();
            return Err(io::Error::new(
                e.kind(),
                format!(
                    "no se pudo tomar el candado de estado {}: {e}",
                    path.display()
                ),
            ));
        }
        Ok(Self { file })
    }
}

impl Drop for StateGuard {
    fn drop(&mut self) {
        // SAFETY: liberar el flock del descriptor propio.
        unsafe {
            let _ = flock(self.file.as_raw_fd(), LOCK_UN);
        }
    }
}

/// Ejecuta `f` bajo el candado breve de estado.
pub fn with_state_txn<T>(f: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
    let _guard = StateGuard::acquire()?;
    f()
}

fn read_run_id() -> Option<u64> {
    let raw = fs::read_to_string(animation_run_file()).ok()?;
    raw.trim().parse().ok()
}

fn write_run_id(id: u64) -> io::Result<()> {
    let path = animation_run_file();
    if let Some(parent) = path.parent() {
        xdg::ensure_private_dir(parent)?;
    }
    let tmp = path.with_extension(format!("run-{}.tmp", std::process::id()));
    fs::write(&tmp, format!("{id}\n"))
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            io::Error::new(
                e.kind(),
                format!("no se pudo guardar {}: {e}", path.display()),
            )
        })
}

fn clear_run_id() {
    let _ = fs::remove_file(animation_run_file());
}

fn new_run_id() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    // Mezcla PID para distinguir procesos en el mismo nanosegundo.
    #[allow(clippy::cast_possible_truncation)]
    let low = nanos as u64;
    low ^ (u64::from(std::process::id()) << 32)
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
    with_state_txn(|| {
        let mut st = load_persisted()?;
        st.base_color = rgb;
        st.brightness = pct.clamp(0, 100);
        save_persisted(&st)
    })
}

/// Actualiza solo el modo (y power si aplica) tras una operación confirmada.
///
/// Invalida el identificador de ejecución de animación: un `off`/`fijar` nuevo
/// no debe ser pisado por la limpieza de una animación que ya terminó.
pub fn set_runtime_mode(mode: &str, power: &str) -> io::Result<()> {
    with_state_txn(|| {
        let mut st = load_persisted()?;
        st.mode = mode.to_string();
        st.power = power.to_string();
        save_persisted(&st)?;
        clear_run_id();
        Ok(())
    })
}

/// Publica el modo de animación solo cuando los recursos ya están adquiridos.
/// Devuelve el identificador de esta ejecución (KBC-04 / KBC-05).
pub fn begin_animation_mode(mode: &str) -> io::Result<u64> {
    with_state_txn(|| {
        let mut st = load_persisted()?;
        let run_id = new_run_id();
        st.mode = mode.to_string();
        st.power = "on".into();
        save_persisted(&st)?;
        write_run_id(run_id)?;
        Ok(run_id)
    })
}

/// Cierra la animación `run_id` hacia `fijar`/`on` si sigue siendo la vigente.
/// Si otro comando ya invalidó el id (p. ej. `off`), no modifica el estado.
pub fn end_animation_mode(run_id: u64) -> io::Result<()> {
    with_state_txn(|| {
        if read_run_id() != Some(run_id) {
            return Ok(());
        }
        let mut st = load_persisted()?;
        st.mode = "fijar".into();
        st.power = "on".into();
        save_persisted(&st)?;
        clear_run_id();
        Ok(())
    })
}

/// Desactiva autónomos, escribe el color escalado y guarda el estado.
pub fn apply(l: &mut Lamp, base: Rgb, pct: u32) -> io::Result<()> {
    l.autonomous(false)?;
    l.color(color::scale(base, pct))?;
    with_state_txn(|| {
        let mut st = load_persisted()?;
        st.base_color = base;
        st.brightness = pct.clamp(0, 100);
        st.power = "on".into();
        st.mode = "fijar".into();
        save_persisted(&st)?;
        clear_run_id();
        Ok(())
    })
}

/// Ajusta el brillo persistido bajo el candado de estado (RMW serializado).
pub fn update_brightness(
    change: impl FnOnce(u32) -> u32,
    power_on_if_off: bool,
) -> io::Result<(PersistedState, u32)> {
    with_state_txn(|| {
        let mut st = load_persisted()?;
        let new = change(st.brightness).clamp(0, 100);
        st.brightness = new;
        if power_on_if_off && st.power == "off" {
            st.power = "on".into();
            st.mode = "fijar".into();
        }
        save_persisted(&st)?;
        Ok((st, new))
    })
}

/// Indica si la restauración al login debe ejecutarse.
pub fn should_restore_on_login() -> io::Result<bool> {
    let st = load_persisted()?;
    Ok(st.restore_policy != "never")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;

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
    fn kbc05_limpieza_obsoleta_no_pisa_off() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = PathBuf::from(format!(
            "/tmp/keybackcon-state-kbc05-{}",
            std::process::id()
        ));
        let runtime = PathBuf::from(format!(
            "/tmp/keybackcon-runtime-kbc05-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&runtime);
        fs::create_dir_all(&tmp).unwrap();
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(runtime.join("keybackcon"))
            .unwrap();
        let prev_state = std::env::var_os("XDG_STATE_HOME");
        let prev_runtime = std::env::var_os("XDG_RUNTIME_DIR");
        // SAFETY: TEST_ENV_LOCK serializa el entorno.
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &tmp);
            std::env::set_var("XDG_RUNTIME_DIR", &runtime);
        }

        let run_id = begin_animation_mode("breathe").unwrap();
        let mid = load_persisted().unwrap();
        assert_eq!(mid.mode, "breathe");
        assert_eq!(mid.power, "on");

        // Un off nuevo invalida el identificador de ejecución.
        set_runtime_mode("fijar", "off").unwrap();
        assert_eq!(load_persisted().unwrap().power, "off");

        // La limpieza de la animación antigua no debe reescribir on.
        end_animation_mode(run_id).unwrap();
        let after = load_persisted().unwrap();
        assert_eq!(after.power, "off");
        assert_eq!(after.mode, "fijar");

        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&runtime);
        unsafe {
            match prev_state {
                Some(v) => std::env::set_var("XDG_STATE_HOME", v),
                None => std::env::remove_var("XDG_STATE_HOME"),
            }
            match prev_runtime {
                Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
                None => std::env::remove_var("XDG_RUNTIME_DIR"),
            }
        }
    }

    #[test]
    fn kbc03_update_brightness_serializa_dos_ups() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = PathBuf::from(format!(
            "/tmp/keybackcon-state-kbc03-txn-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &tmp);
        }
        let st = PersistedState {
            brightness: 50,
            ..Default::default()
        };
        save_persisted(&st).unwrap();
        update_brightness(|c| c.saturating_add(10), false).unwrap();
        update_brightness(|c| c.saturating_add(10), false).unwrap();
        assert_eq!(load_persisted().unwrap().brightness, 70);
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
