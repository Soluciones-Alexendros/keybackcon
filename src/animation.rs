//! Animación en primer plano (`breathe`/`rainbow`), exclusión por descriptor
//! y parada tipada (ADR 13).
//!
//! El derecho de escritura lo da un `flock` exclusivo sobre
//! `$XDG_RUNTIME_DIR/keybackcon/animation.lock`. El pidfile sigue como
//! metadato auxiliar para señalizar y para clientes que consultan el PID.

use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::color::{hsv, scale};
use crate::lamp::{is_device_gone, Lamp, DEVICE_GONE_MSG};
use crate::state;
use crate::xdg;

// SAFETY: signal, kill y flock son libc con ABI estable.
unsafe extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    fn kill(pid: i32, sig: i32) -> i32;
    fn flock(fd: i32, op: i32) -> i32;
}

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;
/// Máximo de fotogramas por segundo.
pub const MAX_FPS: u64 = 60;

static STOP: AtomicBool = AtomicBool::new(false);

/// Resultado de intentar detener la animación en curso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopResult {
    /// Había animación y quedó detenida (o era un zombie; conviene restaurar).
    Detenida,
    /// El proceso sigue vivo tras el timeout: no escribir en el HID.
    SigueViva,
    /// No había animación que detener.
    NoHabia,
}

impl StopResult {
    /// `true` si es seguro que ningún otro proceso escribe por la animación.
    #[must_use]
    pub const fn writer_cleared(self) -> bool {
        matches!(self, Self::Detenida | Self::NoHabia)
    }

    /// `true` si había (o había habido) una animación a efectos de restaurar color.
    #[must_use]
    pub const fn had_animation(self) -> bool {
        matches!(self, Self::Detenida | Self::SigueViva)
    }
}

extern "C" fn on_stop_signal(_sig: i32) {
    STOP.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() {
    // SAFETY: solo un store atómico; async-signal-safe.
    unsafe {
        signal(SIGINT, on_stop_signal);
        signal(SIGTERM, on_stop_signal);
    }
}

fn runtime_keybackcon_dir() -> PathBuf {
    xdg::runtime_base().join("keybackcon")
}

/// Ruta del pidfile de la animación.
pub fn pid_file() -> PathBuf {
    runtime_keybackcon_dir().join("animation.pid")
}

fn lock_file_path() -> PathBuf {
    runtime_keybackcon_dir().join("animation.lock")
}

fn legacy_pid_files() -> Vec<PathBuf> {
    let base = xdg::runtime_base();
    vec![
        base.join("kbd-rgb/anim.pid"),
        base.join("keybackcon/anim.pid"),
    ]
}

fn read_pid(path: &Path) -> Option<i32> {
    fs::read_to_string(path)
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|pid: &i32| *pid > 0)
}

fn pid_is_zombie(pid: i32) -> bool {
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    stat.rsplit(')')
        .next()
        .map(str::trim_start)
        .is_some_and(|rest| rest.as_bytes().first().is_some_and(|b| *b == b'Z'))
}

fn pid_exists(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn pid_alive_at(path: &Path) -> Option<i32> {
    let Some(pid) = read_pid(path) else {
        let _ = fs::remove_file(path);
        return None;
    };
    // Preferimos /proc a kill(0): algunos entornos (sandbox de CI/agente)
    // bloquean señales aunque el proceso sea visible en /proc.
    if !pid_exists(pid) || pid_is_zombie(pid) {
        let _ = fs::remove_file(path);
        return None;
    }
    let cmd = fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    if cmd.contains("keybackcon") || cmd.contains("kbd-rgb") {
        Some(pid)
    } else {
        let _ = fs::remove_file(path);
        None
    }
}

/// Devuelve el pid de una animación viva validada.
pub fn animation_pid_alive() -> Option<i32> {
    if let Some(pid) = pid_alive_at(&pid_file()) {
        return Some(pid);
    }
    for legacy in legacy_pid_files() {
        if let Some(pid) = pid_alive_at(&legacy) {
            return Some(pid);
        }
    }
    None
}

fn pidfile_references_keybackcon() -> bool {
    for path in std::iter::once(pid_file()).chain(legacy_pid_files()) {
        let Some(pid) = read_pid(&path) else {
            continue;
        };
        if !pid_exists(pid) {
            continue;
        }
        if pid_is_zombie(pid) {
            return true;
        }
        let cmd = fs::read_to_string(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        if cmd.contains("keybackcon") || cmd.contains("kbd-rgb") {
            return true;
        }
    }
    false
}

/// Señaliza SIGTERM y espera hasta 1 s. Si el proceso sigue vivo, no borra el
/// pidfile y devuelve [`StopResult::SigueViva`].
pub fn stop_animation() -> StopResult {
    let me = std::process::id().cast_signed();
    let live = animation_pid_alive();
    let had = live.is_some() || pidfile_references_keybackcon();
    if let Some(pid) = live {
        if pid != me {
            // SAFETY: pid validado como keybackcon vivo distinto.
            let signaled = unsafe { kill(pid, SIGTERM) } == 0;
            if !signaled {
                eprintln!(
                    "aviso: no se pudo enviar SIGTERM a la animación (pid {pid}); \
                     no se escribe en el dispositivo"
                );
                return StopResult::SigueViva;
            }
            let mut still_alive = true;
            for _ in 0..20 {
                if !pid_exists(pid) || pid_is_zombie(pid) {
                    still_alive = false;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if still_alive {
                eprintln!(
                    "aviso: la animación anterior (pid {pid}) no respondió a SIGTERM; \
                     se conserva su bloqueo para no tener dos escritores"
                );
                return StopResult::SigueViva;
            }
        }
    }
    let _ = fs::remove_file(pid_file());
    for legacy in legacy_pid_files() {
        let _ = fs::remove_file(legacy);
    }
    if had {
        StopResult::Detenida
    } else {
        StopResult::NoHabia
    }
}

fn pid_owned_by(path: &Path, pid: i32) -> bool {
    read_pid(path) == Some(pid)
}

fn remove_own_pidfile(path: &Path, own: i32) {
    if pid_owned_by(path, own) {
        let _ = fs::remove_file(path);
    }
}

/// Bloqueo de exclusión mantenido mientras el descriptor vive (ADR 13).
#[derive(Debug)]
struct AnimationLock {
    file: File,
    pid_path: PathBuf,
    own: i32,
}

impl AnimationLock {
    fn claim(own: i32) -> io::Result<Self> {
        let lock_path = lock_file_path();
        let file = xdg::open_lock_file(&lock_path)?;
        // SAFETY: fd del fichero que acabamos de abrir; LOCK_EX|LOCK_NB.
        let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
        if rc != 0 {
            let e = io::Error::last_os_error();
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "ya hay una animación en marcha (no se pudo obtener {}): {e}",
                    lock_path.display()
                ),
            ));
        }
        let pid_path = pid_file();
        if let Some(parent) = pid_path.parent() {
            xdg::ensure_private_dir(parent)?;
        }
        {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&pid_path)
                .map_err(|e| {
                    // SAFETY: liberar el flock si falla el metadato.
                    unsafe {
                        let _ = flock(file.as_raw_fd(), LOCK_UN);
                    }
                    io::Error::new(
                        e.kind(),
                        format!("no se pudo escribir {}: {e}", pid_path.display()),
                    )
                })?;
            if let Err(e) = writeln!(f, "{own}") {
                let _ = fs::remove_file(&pid_path);
                unsafe {
                    let _ = flock(file.as_raw_fd(), LOCK_UN);
                }
                return Err(io::Error::new(
                    e.kind(),
                    format!("no se pudo escribir {}: {e}", pid_path.display()),
                ));
            }
        }
        Ok(Self {
            file,
            pid_path,
            own,
        })
    }
}

impl Drop for AnimationLock {
    fn drop(&mut self) {
        remove_own_pidfile(&self.pid_path, self.own);
        // SAFETY: liberar el flock del descriptor propio.
        unsafe {
            let _ = flock(self.file.as_raw_fd(), LOCK_UN);
        }
    }
}

/// Ejecuta `mode` (`breathe`/`rainbow`) a `fps` (1–60).
///
/// Orden: parar la anterior; si sigue viva, abortar sin abrir el HID; reclamar
/// el flock; abrir el dispositivo. El `Drop` del bloqueo limpia en cualquier
/// salida.
pub fn run_animation(mode: &str, fps: u64) -> io::Result<()> {
    match stop_animation() {
        StopResult::SigueViva => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "la animación anterior sigue viva; no se abre el dispositivo",
            ));
        }
        StopResult::Detenida | StopResult::NoHabia => {}
    }

    let own = std::process::id().cast_signed();
    STOP.store(false, Ordering::SeqCst);
    install_signal_handlers();
    let lock = AnimationLock::claim(own)?;

    let mut lamp = Lamp::open()?;
    lamp.autonomous(false)?;

    let dt = Duration::from_micros(1_000_000 / fps.clamp(1, MAX_FPS));
    let t0 = Instant::now();
    let (mut base, mut pct) = state::load_state();
    let mut refreshed = Instant::now();
    let result = (|| -> io::Result<()> {
        loop {
            if STOP.load(Ordering::SeqCst) {
                let (b, p) = state::load_state();
                let _ = lamp.color(scale(b, p));
                return Ok(());
            }
            if refreshed.elapsed() >= Duration::from_millis(500) {
                let (b, p) = state::load_state();
                base = b;
                pct = p;
                refreshed = Instant::now();
            }
            let t = t0.elapsed().as_secs_f64();
            let c = if mode == "rainbow" {
                scale(hsv((t * 45.0) % 360.0, 1.0, 1.0), pct)
            } else {
                let k = (1.0 - (t * std::f64::consts::TAU / 4.0).cos()) / 2.0;
                let factor = (8.0 + k * 92.0) * (f64::from(pct) / 100.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pct_factor = factor.clamp(0.0, 100.0) as u32;
                scale(base, pct_factor)
            };
            if let Err(e) = lamp.color(c) {
                if is_device_gone(&e) {
                    return Err(io::Error::new(e.kind(), DEVICE_GONE_MSG));
                }
                return Err(e);
            }
            std::thread::sleep(dt);
        }
    })();
    drop(lock);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::MutexGuard;

    fn tmp_runtime(tag: &str) -> (PathBuf, MutexGuard<'static, ()>) {
        let guard = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = PathBuf::from(format!(
            "/tmp/keybackcon-runtime-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("keybackcon")).expect("crear runtime de prueba");
        // SAFETY: TEST_ENV_LOCK serializa el entorno.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
        }
        (dir, guard)
    }

    fn live_keybackcon_child() -> Child {
        // argv0 = "keybackcon" para que /proc/<pid>/cmdline coincida.
        // Sueño corto: en sandboxes que bloquean señales no queremos esperar 60 s.
        Command::new("sleep")
            .arg0("keybackcon")
            .arg("3")
            .spawn()
            .unwrap()
    }

    #[test]
    fn stop_sin_pidfile_es_no_habia() {
        let (tmp, _env) = tmp_runtime("obsoleto");
        fs::write(tmp.join("keybackcon/animation.pid"), "2147483647").unwrap();
        assert_eq!(stop_animation(), StopResult::NoHabia);
        assert!(!tmp.join("keybackcon/animation.pid").exists());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stop_animation_mata_proceso_vivo() {
        let (tmp, _env) = tmp_runtime("vivo");
        assert_eq!(
            xdg::runtime_base(),
            tmp,
            "XDG_RUNTIME_DIR debe apuntar al tmp del test"
        );
        let mut child = live_keybackcon_child();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let cmd =
                fs::read_to_string(format!("/proc/{}/cmdline", child.id())).unwrap_or_default();
            if cmd.contains("keybackcon") {
                break;
            }
            assert!(Instant::now() < deadline, "el hijo no llegó a ejecutarse");
            std::thread::sleep(Duration::from_millis(5));
        }
        let pf = tmp.join("keybackcon/animation.pid");
        let pid_i = child.id().cast_signed();
        fs::write(&pf, pid_i.to_string()).unwrap();
        assert_eq!(pid_file(), pf);
        assert_eq!(read_pid(&pf), Some(pid_i));
        assert_eq!(
            animation_pid_alive(),
            Some(pid_i),
            "debe ver la animación viva antes de stop"
        );
        let stopped = stop_animation();
        // En sandboxes que bloquean SIGTERM el proceso puede seguir vivo.
        match stopped {
            StopResult::Detenida => {
                let _ = child.wait();
                assert!(!pf.exists());
            }
            StopResult::SigueViva => {
                let _ = child.kill();
                let _ = child.wait();
            }
            StopResult::NoHabia => panic!("esperaba Detenida o SigueViva"),
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn claim_flock_excluye_segundo_proceso() {
        let (tmp, _env) = tmp_runtime("flock");
        let first = AnimationLock::claim(1111).unwrap();
        let err = AnimationLock::claim(2222).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        drop(first);
        let second = AnimationLock::claim(2222).unwrap();
        drop(second);
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stop_con_zombie_es_detenida() {
        let (tmp, _env) = tmp_runtime("zombie-stop");
        let mut child = Command::new("sleep")
            .arg0("keybackcon")
            .arg("0")
            .spawn()
            .unwrap();
        let pid = child.id().cast_signed();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let zombie = fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
                s.rsplit(')')
                    .next()
                    .map(str::trim_start)
                    .is_some_and(|rest| rest.as_bytes().first() == Some(&b'Z'))
            });
            if zombie {
                break;
            }
            assert!(Instant::now() < deadline, "el hijo no quedó zombie");
            std::thread::sleep(Duration::from_millis(10));
        }
        fs::write(tmp.join("keybackcon/animation.pid"), pid.to_string()).unwrap();
        // Un zombie cuenta como «había animación» si /proc sigue visible.
        let r = stop_animation();
        assert!(
            matches!(r, StopResult::Detenida | StopResult::NoHabia),
            "zombie: {r:?}"
        );
        let _ = child.wait();
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn el_manejador_solo_marca_la_bandera_de_parada() {
        STOP.store(false, Ordering::SeqCst);
        on_stop_signal(SIGTERM);
        assert!(STOP.load(Ordering::SeqCst));
        STOP.store(false, Ordering::SeqCst);
    }

    #[test]
    fn writer_cleared_y_had_animation() {
        assert!(StopResult::Detenida.writer_cleared());
        assert!(StopResult::NoHabia.writer_cleared());
        assert!(!StopResult::SigueViva.writer_cleared());
        assert!(StopResult::SigueViva.had_animation());
        assert!(!StopResult::NoHabia.had_animation());
    }
}
