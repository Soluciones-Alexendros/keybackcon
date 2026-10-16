//! Modelo estructurado de comandos y despacho del CLI.
//!
//! `parse` valida los argumentos y `run` despacha a los `cmd_*`. Códigos de
//! salida: 0 éxito (incluida ayuda pedida), 2 uso incorrecto, 1 error.
//! Cambiar color detiene la animación; el brillo no (ADR 4 / contrato).

use std::io::{self, Write};

use crate::animation::{self, MAX_FPS};
use crate::color::{self, Rgb};
use crate::error::Error;
use crate::lamp::Lamp;
use crate::state;

/// Nombre del binario usado en la ayuda y en los errores.
pub const BIN: &str = "keybackcon";

/// Comando ya validado que `run` sabe ejecutar.
pub enum Command {
    /// Muestra la ayuda (salida 0 si se pidió explícitamente).
    Help,
    /// Imprime nombre y versión.
    Version,
    /// Muestra dispositivo, lámparas, tipo y estado.
    Info {
        /// Salida JSON estable (para la GUI) en vez del formato humano.
        json: bool,
    },
    /// Fija el color base y lo persiste con el brillo actual.
    Set(Rgb),
    /// Apaga la zona sin perder el estado.
    Off,
    /// Ajusta el porcentaje de brillo y reaplica el color.
    Brightness(BrightnessChange),
    /// Activa o desactiva los efectos autónomos del firmware.
    FirmwareEffects(bool),
    /// Ejecuta una animación hasta que llega una señal.
    Animation {
        /// Modo de animación.
        mode: Mode,
        /// Fotogramas por segundo, ya acotados a 1–`MAX_FPS`.
        fps: u64,
    },
    /// Detiene la animación y restaura el color base.
    Stop,
    /// Reaplica el color y brillo guardados (p. ej. al iniciar sesión).
    Restore {
        /// Si es true, respeta `restore_policy=never` (unidad systemd).
        on_login: bool,
    },
}

/// Modo de animación en primer plano.
pub enum Mode {
    /// Respiración suave sobre el color base.
    Breathe,
    /// Ciclo de arcoíris completo.
    Rainbow,
}

impl Mode {
    fn as_str(&self) -> &'static str {
        match self {
            Mode::Breathe => "breathe",
            Mode::Rainbow => "rainbow",
        }
    }
}

/// Cambio pedido para el porcentaje de brillo.
pub enum BrightnessChange {
    /// Fija el porcentaje absoluto (acotado luego a 0–100).
    Set(u32),
    /// Suma `N` al porcentaje actual.
    Up(u32),
    /// Resta `N` al porcentaje actual.
    Down(u32),
}

fn help_text() -> String {
    format!(
        "uso: {BIN} <comando>\n\
         \n  Keyboard Backlight Controls — una zona, instancia única de animación\n\
         \n  info                     información del dispositivo (nº de lámparas, ruta hidraw)\n  set <color>              asigna color — hex 'ff6400' o nombre de predefinido\n  off                      apaga (el estado se conserva)\n  brightness <+N|-N|N>     brillo — escala RGB, persistente (alias: bright)\n  firmware-effects on|off  efectos del firmware (alias: auto)\n  animation <breathe|rainbow> [--fps N]\n                           animación en primer plano; instancia única (alias: anim)\n  stop                     detiene la animación y restaura el color base\n  restore [--on-login]     reaplica el color guardado (--on-login respeta restore_policy)\n  --help, -h               muestra esta ayuda\n  --version, -V            muestra la versión\n\
         \npredefinidos: {}",
        crate::color::PRESETS
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Imprime la ayuda en stderr y sale con código 2 (uso incorrecto).
pub fn usage() -> ! {
    eprintln!("{}", help_text());
    std::process::exit(2)
}

fn print_help_ok() {
    println!("{}", help_text());
}

/// Aborta si la animación anterior sigue viva (no hay exclusión limpia).
fn require_writer_clear() -> Result<(), Error> {
    let r = animation::stop_animation();
    if !r.writer_cleared() {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "la animación anterior sigue viva; no se escribe en el dispositivo",
        )));
    }
    Ok(())
}

/// Valida `args` (sin el nombre del binario) y construye el comando.
pub fn parse(args: &[String]) -> Result<Command, Error> {
    match args.first().map(String::as_str) {
        Some("--help" | "-h" | "help") => Ok(Command::Help),
        Some("--version" | "-V") => Ok(Command::Version),
        Some("info") => match args.get(1).map(String::as_str) {
            None => Ok(Command::Info { json: false }),
            Some("--json") if args.len() == 2 => Ok(Command::Info { json: true }),
            _ => Err(Error::Usage),
        },
        Some("set") => {
            if args.len() > 2 {
                return Err(Error::Usage);
            }
            let s = args.get(1).map_or("", String::as_str);
            let c = color::parse_color(s).map_err(|e| Error::Invalid(e.to_string()))?;
            Ok(Command::Set(c))
        }
        Some("off") if args.len() == 1 => Ok(Command::Off),
        Some("brightness" | "bright") => {
            if args.len() > 2 {
                return Err(Error::Usage);
            }
            let a = args.get(1).map_or("", String::as_str);
            Ok(Command::Brightness(parse_brightness(a)?))
        }
        Some("firmware-effects" | "auto") => {
            if args.len() > 2 {
                return Err(Error::Usage);
            }
            let a = args.get(1).map_or("", String::as_str);
            let on = match a {
                "on" => true,
                "off" => false,
                "" => {
                    return Err(Error::Invalid(
                        "firmware-effects requiere 'on' u 'off'".into(),
                    ));
                }
                other => {
                    return Err(Error::Invalid(format!(
                        "firmware-effects: '{other}' no válido (válidos: on, off)"
                    )));
                }
            };
            Ok(Command::FirmwareEffects(on))
        }
        Some("animation" | "anim") => parse_animation(args),
        Some("stop") if args.len() == 1 => Ok(Command::Stop),
        Some("restore") => match args.get(1).map(String::as_str) {
            None => Ok(Command::Restore { on_login: false }),
            Some("--on-login") if args.len() == 2 => Ok(Command::Restore { on_login: true }),
            _ => Err(Error::Usage),
        },
        _ => Err(Error::Usage),
    }
}

fn parse_brightness(a: &str) -> Result<BrightnessChange, Error> {
    if let Some(d) = a.strip_prefix('+') {
        let n = if d.is_empty() {
            10
        } else {
            d.parse()
                .map_err(|_| Error::Invalid("el brillo relativo debe ser un número (+N)".into()))?
        };
        return Ok(BrightnessChange::Up(n));
    }
    if let Some(d) = a.strip_prefix('-') {
        let n = if d.is_empty() {
            10
        } else {
            d.parse()
                .map_err(|_| Error::Invalid("el brillo relativo debe ser un número (-N)".into()))?
        };
        return Ok(BrightnessChange::Down(n));
    }
    a.parse()
        .map(BrightnessChange::Set)
        .map_err(|_| Error::Invalid("el brillo debe ser un número".into()))
}

fn parse_animation(args: &[String]) -> Result<Command, Error> {
    let mode = args.get(1).cloned().unwrap_or_else(|| "breathe".into());
    let mode = match mode.as_str() {
        "breathe" => Mode::Breathe,
        "rainbow" => Mode::Rainbow,
        _ => {
            return Err(Error::Invalid(format!(
                "modo desconocido: '{mode}' (válidos: breathe, rainbow)"
            )))
        }
    };
    let mut fps: u64 = 60;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--fps" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| Error::Invalid("animation: --fps requiere un valor".into()))?;
                fps = v.parse().map_err(|_| {
                    Error::Invalid(format!(
                        "animation: --fps '{v}' no es un número (válidos: 1–{MAX_FPS})"
                    ))
                })?;
                i += 2;
            }
            other => {
                return Err(Error::Invalid(format!(
                    "animation: argumento inesperado '{other}'"
                )));
            }
        }
    }
    Ok(Command::Animation {
        mode,
        fps: fps.clamp(1, MAX_FPS),
    })
}

/// Lee los argumentos reales y ejecuta el comando resultante.
pub fn run() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args)? {
        Command::Help => {
            print_help_ok();
            Ok(())
        }
        Command::Version => {
            println!("{} {}", BIN, env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Info { json } => cmd_info(json),
        Command::Set(c) => cmd_set(c),
        Command::Off => cmd_off(),
        Command::Brightness(change) => cmd_brightness(change),
        Command::FirmwareEffects(on) => cmd_firmware_effects(on),
        Command::Animation { mode, fps } => cmd_animation(mode, fps),
        Command::Stop => cmd_stop(),
        Command::Restore { on_login } => cmd_restore(on_login),
    }
}

fn cmd_info(json: bool) -> Result<(), Error> {
    let st = state::load_persisted();
    let path = Lamp::find()?;
    let mut l = Lamp::open()?;
    let (count, kind) = l.attrs()?;
    let hex = color::to_hex(st.base_color);
    if json {
        println!(
            "{{\"version\":\"{}\",\"device\":\"{}\",\"lamps\":{count},\"kind\":{kind},\
             \"state\":\"{hex}\",\"brightness\":{},\"power\":\"{}\",\"mode\":\"{}\",\
             \"state_origin\":\"persisted_preference\",\
             \"brightness_origin\":\"persisted_preference\"}}",
            env!("CARGO_PKG_VERSION"),
            json_escape(&path.to_string_lossy()),
            st.brightness,
            json_escape(&st.power),
            json_escape(&st.mode),
        );
        return Ok(());
    }
    println!("dispositivo : {}", path.display());
    println!("nº lámparas : {count}");
    println!("tipo        : {kind} (lo que declara el firmware; en este modelo es engañoso)");
    println!(
        "preferencia : #{hex} @ %{} (no es una medición física)",
        st.brightness
    );
    println!("modo        : {} · power={}", st.mode, st.power);
    Ok(())
}

/// Escapa un texto para incrustarlo entre comillas en JSON (sin dependencias).
fn json_escape(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < '\u{20}' => {
                #[allow(clippy::cast_possible_truncation)]
                let code = c as u32;
                let _ = write!(out, "\\u{code:04x}");
            }
            c => out.push(c),
        }
    }
    out
}

fn cmd_set(c: Rgb) -> Result<(), Error> {
    let (_, pct) = state::load_state();
    require_writer_clear()?;
    state::apply(&mut Lamp::open()?, c, pct)?;
    Ok(())
}

fn cmd_off() -> Result<(), Error> {
    require_writer_clear()?;
    let mut l = Lamp::open()?;
    l.autonomous(false)?;
    l.color((0, 0, 0))?;
    state::set_runtime_mode("fijar", "off")?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn cmd_brightness(change: BrightnessChange) -> Result<(), Error> {
    let mut st = state::load_persisted();
    let new = match change {
        BrightnessChange::Set(n) => n,
        BrightnessChange::Up(n) => st.brightness.saturating_add(n),
        BrightnessChange::Down(n) => st.brightness.saturating_sub(n),
    }
    .clamp(0, 100);
    st.brightness = new;
    // No detiene la animación: relee el porcentaje (ADR 4).
    // Si no hay animación, reaplica el color fijo.
    if animation::animation_pid_alive().is_none() {
        let mut l = Lamp::open()?;
        l.autonomous(false)?;
        l.color(color::scale(st.base_color, new))?;
        if st.power == "off" {
            st.power = "on".into();
            st.mode = "fijar".into();
        }
    }
    state::save_persisted(&st)?;
    println!("brillo: %{new}");
    Ok(())
}

fn cmd_firmware_effects(on: bool) -> Result<(), Error> {
    require_writer_clear()?;
    Lamp::open()?.autonomous(on)?;
    let mode = if on { "firmware" } else { "fijar" };
    state::set_runtime_mode(mode, "on")?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
fn cmd_animation(mode: Mode, fps: u64) -> Result<(), Error> {
    let name = mode.as_str();
    state::set_runtime_mode(name, "on")?;
    let result = animation::run_animation(name, fps);
    let _ = state::set_runtime_mode("fijar", "on");
    result.map_err(Error::from)
}

fn cmd_stop() -> Result<(), Error> {
    use animation::StopResult;
    let (base, pct) = state::load_state();
    let r = animation::stop_animation();
    match r {
        StopResult::SigueViva => {
            return Err(Error::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "la animación no respondió a SIGTERM; no se escribe en el dispositivo",
            )));
        }
        StopResult::Detenida => {
            state::apply(&mut Lamp::open()?, base, pct)?;
            println!("animación detenida; color base restaurado");
        }
        StopResult::NoHabia => {
            debug_assert!(!r.had_animation());
            println!("no hay ninguna animación en marcha");
        }
    }
    Ok(())
}

/// Punto de entrada: traduce el resultado a stderr y al código de salida
/// (2 para uso, 1 para el resto).
pub fn main() {
    match run() {
        Ok(()) => {}
        Err(Error::Usage) => usage(),
        Err(e) => {
            let _ = writeln!(io::stderr(), "{BIN}: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_restore(on_login: bool) -> Result<(), Error> {
    if on_login && !state::should_restore_on_login() {
        println!("restauración omitida (restore_policy=never)");
        return Ok(());
    }
    let (base, pct) = state::load_state();
    require_writer_clear()?;
    state::apply(&mut Lamp::open()?, base, pct)?;
    println!(
        "restaurado: #{:02x}{:02x}{:02x} @ %{pct}",
        base.0, base.1, base.2
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn parse_restore_devuelve_restore() {
        assert!(matches!(
            parse(&args(&["restore"])),
            Ok(Command::Restore { on_login: false })
        ));
    }

    #[test]
    fn firmware_effects_solo_on_u_off() {
        assert!(matches!(
            parse(&args(&["firmware-effects", "on"])),
            Ok(Command::FirmwareEffects(true))
        ));
        assert!(matches!(
            parse(&args(&["auto", "off"])),
            Ok(Command::FirmwareEffects(false))
        ));
        assert!(matches!(
            parse(&args(&["firmware-effects"])),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            parse(&args(&["firmware-effects", "maybe"])),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn brightness_relativo_exige_número_o_vacío() {
        assert!(matches!(
            parse(&args(&["brightness", "+"])),
            Ok(Command::Brightness(BrightnessChange::Up(10)))
        ));
        assert!(matches!(
            parse(&args(&["bright", "-5"])),
            Ok(Command::Brightness(BrightnessChange::Down(5)))
        ));
        assert!(matches!(
            parse(&args(&["brightness", "+abc"])),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            parse(&args(&["brightness", "-x"])),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn animation_fps_exige_número_y_rechaza_extras() {
        assert!(matches!(
            parse(&args(&["animation"])),
            Ok(Command::Animation { fps: 60, .. })
        ));
        assert!(matches!(
            parse(&args(&["animation", "rainbow"])),
            Ok(Command::Animation {
                mode: Mode::Rainbow,
                ..
            })
        ));
        assert!(matches!(
            parse(&args(&["animation", "breathe", "--fps", "30"])),
            Ok(Command::Animation { fps: 30, .. })
        ));
        // Fuera de rango se acota al máximo (documentado), no da error.
        assert!(matches!(
            parse(&args(&["animation", "breathe", "--fps", "500"])),
            Ok(Command::Animation { fps: 60, .. })
        ));
        assert!(matches!(
            parse(&args(&["animation", "breathe", "--fps"])),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            parse(&args(&["animation", "breathe", "--fps", "abc"])),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            parse(&args(&["animation", "breathe", "30"])),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            parse(&args(&["animation", "breathe", "--fps", "30", "extra"])),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn info_json_y_aridad_estricta() {
        assert!(matches!(
            parse(&args(&["info"])),
            Ok(Command::Info { json: false })
        ));
        assert!(matches!(
            parse(&args(&["info", "--json"])),
            Ok(Command::Info { json: true })
        ));
        assert!(matches!(
            parse(&args(&["info", "extra"])),
            Err(Error::Usage)
        ));
        assert!(matches!(
            parse(&args(&["set", "red", "extra"])),
            Err(Error::Usage)
        ));
        assert!(matches!(parse(&args(&["off", "x"])), Err(Error::Usage)));
        assert!(matches!(parse(&args(&["stop", "x"])), Err(Error::Usage)));
        assert!(matches!(parse(&args(&["restore", "x"])), Err(Error::Usage)));
        assert!(matches!(
            parse(&args(&["brightness", "50", "x"])),
            Err(Error::Usage)
        ));
    }

    #[test]
    fn json_escape_escapa_lo_escapable() {
        assert_eq!(json_escape("/dev/hidraw0"), "/dev/hidraw0");
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("a\u{1}b"), "a\\u0001b");
    }
}
