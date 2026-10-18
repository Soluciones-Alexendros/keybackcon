# Arquitectura

### Propósito de este documento

- **Objetivos:** Describir capas, fronteras CLI/GUI y flujos HID para que un cambio no rompa el protocolo, el pidfile ni la identidad de cero dependencias.
- **Estructura:** Diagrama → módulos `src/` → GUI → flujos.
- **Contenido a integrar según contexto:** Adapta módulos de este crate. No copies una arquitectura de SaaS ni crates de HID ajenos. El protocolo vive en [`docs/architecture/protocol.md`](docs/architecture/protocol.md); los ADR en [`docs/architecture/decisions/DECISIONS.md`](docs/architecture/decisions/DECISIONS.md). No añadas dependencias Cargo sin ADR.

`keybackcon` habla directamente con el teclado vía hidraw, sin demonios ni dependencias.

```
┌──────────────┐   ioctl HIDIOCGFEATURE / HIDIOCSFEATURE   ┌────────────┐
│ keybackcon   │  ──────────────────────────────────────▶  │ /dev/hidrawN│
│ (Rust, 0 deps)│   LampArray, Usage Page 0x59, reportes    │ AERO X16    │
└──────────────┘   1/5/6: atributos, color, autónomo       └────────────┘
       │ state: ~/.local/state/keybackcon/state  ("ff7800 60")
       │ pidfile: $XDG_RUNTIME_DIR/keybackcon/animation.pid
       ▼
┌──────────────┐   subprocess: set / brightness / animation / stop
│ control_panel│   + lectura de state/pidfile para vista previa y estado
│ (GTK4/Adwaita)│
└──────────────┘
```

## Módulos del CLI (`src/`)

### `protocol.rs` — fuente única del protocolo en el cable

Firma del descriptor `05 59 09 01 A1 01` y subruta sysfs
`device/report_descriptor`, IDs 1/5/6, longitudes 23/10 (mínimo aceptado
19), offsets de `LampCount` (u16 LE, 1–2) y `LampArrayKind` (u32 LE, 15–18),
bytes de autónomo e intensidad `0xff`, `hid_ioctl_request`
(`HIDIOCGFEATURE`/`HIDIOCSFEATURE`, tipo `'H'` `0x48`, dirección
`0xC000_0000`; rechaza longitud 0 o > `0x3FFF` = 16383, el máximo de los
14 bits de tamaño de `_IOC`) y los constructores/parser
de informes (`parse_attrs`, `color_report`, `autonomous_report`).
Ver especificación en [`docs/architecture/protocol.md`](docs/architecture/protocol.md).

### `lamp.rs` — localización y transporte HID

Localiza el hidraw por la firma y la subruta de `protocol.rs`, abre con RW.
El transporte vive tras el rasgo `HidTransport` (`HidRaw` es el único sitio
con ioctl FFI), así los informes se construyen y parsean sin hardware; los
errores nombran operación y ruta. `attrs()` (LampCount), `autonomous(on)`
(reporte 6), `color(rgb)` (reporte 5, zona única) e `is_device_gone`
(ENODEV/ENXIO/ENOENT → desconexión) consumen `protocol.rs`.

### `color.rs` — color y brillo

`Rgb`, 11 predefinidos, `parse_color` (nombre/hex/`#hex`, error tipado
`ColorError` con el mensaje intacto), `scale` (brillo por escala RGB: el
firmware declara `IntensityLevelCount=1`, no hay canal de brillo), `hsv`,
`to_hex`.

### `state.rs` — estado persistente

`state_file()` nuevo (`keybackcon/state`) con lectura heredada
(`kbd-rgb/state`); `load_state` tolerante, `save_state` atómica
(fichero temporal + `rename`, avisa en stderr si falla), `apply` (apaga
autónomo → color → persiste).

### `animation.rs` — animación exclusiva y parada limpia

Pidfile nuevo + dos rutas heredadas. El derecho de escribir no es el
pidfile: es el `flock` de `/run/keybackcon/writer.lock` (`writer.rs`), el
mismo que usan `set`, `off`, `brightness`, `firmware-effects`, `restore` y
`stop` cuando restauran color. `info` no lo toma. Si el directorio no es el
que instala `tmpfiles.d` (`root:keybackcon`, `0770`) o el fichero no es
`0660`, la escritura se rechaza. No hay fallback a `/var/tmp`.

La parada sigue validando el pid con `/proc/<pid>/stat` (un zombie no es
una animación viva) y `/proc/<pid>/cmdline` (un PID reciclado que ya no es
`keybackcon` no se señaliza). `run_animation` señala primero la animación
anterior y solo abre el HID con el candado ya tomado. Los signos solo marcan
un `AtomicBool`; el bucle relee el estado cada 500 ms y `fps` se acota a
1–`MAX_FPS` (60); al parar restaura el color base y borra su pidfile solo si
sigue siendo suyo, antes de soltar el candado. Si el teclado se desconecta
limpia y sale con "el teclado parece haberse desconectado".
`stop_animation()` señaliza SIGTERM y espera (≤1 s). Si el peer sobrevive,
no se abre el dispositivo.

### `cli.rs` — comandos y despacho

Modelo estructurado de comandos (`Command`, `Mode`, `BrightnessChange`) con
parser manual sin dependencias: `parse(&[String])` valida y `run()` despacha
a `cmd_info/cmd_set/cmd_off/cmd_brightness/cmd_firmware_effects/
cmd_animation/cmd_stop/cmd_restore`; `usage()` conserva texto y `exit 2`.
Comandos `info [--json]/set/off/brightness|bright/firmware-effects|auto/
animation|anim/stop/restore`, `--help/--version`. `info --json` imprime una
línea `{"version","device","lamps","kind","state","brightness"}` estable para
la GUI (el formato humano no es API). El parser rechaza argumentos extra y
`--fps` no numérico. Todo lo que cambia el color detiene primero la
animación: una sola escritora, nunca "teclado loco".

### `error.rs` — errores del CLI

`Error { Usage, Invalid(String), Io(io::Error) }` con `Display` +
`std::error::Error` + `From<io::Error>`: `Usage` termina en `usage()`
(exit 2) y el resto en `{BIN}: {e}` (exit 1).

## GUI (`gui/`)

Mesa de luz de una columna: hero light-stage (`DrawingArea` con bloom según
`scale(base, pct)`; previsualiza breathe 4 s / rainbow en local con
matemática duplicada respecto a `animation.rs`, ver ADR 9 en
[`docs/architecture/decisions/DECISIONS.md`](docs/architecture/decisions/DECISIONS.md)), filtros
circulares + tono propio, intensidad con slider continuo 0–100, movimiento
Fijar/Respirar/Arcoíris, estado vivo (pidfile + `info --json`, leído una vez
y cacheado). El estado se sigue con `Gio.FileMonitor` sobre el state file
(nunca un subproceso por segundo); las operaciones con diálogos (udev/pkexec)
corren en hilos worker con `GLib.idle_add`. Habla con el hardware solo vía
subproceso al CLI.

La bandeja (`--tray`) es un proceso aparte (GTK3 + AyatanaAppIndicator3, por
incompatible con GTK4): menú con slider de brillo, predefinidos de color,
submenú de animaciones con modo activo marcado, Restaurar/Preferencias/Acerca
de/Salir; sincroniza su estado leyendo state/pidfile cada 2 s y lanza la
ventana como subproceso trackeado que mata y reaped al salir. El cierre es
limpio (sin `os._exit`), así `client.stop()` siempre se ejecuta.

La GUI tiene suite de tests unitarios en `gui/tests/` (cliente con binario
fake, colores, tema, i18n, helpers de bandeja) y `gui/compat.py` detecta la
distribución (`/etc/os-release`) para sugerir el comando de instalación de
las dependencias en cada familia de paquetes (apt/pacman/dnf/zypper).

La i18n de la GUI es gettext con **msgids en inglés** (`po/keybackcon.pot`)
y catálogos `po/es.po` y `po/en.po`; `gui/i18n.py` resuelve el idioma y
compila a `gui/locale/<idioma>/LC_MESSAGES/` con `make i18n` (no se
versionan los `.mo`). El CLI Rust no se traduce (ADR 14).

## Flujos

| Flujo | Pasos |
|---|---|
| `set` / `off` / `brightness` | Si va a escribir en el HID, toma `/run/keybackcon/writer.lock`, abre el dispositivo y persiste. El brillo no detiene una animación viva (ADR 4): en ese caso solo guarda el porcentaje. |
| `animation` | Detiene la anterior → toma el candado → anota el pidfile → abre el dispositivo → bucle (relee estado cada 500 ms) → al parar restaura la base, borra su pidfile y suelta el candado |
| `stop` | Señaliza SIGTERM y espera (≤1 s). Si la animación muere, restaura el color con el candado tomado. Si sigue viva, no abre el dispositivo. |
| `restore` | Lee estado guardado → `apply` (autónomo off → color) |
| GUI (vista previa y estado) | Invoca al CLI por subproceso + lee state/pidfile (nunca toca hidraw) |
