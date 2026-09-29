# Política de seguridad

### Propósito de este documento

- **Objetivos:** Declarar versiones soportadas, el canal privado de avisos y la superficie HID/udev/pkexec.
- **Estructura:** Versiones soportadas → cómo reportar → superficie relevante → alcance (CLI local, no SaaS).
- **Contenido a integrar según contexto:** Adapta versiones de `Cargo.toml`. No copies la política de un SaaS. No commitees dumps hidraw con datos ajenos ni claves.

## Versiones soportadas

| Versión        | Soportada                          |
| -------------- | ---------------------------------- |
| 2.2.x (`main`) | Sí                                 |
| 2.1.x / 2.0.x  | Solo histórico                     |

La versión vive en `Cargo.toml`. El tag `vX.Y.Z` debe coincidir.

## Cómo reportar una vulnerabilidad

**No abras un issue público** si el hallazgo puede elevar privilegios (udev/`pkexec`), inyectar rutas o hablar con el HID de forma inesperada.

1. Preferible: [GitHub Security Advisory](https://github.com/Soluciones-Alexendros/keybackcon/security/advisories/new) en este repositorio.
2. Alternativa: correo a [operaciones@alexendros.dev](mailto:operaciones@alexendros.dev).

Incluye: versión o commit, comando reproducido, distro, y un descriptor **mínimo** (nunca dumps con datos personales). Responderemos en un plazo máximo de 7 días naturales.

## Superficie relevante

- Transporte HID (`src/lamp.rs`, `src/protocol.rs`): ioctl `HIDIOC{G,S}FEATURE` con longitud acotada.
- Instalación udev vía `pkexec`: las rutas van por `$1`/`$2`, no interpoladas en `sh -c`.
- Estado en `$XDG_STATE_HOME/keybackcon/state` y pidfile en `$XDG_RUNTIME_DIR`.
- Renovate (`.github/renovate.json`) cubre `cargo` y `github-actions`. No hay Dependabot de version-updates.

## Alcance

Este repositorio es una CLI/GUI local para un teclado concreto. No opera un SaaS ni almacena datos de terceros. Las vulnerabilidades del firmware del teclado no son de este proyecto, salvo que el defecto esté en cómo hablamos con el dispositivo.
