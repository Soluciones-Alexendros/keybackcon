//! Estado persistente versionado en `$XDG_STATE_HOME/ilutex/state`,
//! con lectura heredada de `kbd-rgb/state` y del formato plano `<hex> <pct>`.
//!
//! Las actualizaciones RMW (brillo, modo) toman un `flock` breve sobre
//! `state.lock` (KBC-03). No se mantiene durante el bucle de animación.
//! El identificador de ejecución de animación vive en el runtime y evita
//! que una limpieza obsoleta pise un `off` nuevo (KBC-05).

use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `O_NOFOLLOW` en Linux: el temporal de estado no sigue enlaces.
const O_NOFOLLOW: i32 = 0o400_000;

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
    Ok(state_base()?.join("ilutex/state"))
}

/// Candado breve de transacciones RMW del estado (no del HID).
pub fn state_lock_file() -> io::Result<PathBuf> {
    Ok(state_base()?.join("ilutex/state.lock"))
}

/// Ficheros de estado anteriores al nombre `ilutex`; se leen si el actual no existe.
fn legacy_state_files() -> io::Result<Vec<PathBuf>> {
    let base = state_base()?;
    Ok(vec![
        base.join("keybackcon/state"),
        base.join("kbd-rgb/state"),
    ])
}

fn animation_run_file() -> PathBuf {
    xdg::runtime_base().join("ilutex/animation.run")
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

/// Resultado del oráculo de parseo compartido con la GUI (KBC-12 / KBC-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateParseOutcome {
    /// Instantánea válida (versionada o legado plano).
    Ok(PersistedState),
    /// Contenido irreconocible o truncado; el fichero no se reescribe.
    Corrupt,
    /// `schema_version` futura o nula; el fichero no se reescribe.
    UnsupportedVersion(u32),
}

fn parse_brightness(raw: &str) -> Option<u32> {
    let n: i64 = raw.trim().parse().ok()?;
    Some(u32::try_from(n.clamp(0, 100)).unwrap_or(0))
}

fn looks_versioned(raw: &str) -> bool {
    raw.lines().any(|line| {
        let line = line.trim();
        !line.is_empty()
            && !line.starts_with('#')
            && line
                .split_once('=')
                .is_some_and(|(k, _)| k.trim() == "schema_version")
    })
}

fn parse_legacy_line(raw: &str) -> Option<(Rgb, u32)> {
    let mut it = raw.split_whitespace();
    let c = it.next().and_then(|s| color::parse_color(s).ok())?;
    let p = it.next().map_or(Some(100u32), parse_brightness)?;
    Some((c, p))
}

fn parse_versioned(raw: &str) -> StateParseOutcome {
    let mut st = PersistedState::default();
    let mut saw_version = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k.trim() {
            "schema_version" => {
                let Ok(ver) = v.trim().parse::<u32>() else {
                    return StateParseOutcome::Corrupt;
                };
                if ver == 0 || ver > SCHEMA_VERSION {
                    return StateParseOutcome::UnsupportedVersion(ver);
                }
                saw_version = true;
            }
            "base_color" => match color::parse_color(v.trim()) {
                Ok(c) => st.base_color = c,
                Err(_) => return StateParseOutcome::Corrupt,
            },
            "brightness" => match parse_brightness(v) {
                Some(p) => st.brightness = p,
                None => return StateParseOutcome::Corrupt,
            },
            "power" => st.power = v.trim().to_string(),
            "mode" => st.mode = v.trim().to_string(),
            "restore_policy" => st.restore_policy = v.trim().to_string(),
            _ => {}
        }
    }
    if saw_version {
        StateParseOutcome::Ok(st)
    } else {
        StateParseOutcome::Corrupt
    }
}

/// Oráculo de texto compartido con `gui/client.py` (KBC-16).
pub fn parse_state_text(raw: &str) -> StateParseOutcome {
    if looks_versioned(raw) {
        return parse_versioned(raw);
    }
    if let Some((c, p)) = parse_legacy_line(raw) {
        return StateParseOutcome::Ok(PersistedState {
            base_color: c,
            brightness: p,
            ..PersistedState::default()
        });
    }
    StateParseOutcome::Corrupt
}

fn parse_error(kind: &str, path: &Path, detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "estado {kind} en {}; se conserva el original ({detail})",
            path.display()
        ),
    )
}

fn outcome_to_result(path: &Path, outcome: StateParseOutcome) -> io::Result<PersistedState> {
    match outcome {
        StateParseOutcome::Ok(st) => Ok(st),
        StateParseOutcome::Corrupt => Err(parse_error("corrupto", path, "contenido irreconocible")),
        StateParseOutcome::UnsupportedVersion(v) => Err(parse_error(
            "con esquema no soportado",
            path,
            format!("schema_version={v}"),
        )),
    }
}

/// Carga el estado completo; migra el formato plano si hace falta.
///
/// Ausencia de fichero → defaults. Corrupción o esquema futuro → error y el
/// original se conserva (KBC-12). Sin ruta XDG absoluta → error (no `/var/tmp`).
pub fn load_persisted() -> io::Result<PersistedState> {
    let primary = state_file()?;
    match fs::read_to_string(&primary) {
        Ok(raw) => outcome_to_result(&primary, parse_state_text(&raw)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            for legacy in legacy_state_files()? {
                match fs::read_to_string(&legacy) {
                    Ok(raw) => return outcome_to_result(&legacy, parse_state_text(&raw)),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(PersistedState::default())
        }
        Err(e) => Err(e),
    }
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

/// Quita un temporal residual solo si es fichero regular (nunca un symlink).
fn remove_stale_tmp(tmp: &Path) -> io::Result<()> {
    match fs::symlink_metadata(tmp) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(meta) if meta.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} es un enlace; se rechaza el temporal", tmp.display()),
        )),
        Ok(meta) if meta.file_type().is_file() => fs::remove_file(tmp),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} no es un fichero regular", tmp.display()),
        )),
    }
}

/// Escribe `content` en `path` vía temporal exclusivo `0600` + `rename` (KBC-15).
fn write_state_atomic(path: &Path, content: &str) -> io::Result<()> {
    let Some(d) = path.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ruta de estado sin directorio padre",
        ));
    };
    xdg::ensure_private_dir(d)?;
    let tmp = d.join(format!(".state-{}.tmp", std::process::id()));
    remove_stale_tmp(&tmp)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW)
        .open(&tmp)
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "no se pudo crear el temporal exclusivo {}: {e}",
                    tmp.display()
                ),
            )
        })?;
    let write_result = (|| {
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(io::Error::new(
            e.kind(),
            format!("no se pudo guardar el estado en {}: {e}", path.display()),
        ));
    }
    Ok(())
}

/// Persiste el estado de forma atómica. Devuelve error si falla la escritura.
pub fn save_persisted(st: &PersistedState) -> io::Result<()> {
    write_state_atomic(&state_file()?, &serialize(st))
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
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    #[test]
    fn state_migra_desde_legado() {
        let mut env = crate::EnvGuard::lock();
        let tmp = PathBuf::from(format!("/tmp/ilutex-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("kbd-rgb")).unwrap();
        fs::write(tmp.join("kbd-rgb/state"), "ff7800 60\n").unwrap();
        env.set("XDG_STATE_HOME", &tmp);
        let (c, p) = load_state().unwrap();
        assert_eq!((c, p), ((255, 120, 0), 60));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn save_state_es_atómico_y_versionado() {
        let mut env = crate::EnvGuard::lock();
        let tmp = PathBuf::from(format!("/tmp/ilutex-state-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        env.set("XDG_STATE_HOME", &tmp);
        save_state((255, 120, 0), 60).unwrap();
        assert_eq!(load_state().unwrap(), ((255, 120, 0), 60));
        let raw = fs::read_to_string(tmp.join("ilutex/state")).unwrap();
        assert!(raw.contains("schema_version=1"));
        assert!(raw.contains("base_color=ff7800"));
        set_runtime_mode("breathe", "on").unwrap();
        let st = load_persisted().unwrap();
        assert_eq!(st.mode, "breathe");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn kbc05_limpieza_obsoleta_no_pisa_off() {
        let mut env = crate::EnvGuard::lock();
        let tmp = PathBuf::from(format!("/tmp/ilutex-state-kbc05-{}", std::process::id()));
        let runtime = PathBuf::from(format!("/tmp/ilutex-runtime-kbc05-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&runtime);
        fs::create_dir_all(&tmp).unwrap();
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(runtime.join("ilutex"))
            .unwrap();
        env.set("XDG_STATE_HOME", &tmp);
        env.set("XDG_RUNTIME_DIR", &runtime);

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
    }

    #[test]
    fn kbc03_update_brightness_serializa_dos_ups() {
        let mut env = crate::EnvGuard::lock();
        let tmp = PathBuf::from(format!(
            "/tmp/ilutex-state-kbc03-txn-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        env.set("XDG_STATE_HOME", &tmp);
        let st = PersistedState {
            brightness: 50,
            ..Default::default()
        };
        save_persisted(&st).unwrap();
        update_brightness(|c| c.saturating_add(10), false).unwrap();
        update_brightness(|c| c.saturating_add(10), false).unwrap();
        assert_eq!(load_persisted().unwrap().brightness, 70);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn kbc12_corrupcion_y_esquema_futuro_conservan_original() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = PathBuf::from(format!("/tmp/ilutex-state-kbc12-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let dir = tmp.join("ilutex");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state");
        let trunc = "schema_version=1\nbase_color=\nbrightness=40\n";
        fs::write(&path, trunc).unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &tmp);
        }
        let err = load_persisted().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
        assert!(err.to_string().contains("corrupto"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), trunc);

        let future = "schema_version=99\nbase_color=112233\nbrightness=50\n";
        fs::write(&path, future).unwrap();
        let err = load_persisted().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
        assert!(err.to_string().contains("schema_version=99"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), future);

        let _ = fs::remove_dir_all(&tmp);
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }

    #[test]
    fn kbc15_temporal_exclusivo_rechaza_symlink_y_limpia_previo() {
        let _env = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let tmp = PathBuf::from(format!("/tmp/ilutex-state-kbc15-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &tmp);
        }
        let st = PersistedState {
            base_color: (1, 2, 3),
            brightness: 42,
            ..Default::default()
        };
        save_persisted(&st).unwrap();
        let state_path = tmp.join("ilutex/state");
        let meta = fs::metadata(&state_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);

        let tmp_path = tmp.join(format!("ilutex/.state-{}.tmp", std::process::id()));
        fs::write(&tmp_path, b"stale").unwrap();
        save_persisted(&st).unwrap();
        assert!(!tmp_path.exists());

        fs::write(&tmp_path, b"x").unwrap();
        fs::remove_file(&tmp_path).unwrap();
        std::os::unix::fs::symlink("/tmp/ilutex-kbc15-nowhere", &tmp_path).unwrap();
        let err = save_persisted(&st).unwrap_err();
        assert!(
            err.to_string().contains("enlace") || err.kind() == io::ErrorKind::PermissionDenied,
            "{err}"
        );
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_dir_all(&tmp);
        unsafe {
            std::env::remove_var("XDG_STATE_HOME");
        }
    }

    #[test]
    fn kbc16_oraculo_compartido_testdata() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/state");
        let oracle = fs::read_to_string(root.join("oracle.txt")).unwrap();
        for line in oracle.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let kind = parts.next().unwrap();
            let file = parts.next().unwrap();
            let raw = fs::read_to_string(root.join(file)).unwrap();
            let outcome = parse_state_text(&raw);
            match kind {
                "ok" => {
                    let color = parts.next().unwrap();
                    let brightness: u32 = parts.next().unwrap().parse().unwrap();
                    let StateParseOutcome::Ok(st) = outcome else {
                        panic!("{file}: esperado ok, got {outcome:?}");
                    };
                    assert_eq!(color::to_hex(st.base_color), color, "{file}");
                    assert_eq!(st.brightness, brightness, "{file}");
                }
                "corrupt" => assert_eq!(outcome, StateParseOutcome::Corrupt, "{file}"),
                "unsupported" => {
                    let version: u32 = parts.next().unwrap().parse().unwrap();
                    assert_eq!(
                        outcome,
                        StateParseOutcome::UnsupportedVersion(version),
                        "{file}"
                    );
                }
                other => panic!("kind desconocido {other}"),
            }
        }
    }

    #[test]
    fn t07_sin_xdg_no_hay_fallback_compartido() {
        let mut env = crate::EnvGuard::lock();
        env.remove("XDG_STATE_HOME");
        env.set("HOME", "relativo/no-absoluto");
        let err = state_file().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound, "{err}");
        assert!(
            !err.to_string().contains("/var/tmp"),
            "no debe aparecer un fallback compartido: {err}"
        );
    }
}
