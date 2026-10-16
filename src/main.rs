//! `keybackcon` — Keyboard Backlight Controls: controla el RGB de la zona
//! única del teclado AERO X16 hablando directamente con `/dev/hidrawN`
//! mediante `ioctl` (`HIDIOCGFEATURE`/`HIDIOCSFEATURE`) y el protocolo USB HID
//! `LampArray` (`Usage Page 0x59`), sin dependencias externas.
//!
//! # Modelo de hardware
//!
//! El firmware expone un descriptor `LampArray` estándar y `keybackcon` solo
//! usa los informes 1 (atributos), 5 (color de la zona `0..=LampCount-1`) y 6
//! (efectos autónomos on/off). El dispositivo declara `IntensityLevelCount=1`
//! (sin canal de brillo separable), así que `brightness` escala el color base
//! y lo persiste en `$XDG_STATE_HOME/keybackcon/state`. Una animación en
//! primer plano es la única escritora de color, coordinada por un pidfile
//! atómico en `$XDG_RUNTIME_DIR/keybackcon/animation.pid`.
//!
//! # Mapa de módulos
//!
//! - `animation`: bucle de animación, pidfile validado y parada por señal.
//! - `cli`: parser y despacho de comandos, ayuda y códigos de salida.
//! - `color`: `Rgb`, predefinidos, parseo, escala de brillo y HSV.
//! - `error`: errores de uso, entrada inválida e I/O del CLI.
//! - `lamp`: localización y apertura del hidraw y transporte HID.
//! - `protocol`: fuente única del protocolo en el cable (informes e ioctl).
//! - `state`: color base y porcentaje persistidos entre ejecuciones.
//!
//! Los mensajes de cara al usuario están en español a propósito: son la
//! interfaz estable del proyecto, no cadenas pendientes de traducción.

mod animation;
mod cli;
mod color;
mod error;
mod lamp;
mod protocol;
mod state;
mod xdg;

fn main() {
    cli::main();
}

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
