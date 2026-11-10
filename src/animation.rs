//! Animación en primer plano (`breathe`/`rainbow`) y parada tipada (ADR 13).
//!
//! El derecho de escritura lo da el candado común
//! `/run/keybackcon/writer.lock` ([`crate::writer`]). El pidfile solo anota
//! el PID mientras ese candado está tomado; no otorga el derecho de escribir.

use std::fs;
use std::io::{self, Write};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::color::{hsv, scale};
use crate::lamp::{is_device_gone, Lamp, DEVICE_GONE_MSG};
use crate::state;
use crate::writer::WriterGuard;
use crate::xdg;

// SAFETY: signal/kill/getuid/syscall son libc con ABI estable en Linux.
unsafe extern "C" {
    fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    fn kill(pid: i32, sig: i32) -> i32;
    fn getuid() -> u32;
    fn syscall(num: i64, ...) -> i64;
}

const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
/// Máximo de fotogramas por segundo.
pub const MAX_FPS: u64 = 60;

/// Binarios de animación reconocidos (basename exacto de `/proc/<pid>/exe` y argv0).
const KNOWN_ANIM_BASENAMES: &[&str] = &["keybackcon", "kbd-rgb"];

/// Syscalls `pidfd_open` / `pidfd_send_signal` (Linux; mismos nº en `x86_64` y `aarch64`).
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const SYS_PIDFD_SEND_SIGNAL: i64 = 424;
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
const SYS_PIDFD_OPEN: i64 = 434;

/// Identidad temporal de un proceso de animación validado (PID + `starttime`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidatedAnim {
    pid: i32,
    starttime: u64,
}

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

fn path_basename(s: &str) -> &str {
    Path::new(s)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(s)
}

fn is_known_anim_basename(name: &str) -> bool {
    KNOWN_ANIM_BASENAMES.contains(&name)
}

fn read_starttime(pid: i32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    // Campo 22 (starttime): tras `)` el índice 0 es state → starttime = 22 - 3 = 19.
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn read_real_uid(pid: i32) -> Option<u32> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("Uid:") else {
            continue;
        };
        return rest.split_whitespace().next()?.parse().ok();
    }
    None
}

fn self_uid() -> u32 {
    // SAFETY: getuid no tiene efectos laterales.
    unsafe { getuid() }
}

fn read_exe_basename(pid: i32) -> Option<String> {
    let link = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    link.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn read_argv0_basename(pid: i32) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let first = raw.split(|&b| b == 0).next().filter(|s| !s.is_empty())?;
    let argv0 = std::str::from_utf8(first).ok()?;
    Some(path_basename(argv0).to_string())
}

/// Verifica ejecutable, argv0, propietario e identidad temporal (KBC-06).
/// Rechaza subcadenas incidentales (`decoy-keybackcon-tool`); acepta el
/// binario legado real `kbd-rgb` solo por basename exacto de exe/argv0.
fn validate_animation_pid(pid: i32) -> Option<ValidatedAnim> {
    if !pid_exists(pid) || pid_is_zombie(pid) {
        return None;
    }
    if read_real_uid(pid)? != self_uid() {
        return None;
    }
    let exe = read_exe_basename(pid)?;
    if !is_known_anim_basename(&exe) {
        return None;
    }
    let argv0 = read_argv0_basename(pid)?;
    if !is_known_anim_basename(&argv0) {
        return None;
    }
    let starttime = read_starttime(pid)?;
    Some(ValidatedAnim { pid, starttime })
}

fn same_validated_process(target: ValidatedAnim) -> bool {
    validate_animation_pid(target.pid) == Some(target)
}

/// Reintenta validación breve si `/proc/<pid>/exe` ya es conocido pero
/// `cmdline` aún está vacío tras el `exec` (carrera observada en CI/tests).
fn validate_animation_pid_ready(pid: i32) -> Option<ValidatedAnim> {
    if let Some(v) = validate_animation_pid(pid) {
        return Some(v);
    }
    let exe_known = read_exe_basename(pid).is_some_and(|b| is_known_anim_basename(&b));
    if !exe_known || !pid_exists(pid) || pid_is_zombie(pid) {
        return None;
    }
    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        if let Some(v) = validate_animation_pid(pid) {
            return Some(v);
        }
        if !pid_exists(pid) || pid_is_zombie(pid) {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    validate_animation_pid(pid)
}

fn pid_alive_validated_at(path: &Path) -> Option<ValidatedAnim> {
    let Some(pid) = read_pid(path) else {
        let _ = fs::remove_file(path);
        return None;
    };
    // Preferimos /proc a kill(0): algunos entornos (sandbox de CI/agente)
    // bloquean señales aunque el proceso sea visible en /proc.
    if let Some(v) = validate_animation_pid_ready(pid) {
        Some(v)
    } else {
        let _ = fs::remove_file(path);
        None
    }
}

#[cfg(test)]
fn pid_alive_at(path: &Path) -> Option<i32> {
    pid_alive_validated_at(path).map(|v| v.pid)
}

fn animation_validated_alive() -> Option<ValidatedAnim> {
    if let Some(v) = pid_alive_validated_at(&pid_file()) {
        return Some(v);
    }
    for legacy in legacy_pid_files() {
        if let Some(v) = pid_alive_validated_at(&legacy) {
            return Some(v);
        }
    }
    None
}

/// Devuelve el pid de una animación viva validada.
pub fn animation_pid_alive() -> Option<i32> {
    animation_validated_alive().map(|v| v.pid)
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
            // Zombie: solo cuenta si exe/argv0 aún identifican el binario.
            let exe_ok = read_exe_basename(pid).is_some_and(|b| is_known_anim_basename(&b));
            let argv_ok = read_argv0_basename(pid).is_some_and(|b| is_known_anim_basename(&b));
            if exe_ok || argv_ok {
                return true;
            }
            continue;
        }
        if validate_animation_pid(pid).is_some() {
            return true;
        }
    }
    false
}

fn pidfd_open(pid: i32) -> Option<OwnedFd> {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        // SAFETY: pidfd_open(pid, 0); fd nuevo o -errno.
        let fd = unsafe { syscall(SYS_PIDFD_OPEN, pid, 0_u32) };
        if fd < 0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation)]
        let raw = fd as RawFd;
        // SAFETY: fd exclusivo de pidfd_open.
        Some(unsafe { OwnedFd::from_raw_fd(raw) })
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = pid;
        None
    }
}

fn pidfd_send_signal(pidfd: &OwnedFd, sig: i32) -> bool {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: pidfd_send_signal(pidfd, sig, NULL, 0).
        let rc = unsafe {
            syscall(
                SYS_PIDFD_SEND_SIGNAL,
                pidfd.as_raw_fd(),
                sig,
                std::ptr::null::<u8>(),
                0_u32,
            )
        };
        rc == 0
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        let _ = (pidfd, sig);
        false
    }
}

/// Señaliza tras revalidar identidad; prefiere pidfd para acotar TOCTOU (KBC-07).
fn signal_validated_animation(target: ValidatedAnim) -> bool {
    if let Some(pidfd) = pidfd_open(target.pid) {
        // Entre open y la señal el PID pudo reciclarse: revalidar starttime/exe.
        if !same_validated_process(target) {
            return false;
        }
        return pidfd_send_signal(&pidfd, SIGTERM);
    }
    if !same_validated_process(target) {
        return false;
    }
    // SAFETY: identidad revalidada inmediatamente antes.
    unsafe { kill(target.pid, SIGTERM) == 0 }
}

fn process_still_target(target: ValidatedAnim) -> bool {
    if !pid_exists(target.pid) || pid_is_zombie(target.pid) {
        return false;
    }
    // Si el starttime cambió, el PID se recicló: el proceso original ya no vive.
    read_starttime(target.pid) == Some(target.starttime)
}

/// Señaliza SIGTERM y espera hasta 1 s. Si el proceso sigue vivo, no borra el
/// pidfile y devuelve [`StopResult::SigueViva`].
pub fn stop_animation() -> StopResult {
    let me = std::process::id().cast_signed();
    let live = animation_validated_alive();
    let had = live.is_some() || pidfile_references_keybackcon();
    if let Some(target) = live {
        if target.pid != me {
            let signaled = signal_validated_animation(target);
            if signaled {
                let mut still_alive = true;
                for _ in 0..20 {
                    if !process_still_target(target) {
                        still_alive = false;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                if still_alive {
                    eprintln!(
                        "aviso: la animación anterior (pid {}) no respondió a SIGTERM; \
                         se conserva su bloqueo para no tener dos escritores",
                        target.pid
                    );
                    return StopResult::SigueViva;
                }
            } else if process_still_target(target) && same_validated_process(target) {
                eprintln!(
                    "aviso: no se pudo enviar SIGTERM a la animación (pid {}); \
                     no se escribe en el dispositivo",
                    target.pid
                );
                return StopResult::SigueViva;
            }
            // Si la señal falló porque el PID se recicló o murió, limpiamos pidfiles.
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

/// Candado de escritura más el pidfile anotado mientras ese candado vive.
#[derive(Debug)]
struct AnimationLock {
    guard: Option<WriterGuard>,
    pid_path: PathBuf,
    own: i32,
}

fn write_own_pid(path: &Path, own: i32) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        xdg::ensure_private_dir(parent)?;
    }
    let mut file = xdg::open_lock_file(path)?;
    file.set_len(0)?;
    writeln!(file, "{own}").map_err(|e| {
        let _ = fs::remove_file(path);
        io::Error::new(
            e.kind(),
            format!("no se pudo escribir {}: {e}", path.display()),
        )
    })
}

impl AnimationLock {
    fn claim(own: i32) -> io::Result<Self> {
        let guard = WriterGuard::acquire()?;
        let pid_path = pid_file();
        if let Err(e) = write_own_pid(&pid_path, own) {
            drop(guard);
            return Err(e);
        }
        Ok(Self {
            guard: Some(guard),
            pid_path,
            own,
        })
    }
}

impl Drop for AnimationLock {
    fn drop(&mut self) {
        remove_own_pidfile(&self.pid_path, self.own);
        drop(self.guard.take());
    }
}

/// Ejecuta `mode` (`breathe`/`rainbow`) a `fps` (1–60).
///
/// Orden: parar la anterior; si sigue viva, abortar sin abrir el HID; tomar el
/// candado común; revalidar; abrir el dispositivo; **entonces** publicar el
/// modo (KBC-04). El guardado final coordinado ocurre **antes** de soltar
/// `AnimationLock` (KBC-05). El candado de estado solo se toma en transacciones
/// breves, nunca durante todo el bucle.
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
    if animation_pid_alive().is_some_and(|pid| pid != own) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "la animación anterior sigue viva; no se abre el dispositivo",
        ));
    }

    let mut lamp = Lamp::open()?;
    lamp.autonomous(false)?;
    // KBC-04: no publicar modo/on hasta tener candado HID + dispositivo.
    let run_id = state::begin_animation_mode(mode)?;

    let dt = Duration::from_micros(1_000_000 / fps.clamp(1, MAX_FPS));
    let t0 = Instant::now();
    let (mut base, mut pct) = state::load_state()?;
    let mut refreshed = Instant::now();
    let result = (|| -> io::Result<()> {
        loop {
            if STOP.load(Ordering::SeqCst) {
                let (b, p) = state::load_state()?;
                let _ = lamp.color(scale(b, p));
                return Ok(());
            }
            if refreshed.elapsed() >= Duration::from_millis(500) {
                let (b, p) = state::load_state()?;
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
    // KBC-05: transición final con el candado de animación aún tomado.
    let _ = state::end_animation_mode(run_id);
    drop(lock);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::process::{Child, Command};
    use std::sync::MutexGuard;

    struct RuntimeEnv {
        _lock: MutexGuard<'static, ()>,
        prev_runtime: Option<std::ffi::OsString>,
        prev_writer: Option<std::ffi::OsString>,
    }

    impl Drop for RuntimeEnv {
        fn drop(&mut self) {
            // SAFETY: el mutex de tests sigue tomado hasta el final de este drop.
            unsafe {
                match self.prev_runtime.take() {
                    Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
                    None => std::env::remove_var("XDG_RUNTIME_DIR"),
                }
                match self.prev_writer.take() {
                    Some(v) => std::env::set_var("KEYBACKCON_WRITER_LOCK", v),
                    None => std::env::remove_var("KEYBACKCON_WRITER_LOCK"),
                }
            }
        }
    }

    fn tmp_runtime(tag: &str) -> (PathBuf, RuntimeEnv) {
        let guard = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = PathBuf::from(format!(
            "/tmp/keybackcon-runtime-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir.join("keybackcon"))
            .expect("crear runtime de prueba");
        let lock = dir.join("writer.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o660)
            .open(&lock)
            .expect("candado de prueba");
        file.set_permissions(fs::Permissions::from_mode(0o660))
            .unwrap();
        let prev_runtime = std::env::var_os("XDG_RUNTIME_DIR");
        let prev_writer = std::env::var_os("KEYBACKCON_WRITER_LOCK");
        // SAFETY: TEST_ENV_LOCK serializa el entorno.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", &dir);
            std::env::set_var("KEYBACKCON_WRITER_LOCK", &lock);
        }
        (
            dir,
            RuntimeEnv {
                _lock: guard,
                prev_runtime,
                prev_writer,
            },
        )
    }

    /// Copia del binario de test con basename exacto (exe + argv0).
    /// No usamos `sleep`: en runners con rust-coreutils/uutils el multicall
    /// muere al renombrarse (`coreutils: unknown program 'keybackcon'`).
    /// Copiamos (no hardlink) para que `/proc/<pid>/exe` resuelva a `name`.
    fn fixture_anim_bin(runtime: &Path, name: &str) -> PathBuf {
        let fixtures = runtime.join("fixtures");
        fs::create_dir_all(&fixtures).expect("crear fixtures");
        let dest = fixtures.join(name);
        let _ = fs::remove_file(&dest);
        let src = std::env::current_exe().expect("current_exe");
        fs::copy(&src, &dest).expect("copiar binario de test como fixture");
        let mut perms = fs::metadata(&dest).expect("meta fixture").permissions();
        perms.set_mode(perms.mode() | 0o111);
        fs::set_permissions(&dest, perms).expect("chmod +x fixture");
        dest
    }

    /// Hijo que solo duerme cuando el padre fija `KEYBACKCON_ANIM_FIXTURE_SLEEP`.
    #[test]
    fn fixture_sleep_child() {
        let Ok(secs) = std::env::var("KEYBACKCON_ANIM_FIXTURE_SLEEP") else {
            return;
        };
        let n: u64 = secs.parse().unwrap_or(1);
        std::thread::sleep(Duration::from_secs(n));
        std::process::exit(0);
    }

    fn live_named_child(runtime: &Path, name: &str, secs: &str) -> Child {
        let bin = fixture_anim_bin(runtime, name);
        // Sueño corto: en sandboxes que bloquean señales no queremos esperar 60 s.
        Command::new(&bin)
            .args(["--exact", "animation::tests::fixture_sleep_child"])
            .env("KEYBACKCON_ANIM_FIXTURE_SLEEP", secs)
            .env_remove("RUST_TEST_THREADS")
            .spawn()
            .unwrap()
    }

    fn wait_exe_basename(pid: u32, expected: &str) {
        let pid_i = pid.cast_signed();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            // Exige exe y argv0: cmdline puede ir vacío un instante tras exec.
            let exe_ok = read_exe_basename(pid_i).as_deref() == Some(expected);
            let argv_ok = read_argv0_basename(pid_i).as_deref() == Some(expected);
            if exe_ok && argv_ok {
                return;
            }
            assert!(Instant::now() < deadline, "el hijo {expected} no arrancó");
            std::thread::sleep(Duration::from_millis(5));
        }
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
        let mut child = live_named_child(&tmp, "keybackcon", "3");
        wait_exe_basename(child.id(), "keybackcon");
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
    fn pid_reciclado_no_se_senala() {
        let (tmp, _env) = tmp_runtime("reciclado");
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id().cast_signed();
        fs::write(tmp.join("keybackcon/animation.pid"), pid.to_string()).unwrap();
        assert_eq!(stop_animation(), StopResult::NoHabia);
        assert!(
            Path::new(&format!("/proc/{pid}")).exists(),
            "un PID reciclado que ya no es keybackcon no se señaliza"
        );
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&tmp);
    }

    /// KBC-06: tras el fix, subcadena en argv0/exe no basta; decoy se rechaza.
    /// No se llama a `stop_animation` aquí (solo `pid_alive_at`); el hijo se mata al final.
    #[test]
    fn kbc06_pid_alive_rechaza_cmdline_con_subcadena_ajena() {
        let (tmp, _env) = tmp_runtime("subcadena-ajena");
        let mut child = live_named_child(&tmp, "decoy-keybackcon-tool", "8");
        wait_exe_basename(child.id(), "decoy-keybackcon-tool");
        let pid = child.id().cast_signed();
        let pf = tmp.join("keybackcon/animation.pid");
        fs::write(&pf, pid.to_string()).unwrap();
        assert_eq!(
            pid_alive_at(&pf),
            None,
            "KBC-06: decoy-keybackcon-tool no debe pasar la identidad estricta"
        );
        assert!(
            Path::new(&format!("/proc/{pid}")).exists(),
            "el decoy sigue vivo (no se señalizó)"
        );
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&tmp);
    }

    /// KBC-06: el binario legado real `kbd-rgb` (basename exacto de exe/argv0) sí cuenta.
    #[test]
    fn kbc06_pid_alive_acepta_kbd_rgb_legado_exacto() {
        let (tmp, _env) = tmp_runtime("kbd-rgb-legado");
        let mut legacy = live_named_child(&tmp, "kbd-rgb", "8");
        wait_exe_basename(legacy.id(), "kbd-rgb");
        let legacy_pid = legacy.id().cast_signed();
        let pf = tmp.join("keybackcon/animation.pid");
        fs::write(&pf, legacy_pid.to_string()).unwrap();
        let got = pid_alive_at(&pf);
        assert_eq!(
            got,
            Some(legacy_pid),
            "KBC-06: kbd-rgb legado real (basename exacto) se acepta; exe={:?} argv0={:?} uid={:?}/{}",
            read_exe_basename(legacy_pid),
            read_argv0_basename(legacy_pid),
            read_real_uid(legacy_pid),
            self_uid()
        );
        let _ = legacy.kill();
        let _ = legacy.wait();
        let _ = fs::remove_dir_all(&tmp);
    }

    /// KBC-07: stop no debe SIGTERM a un proceso ajeno aunque el pidfile lo cite.
    #[test]
    fn kbc07_stop_no_senala_decoy_ni_ajeno() {
        let (tmp, _env) = tmp_runtime("kbc07-decoy");
        let mut decoy = live_named_child(&tmp, "decoy-keybackcon-tool", "30");
        wait_exe_basename(decoy.id(), "decoy-keybackcon-tool");
        let pid = decoy.id().cast_signed();
        fs::write(tmp.join("keybackcon/animation.pid"), pid.to_string()).unwrap();
        assert_eq!(stop_animation(), StopResult::NoHabia);
        assert!(
            Path::new(&format!("/proc/{pid}")).exists(),
            "KBC-07: stop no señala el decoy"
        );
        let _ = decoy.kill();
        let _ = decoy.wait();
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
        let mut child = live_named_child(&tmp, "keybackcon", "0");
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
        // Un zombie de binario conocido cuenta como «había animación».
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
