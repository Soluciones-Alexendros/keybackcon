# Keyboard Backlight Controls (`keybackcon`)

### Propósito de este documento

- **Objetivos:** Explicar qué es keybackcon, cómo instalarlo y los comandos de la CLI/GUI sin sustituir la spec HID ni los ADR.
- **Estructura:** Instalación → uso → cómo funciona → problemas típicos → desarrollo y contratos.
- **Contenido a integrar según contexto:** Adapta comandos y empaquetado de este repo. No copies un README de SaaS. El detalle HID está en [`docs/architecture/protocol.md`](docs/architecture/protocol.md); no lo dupliques aquí.

[![CI](https://github.com/Soluciones-Alexendros/keybackcon/actions/workflows/ci.yml/badge.svg)](https://github.com/Soluciones-Alexendros/keybackcon/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Soluciones-Alexendros/keybackcon)](https://github.com/Soluciones-Alexendros/keybackcon/releases)
![Rust](https://img.shields.io/badge/rust-sin%20dependencias-orange)
![GTK4](https://img.shields.io/badge/GUI-GTK4%20%2B%20Adwaita-blue)
![License](https://img.shields.io/badge/licencia-MIT-green)

Fija la luz de tu teclado **AERO X16** en dos toques: elige color, ajusta
intensidad y listo. Desde terminal o con mesa de luz gráfica, todo en castellano.

<p align="center">
  <img src="assets/screenshots/window-dark.png" width="300" alt="Mesa de luz en tema oscuro">
  <img src="assets/demo.gif" alt="Demostración: fijar color y animación">
</p>

La mesa de luz vive también en la **bandeja del sistema**
(`keybackcon-gui --tray`) y sigue el tema claro/oscuro de tu escritorio.

## Empieza en 3 pasos

1. Instala el `.deb` desde la
   [release](https://github.com/Soluciones-Alexendros/keybackcon/releases)
   (o el script local; ver abajo).
2. Fija un color: `keybackcon set ff7800`.
3. Abre la mesa de luz: `keybackcon-gui` (requiere GTK4 y libadwaita ≥ 1.5).

## Instalación

1. **Ubuntu/Debian (`.deb`, canal principal de GUI+CLI):** descarga el `.deb`
   de cada [release](https://github.com/Soluciones-Alexendros/keybackcon/releases)
   e instala con `sudo apt install ./keybackcon_X.Y.Z-1_amd64.deb`
   (binario, GUI, icono, regla udev y unidades de usuario). Debian 12:
   el CLI funciona; la mesa de luz nativa exige libadwaita ≥ 1.5 (ver matriz).
2. **Script local (`~/.local`):** `./scripts/install.sh`
   (pide sudo una vez para la regla udev).
3. **npm (solo CLI):** `npm install -g keybackcon` instala el binario
   precompilado (Linux x86_64/arm64) y, si el postinstalador lo permite, la
   regla udev y unidades de usuario. **No** incluye la mesa de luz GTK ni
   sus recursos. Omite udev con `KEYBACKCON_SKIP_UDEV=1`.
4. **Arch (AUR):** el empaquetado existe en el árbol del repo; la publicación
   AUR y los checksums **no están verificados** en esta revisión. No uses
   `yay -S keybackcon` como canal acreditado hasta que la matriz lo marque
   como probado. Mientras tanto: script local o binario de la release.
5. **Fedora:** usa el script local o el CLI de npm. El empaquetado RPM **no
   está verificado** aún.

Cada release trae además el tarball `keybackcon-vX.Y.Z-linux-x86_64.tar.gz`,
`SHA256SUMS` y SBOM CycloneDX (inventario del crate Rust; la GUI tiene
dependencias de sistema aparte).

Matriz completa (probado / candidato / experimental / no soportado):
[`docs/compatibility/matriz.md`](docs/compatibility/matriz.md).

## Compatibilidad

El CLI es un binario sin crates externos (sí depende de libc y del kernel).
La mesa de luz necesita **GTK 4** y **libadwaita ≥ 1.5**. La bandeja
(Ayatana) es opcional y separada.

| Distro | Instalación | CLI | GUI (Adw ≥ 1.5) |
|---|---|---|---|
| Debian 12 | `.deb` / script | Candidato | No soportada (Adw 1.2) |
| Debian 13 / Ubuntu 24.04+ | `.deb` / script | Candidato | Candidato si Adw ≥ 1.5 |
| Ubuntu 22.04 | `.deb` / script | Candidato | Solo si Adw ≥ 1.5; si no, CLI |
| Arch | script / release | Candidato | Candidato; AUR no acreditado |
| Fedora | script / npm CLI | Candidato | Candidato; RPM no verificado |
| openSUSE | — | Fuera del alcance anunciado | — |
| npm | `npm i -g keybackcon` | Sí (solo binario) | No incluida |

`install.sh` y la GUI detectan la distro (`/etc/os-release`) y proponen el
comando de paquetes cuando falta algo. Sin systemd: udev y CLI siguen
válidos; las unidades de usuario se omiten. Hyprland: integración
**experimental** hasta completar evidencias
([`docs/guides/hyprland.md`](docs/guides/hyprland.md)).

Contrato de brillo, apagado, restauración y JSON:
[`docs/product/contrato.md`](docs/product/contrato.md).

## Uso

| Comando | Efecto |
|---|---|
| `keybackcon set ff7800` | Fija color (hex, `#hex` o nombre: red, blue, orange…) |
| `keybackcon brightness 60` | Ajusta intensidad (o `+10` / `-10`) |
| `keybackcon animation breathe` | Anima (`breathe` o `rainbow`); `stop` la para |
| `keybackcon off` | Apaga (conserva tu color) |
| `keybackcon restore` | Reaplica tu color guardado (p. ej. al iniciar sesión) |
| `keybackcon info` | Muestra dispositivo y estado |
| `keybackcon-gui --tray` | Mesa de luz (solo bandeja con `--tray`) |

Atajos de la GUI: `Ctrl+1…9` cambia de filtro, `+`/`−` ajusta intensidad.

## Cómo funciona

- Habla directamente con el teclado por USB: el CLI es el único escritor; sin
  demonio privilegiado en medio.
- Tu color y brillo se guardan y sobreviven a reinicios. Ajustar el brillo
  **no** detiene una animación en curso (la animación relee el porcentaje).
- Solo hay una animación a la vez: fijar color o pedir `stop` la detiene antes.

Detalle técnico (HID LampArray, informes, estado): [`docs/architecture/protocol.md`](docs/architecture/protocol.md),
diseño en [`ARCHITECTURE.md`](ARCHITECTURE.md) y decisiones en [`docs/architecture/decisions/DECISIONS.md`](docs/architecture/decisions/DECISIONS.md).

## Problemas típicos

| Síntoma | Qué hacer |
|---|---|
| No encuentra el teclado | Revisa la conexión y prueba `keybackcon info` |
| Permiso denegado en `/dev/hidraw*` | `sudo udevadm trigger` y **vuelve a iniciar sesión** |
| La animación no para | `keybackcon stop` |

Runbook: [`docs/runbooks/dispositivo.md`](docs/runbooks/dispositivo.md).

## Desarrollo

¿Quieres compilar o contribuir? [`docs/guides/desarrollo.md`](docs/guides/desarrollo.md) y [`CONTRIBUTING.md`](CONTRIBUTING.md).
Índice de documentos: [`docs/README.md`](docs/README.md).

```bash
make validate   # lint + test + smoke
```

## Contratos

- [SECURITY.md](SECURITY.md) — avisos privados
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — conducta
- [SUPPORT.md](SUPPORT.md) — cómo pedir ayuda
- [AGENTS.md](AGENTS.md) — contrato para agentes
- [CHANGELOG.md](CHANGELOG.md) — historial (`git-cliff`)

Licencia MIT — ver [`LICENSE`](LICENSE).
