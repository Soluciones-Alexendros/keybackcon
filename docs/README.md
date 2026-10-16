# Documentación de keybackcon

### Propósito de este documento

- **Objetivos:** Indexar la documentación de producto (ADR, protocolo, guías y runbooks) y apuntar a los contratos de la raíz.
- **Estructura:** Tabla de rutas `docs/` → enlaces a README, AGENTS, ARCHITECTURE, CONTRIBUTING y SECURITY.
- **Contenido a integrar según contexto:** Adapta el índice al árbol de este repo. No copies guías de SaaS ni crates de HID ajenos. El protocolo no se reescribe fuera de `architecture/protocol.md` y `src/protocol.rs`.

| Ruta | Para qué |
| ---- | -------- |
| [architecture/protocol.md](./architecture/protocol.md) | Bytes del cable LampArray HID |
| [architecture/decisions/DECISIONS.md](./architecture/decisions/DECISIONS.md) | ADR vivos (nombre, cero deps, HID, GUI, exclusión) |
| [compatibility/matriz.md](./compatibility/matriz.md) | Distribuciones, Adw, Hyprland y estados de soporte |
| [product/contrato.md](./product/contrato.md) | Brillo, apagado, restauración, JSON y clientes |
| [guides/desarrollo.md](./guides/desarrollo.md) | Arranque local, `make` y PR |
| [guides/hyprland.md](./guides/hyprland.md) | Perfiles Hyprland, Waybar y UWSM (experimental) |
| [guides/calidad.md](./guides/calidad.md) | Jobs CI, lints y cobertura |
| [runbooks/dispositivo.md](./runbooks/dispositivo.md) | Teclado no encontrado, udev, animación |
| [runbooks/empaquetado.md](./runbooks/empaquetado.md) | `.deb`, AUR, tag de release |
| [ROADMAP.md](./ROADMAP.md) | Qué está hecho y qué falta |

Rutas históricas (stubs): [ARCHITECTURE.md](./ARCHITECTURE.md), [DECISIONS.md](./DECISIONS.md), [DEVELOPMENT.md](./DEVELOPMENT.md), [PROTOCOL.md](./PROTOCOL.md), [INDEX.md](./INDEX.md).

En la raíz: [README.md](../README.md), [AGENTS.md](../AGENTS.md), [ARCHITECTURE.md](../ARCHITECTURE.md), [CONTRIBUTING.md](../CONTRIBUTING.md), [SECURITY.md](../SECURITY.md).
