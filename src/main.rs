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
//! primer plano es una escritora de color. Toda escritura HID comparte el
//! `flock` de `/run/keybackcon/writer.lock`; el pidfile solo anota la animación.
//!
//! # Mapa de módulos
//!
//! - `animation`: bucle de animación, pidfile validado y parada por señal.
//! - `writer`: candado exclusivo de toda escritura HID.
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
mod writer;
mod xdg;

fn main() {
    cli::main();
}

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Restaura variables de entorno al salir (también ante panic). KBC-33.
#[cfg(test)]
pub(crate) struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    prev: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

#[cfg(test)]
impl EnvGuard {
    pub(crate) fn lock() -> Self {
        let guard = TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self {
            _lock: guard,
            prev: Vec::new(),
        }
    }

    fn remember(&mut self, key: &'static str) {
        if !self.prev.iter().any(|(k, _)| *k == key) {
            self.prev.push((key, std::env::var_os(key)));
        }
    }

    pub(crate) fn set(&mut self, key: &'static str, value: impl AsRef<std::ffi::OsStr>) {
        self.remember(key);
        // SAFETY: TEST_ENV_LOCK serializa mutaciones del entorno en tests.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    pub(crate) fn remove(&mut self, key: &'static str) {
        self.remember(key);
        // SAFETY: TEST_ENV_LOCK serializa mutaciones del entorno en tests.
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(test)]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: el mutex sigue tomado hasta el final de este drop.
        unsafe {
            for (key, val) in self.prev.drain(..).rev() {
                match val {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}
