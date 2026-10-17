# Decisiones (ADR breves)

### Propósito de este documento

- **Objetivos:** Conservar el registro de ADR vivos (nombre, cero deps, HID, GUI, calidad) y el motivo de cada uno.
- **Estructura:** Tabla índice → un apartado por ADR.
- **Contenido a integrar según contexto:** No copies ADR de otro crate. Un cambio de protocolo o de dependencias exige un ADR nuevo aquí. La arquitectura narrativa está en [`ARCHITECTURE.md`](../../../ARCHITECTURE.md).

| ID | Título | Estado | Resumen |
|---|---|---|---|
| 1 | Nombre `keybackcon` | Aceptada | Un solo nombre para binario, repo y App ID, con migración heredada desde `kbd-rgb`. |
| 2 | Cargo con cero dependencias | Aceptada | Binario pequeño y reproducible sin crates externos. |
| 3 | Instancia única vía pidfile validado | Mecanismo sustituido por ADR 13 | Invariante: una sola animación viva. El mecanismo `create_new`+pidfile queda sustituido por bloqueo con descriptor (`flock`). |
| 4 | Brillo por escala RGB persistente | Aceptada | Sin canal de brillo en firmware, el % escala el RGB y persiste. |
| 5 | GUI: mesa de luz con light-stage | Aceptada | Fijar luz en <2 s con vista previa fiel, sin prueba-error. |
| 6 | Transporte HID abstracto y parada limpia | Aceptada | `HidTransport` testeable sin hardware y ciclo de vida determinista. |
| 7 | CLI estructurada sin dependencias | Aceptada | `enum Command` + parser manual con mismos textos y salidas. |
| 8 | Protocolo centralizado en `protocol.rs` | Aceptada | Un solo archivo dueño de los bytes del cable, sin literales duplicados. |
| 9 | GUI por subproceso, sin FFI | Aplazada | Sin cdylib/FFI por ahora; se acepta duplicar la matemática de vista previa. |
| 10 | Rustdoc, pedantic y benchmarks sin dependencias | Aceptada | Calidad medible (rustdoc, clippy pedantic, bench/property propios) sin `criterion`/`proptest`. |
| 11 | Cota `HID_IOCTL_MAX_LEN` en el ioctl | Aceptada | Rechazar en construcción lo que el campo size de 14 bits no puede codificar. |
| 12 | Alineación al canon P0+P1+P2 | Aceptada | Jobs `quality`/`test`/`build`/`smoke`, docs canónicos y Renovate; sin clonar stacks ajenos. |
| 13 | Exclusión por descriptor y un solo escritor | Aceptada, enmendada | Todo escritor HID toma el mismo `flock` en `/run/keybackcon/writer.lock`. Sin daemon. La sesión activa no es una política adicional. |

## 1. Nombre: `keybackcon` / "Keyboard Backlight Controls"
Un solo nombre corto para binario, repo y App ID (`org.iniciativas.keybackcon`);
el nombre largo solo en display/docs/ventana. Rutas nuevas
(`keybackcon/state`, `keybackcon/animation.pid`) con lectura heredada de
`kbd-rgb/*`. Motivo: fin del baile de nombres, migración sin romper a nadie.

## 2. Cargo con cero dependencias
Estructura `src/` por módulos + `cargo test/clippy/fmt` en CI. Motivo:
compilación reproducible y binario estático pequeño; el ioctl cabe en
unas líneas sin crates.

## 3. Instancia única vía pidfile validado
**Estado: mecanismo sustituido por ADR 13.** El invariante (una sola
animación viva, sin matar procesos reciclados ni dos escritores) sigue
vigente. El mecanismo original (`create_new` del pidfile y relleno
posterior del PID) presentaba una ventana de carrera y queda reemplazado
por el bloqueo con descriptor descrito en el ADR 13.

## 4. Brillo por escala RGB persistente
Sin canal de brillo en el firmware, el porcentaje escala el color base y
se persiste junto al color. Motivo: `brightness` sobrevive a reinicios y a
`stop`, y la animación lo relee cada 500 ms.

## 5. GUI: mesa de luz con light-stage
Una columna, hero con vista previa fiel (`scale(base,pct)`, breathe 4 s,
rainbow), sin numeración decorativa. Motivo: el trabajo es "fijar luz en
<2 s"; la vista previa elimina el prueba-error contra el hardware.

## 6. Transporte HID abstracto y parada limpia
`lamp.rs` separa el rasgo `HidTransport` de `HidRaw` (único FFI ioctl) para
probar construcción/parseo de informes sin hardware; la petición ioctl
codifica la longitud real del buffer. `animation` captura SIGTERM/SIGINT con
`signal()` y solo escribe un `AtomicBool` async-signal-safe, restaura el color
base al parar y, si el teclado se desconecta, borra su pidfile y sale con
error. Motivo: ciclo de vida determinista y auditable sin crates.

## 7. CLI estructurada sin dependencias
`cli.rs` modela los comandos como un `enum Command` (`Help`, `Version`,
`Info`, `Set`, `Off`, `Brightness`, `FirmwareEffects`, `Animation`, `Stop`,
`Restore`)
poblado por un `parse(&[String])` manual, y cada comando tiene su `cmd_*`.
Los fallos viajan como `error::Error { Usage, Invalid, Io }` (`Display` +
`std::error::Error` + `From<io::Error>`) y `color::ColorError` conserva el
mensaje del color. Motivo: mantener la identidad de cero dependencias
(ADR 2) que `clap`/`thiserror` romperían, con el mismo texto en español y los
mismos códigos de salida (**0** ayuda pedida / éxito, **2** uso incorrecto,
**1** error de ejecución). La ayuda solicitada (`--help` / `help`) deja de
salir con 2 (H-23).

## 8. Protocolo centralizado en `src/protocol.rs`
Los bytes del cable (firma `05 59 09 01 A1 01`, IDs 1/5/6, longitudes 23/10
y mínimo 19, offsets de `LampCount` 1–2 y `LampArrayKind` 15–18, bytes de
autónomo e intensidad `0xff`, codificación `HIDIOCGFEATURE`/
`HIDIOCSFEATURE`) junto con sus constructores (`color_report`,
`autonomous_report`), parser (`parse_attrs`) y el constructor ioctl validado
(`hid_ioctl_request`, rechaza 0 o > `u16::MAX`) viven en `protocol.rs`;
`lamp.rs` solo los consume. Motivo: un cambio de protocolo toca un único
archivo, los tests de bytes exactos quedan junto a las constantes y
desaparecen los literales mágicos duplicados en el transporte.

## 9. GUI por subproceso, sin FFI (cdylib aplazada)
`gui/control_panel.py` sigue hablando con el CLI por subproceso
  (`set`/`brightness`/`animation`/`stop`) y leyendo state/pidfile para la
  vista previa; no se toca. Se descarta por ahora exponer el CLI como cdylib
  con FFI. Motivo: mantiene la identidad de cero dependencias (ADR 2) y una
  única fuente de verdad; el CLI sigue siendo el único dueño del hardware
  (ADR 3) y no aparecen dos escritores; el GUI ya maneja timeouts y diálogos de
  error sobre texto estable; y no se añade superficie FFI/ABI frágil que
  compilar y versionar sin ganancia funcional. Trade-off aceptado: la
  matemática de vista previa (`scale`, HSV, breathe) está duplicada en Rust y
  Python; se asume para no arrastrar GTK/Python al binario y queda documentado
  en este ADR y en `ARCHITECTURE.md`. La decoupling cdylib/FFI queda
  deliberadamente aplazada (sin fecha) hasta que la duplicación cause
  divergencias reales.

## 10. Rustdoc, pedantic y benchmarks/property tests sin dependencias
El crate documenta con `//!`/`///` bajo `missing_docs`, activa
`clippy::pedantic` con `#[allow]` puntuales donde un cambio de tipos alteraría
la aritmética (HSV, casts acotados de la animación) o el despacho del CLI, mide
con `examples/bench_colors.rs` (`std::time::Instant` + `std::hint::black_box`)
en lugar de `criterion`, y cubre invariantes con un xorshift64 determinista de
semilla fija en los módulos de test en lugar de `proptest`. El ejemplo incluye
los módulos con `#[path]` (con `#![allow(dead_code)]`) porque el paquete
publica un binario, no una `lib`. Motivo: mantener la identidad de cero
dependencias (ADR 2) sin renunciar a calidad medible; `criterion` y `proptest`
arrastrarían árboles de dependencias desproporcionados para unas pocas
operaciones puras. Trade-off aceptado: los benchmarks dan tiempo medio (no
percentiles) y las property tests usan un generador propio de 64 bits;
suficiente para las invariantes actuales y reproducible en CI.

## 11. Cota `HID_IOCTL_MAX_LEN` en el ioctl
`hid_ioctl_request` rechaza 0 y `>0x3FFF` (`HID_IOCTL_MAX_LEN`): el campo size solo
tiene 14 bits (`_IOC_SIZEBITS`, `include/uapi/asm-generic/ioctl.h`); antes aceptaba
hasta `u16::MAX` truncando con `& 0x3fff`. Motivo: fallar en construcción en vez de
pedir al kernel otra longitud; 1/10/23 codifican idéntico.

## 12. Alineación al canon P0+P1+P2
Jobs del pipeline principal `quality` / `test` / `build` / `smoke` (el job
`msrv` y `release.yml` quedan aparte). Documentación contractual en la raíz
y árbol `docs/architecture|guides|runbooks`. Renovate en
`.github/renovate.json` (Cargo + GitHub Actions). Motivo: hablar el mismo
contrato de flota sin clonar stacks ajenos ni romper la identidad de cero
dependencias (ADR 2) ni el registro de ADR de este repo.

## 13. Exclusión por descriptor y un solo escritor

**Enmienda (estabilización).** El invariante pasa de «una animación» a
«una escritura HID». Cada comando que abre el dispositivo para escribir
(`set`, `off`, `brightness`, `firmware-effects`, `restore`, la restauración
de `stop` y la animación) adquiere el mismo `flock` exclusivo y lo mantiene
durante la operación y la transición de estado que forma parte de ella.
Consultar (`info`) no toma el candado y no escribe. No hay daemon
privilegiado ni un segundo proceso dueño del dispositivo (ADR 9). Sigue
sin crates nuevos (ADR 2): `flock` y `open` son libc.

El candado es `/run/keybackcon/writer.lock`. `tmpfiles.d` crea el directorio
`root:keybackcon` modo `0770` y el fichero `root:keybackcon` modo `0660`.
El grupo `keybackcon` lo declara `sysusers.d` del paquete. La regla udev
actual no nombra un grupo: solo aplica `TAG+="uaccess"` al `hidraw`. Esa
regla no cambia a `MODE="0666"` ni a un grupo que abra el dispositivo por
sí solo. Para escribir hacen falta las dos cosas: abrir el `hidraw` (ACL de
la sesión) y abrir el candado (miembro del grupo). Si el directorio no
existe, es un enlace, no pertenece a root o el modo no es el esperado, la
escritura se rechaza. No hay fallback a `/var/tmp` ni a un fichero `0666`.

El bloqueo es cooperativo: solo excluye a los procesos del producto que lo
toman. Dos usuarios que puedan abrir el candado se serializan; el segundo
recibe un error claro y no escribe. Esta enmienda no promete «solo la
sesión gráfica activa» más allá de la ACL `uaccess` del nodo. El cambio de
usuario real queda como prueba manual.

La comprobación de que no hay otra animación y la apertura del HID ocurren
con el candado ya tomado. El pidfile puede anotar el PID de la animación,
pero no otorga el derecho de escritura. No se promete `pidfd` en esta
enmienda. Los errores HID conservan `raw_os_error()` del `io::Error`
original. Motivo de la enmienda: el `flock` vivía solo en la animación y
en el runtime del usuario, así que un `set` concurrente o la sesión de
otro usuario quedaban fuera del mismo contrato.
