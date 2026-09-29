# Changelog

La fuente de verdad son los tags y `git-cliff`. Resumen:

## [Unreleased]

## [2.3.0] — 2026-09-25
- Núcleo: parada de animación sin bloqueos (reap de zombies, semántica de
  stop unificada), estado persistente atómico, validación estricta de
  argumentos, `info --json` estable para la GUI y cap de FPS a 60.
- GUI: ventana Adwaita reestructurada (grupos Movimiento/Brillo/Color,
  píldora de estado), apertura instantánea desde la bandeja vía control
  D-Bus y bandeja rediseñada con cierre limpio.
- Calidad: 33 tests Rust y 47 tests unitarios de la GUI (`gui/tests/`),
  integrados en `smoke.sh` y CI; `gui/compat.py` con preflight
  multi-distribución (apt/pacman/dnf/zypper).
- Packaging: `ExecStopPost=keybackcon stop`, `Restart=on-failure` en la
  unidad de restauración e `install.sh` respeta la unidad desactivada.
- CI/CD: autoversionado con git-cliff (bump desde commits convencionales,
  PR de release en auto-merge y tag), publicación npm (`keybackcon` +
  `keybackcon-linux-x64`/`linux-arm64`) y org migrada a
  Soluciones-Alexendros.

## [2.2.1] — 2026-09-24
- Unidades systemd del paquete apuntan a `/usr/bin/keybackcon` (restore al
  iniciar sesión en `.deb`/AUR); `install.sh` las reescribe a `~/.local/bin`.
- Instalación udev vía `pkexec` pasa rutas por `$1`/`$2` (sin interpolar en
  `sh -c`).
- CLI: `firmware-effects` solo acepta `on`/`off`; brillo relativo `+N`/`-N`
  rechaza sufijos no numéricos (`+`/`-` solos siguen siendo ±10).
- CI: job MSRV con toolchain 1.87 además de stable.

## [2.2.0] — 2026-09-14
- Comando `restore`: detiene animación y reaplica color+brillo guardados;
  el servicio de usuario lo ejecuta al iniciar sesión para restaurar el color.
- Lanzador GUI por paquete (`packaging/keybackcon-gui`): localiza `gui/` y lo
  ejecuta; mismo esquema en `.deb`, AUR, `install.sh` y release.
- GUI acabada: bandeja Ayatana (`--tray`) con menú e icono dinámico, temas
  claro/oscuro, vista previa con partículas, ajustes, autostart opt-in e i18n
  en castellano (gettext).
- Calidad/empaquetado: MSRV 1.87 declarada, GUI a `optdepends` en AUR y
  `hid_ioctl_request` acotado a 0x3FFF (14 bits de `_IOC_SIZEBITS`).

## [2.1.0] — 2026-09-14
- Robustez HID: transporte abstracto testeable, ioctls con longitud validada
  y `SAFETY`, pidfile atómico, parada limpia por SIGINT/SIGTERM y aviso de
  desconexión del dispositivo.
- CLI estructurada sin dependencias (`Command`/`Error`), mismos comandos,
  alias, mensajes y códigos de salida; tests movidos junto a su módulo.
- Protocolo LampArray centralizado en `src/protocol.rs` (constantes,
  constructores y parser) con la spec documentada.
- Calidad: rustdoc, `clippy::pedantic`, 24 tests (property tests
  deterministas) y bench std-only (`examples/bench_colors.rs`).
- Empaquetado: `cargo-deb` + postinst/prerm, release con `.deb`, SBOM
  CycloneDX, `SHA256SUMS` y attestation; `PKGBUILD` de AUR listo (pendiente
  de publicar).

## [2.0.0] — 2026-09-14
- Renombrado a `keybackcon` ("Keyboard Backlight Controls"), repo
  `Soluciones-Alexendros/keybackcon`, App ID `org.iniciativas.keybackcon`.
- CLI migrado a Cargo por módulos (`lamp/color/state/animation/cli`), cero
  dependencias, mismos comandos + alias `bright/auto/anim`, rutas nuevas con
  migración heredada `kbd-rgb`.
- GUI rediseñada: mesa de luz con light-stage, filtros + tono propio,
  intensidad −/+, movimiento Fijar/Respirar/Arcoíris, estado vivo, toasts,
  atajos y respeto a `gtk-enable-animations`.
- Packaging (`packaging/`), icono, `scripts/install.sh` idempotente y
  `scripts/smoke.sh`; systemd/udev/desktop con el nombre nuevo.
- CI (fmt+clippy+test+smoke) y release por tag con binario adjunto.
