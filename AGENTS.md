# AGENTS.md

**Destinatarios:** agentes de código y el rol Mantenedor que trabajen en este repositorio.  
**Propósito:** contrato operativo. Homogeneizamos **nombres y contratos**, no el lenguaje ni la API del producto.

## Fuentes de verdad (orden)

1. [README.md](./README.md) — uso de la CLI y la mesa de luz
2. Este archivo
3. [ARCHITECTURE.md](./ARCHITECTURE.md)
4. [docs/architecture/decisions/DECISIONS.md](./docs/architecture/decisions/DECISIONS.md)
5. [CONTRIBUTING.md](./CONTRIBUTING.md)
6. [SECURITY.md](./SECURITY.md)

No reinventes requisitos. Si falta ancla, paras y preguntas.

## Unidad de trabajo

```
Objetivo: <resultado verificable>
Traza: <ADR / issue / comando CLI>
Alcance: <archivos>
Exclusiones: <qué no harás>
Pruebas: make test / make smoke
Criterio de cierre: CI quality + test + build + smoke verdes
```

Una sesión = una unidad cohesiva. PR pequeño. Mensajes al humano y commits en español (Conventional Commits).

## Autonomía

**Puedes sin preguntar**

- Tests que fijan comportamiento ya aceptado
- Corregir fmt/clippy/rustdoc/py_compile causados por tu cambio
- Docs de guía/runbook en español
- Refactors locales que no cambien la CLI pública ni `src/protocol.rs`

**Requiere confirmación**

- Dependencia Cargo nueva (hoy el crate tiene **cero** deps; ADR 2)
- Cambiar informes HID, ioctl o `protocol.rs` → ADR previo
- Publicar tags o el AUR (el humano lanza el tag `vX.Y.Z`)
- Exponer el CLI como cdylib/FFI (ADR 9, aplazada)

## Stack y comandos

- Rust (MSRV 1.87, `Cargo.toml`), CLI sin crates externos
- GUI Python 3 + GTK4/Adwaita (`gui/`), i18n gettext (`po/`)
- Empaquetado: `cargo-deb`, AUR (`packaging/aur/`), udev/systemd

```bash
make lint
make test
make build
make smoke
make validate
```

CI principal (`.github/workflows/ci.yml`): jobs `quality`, `test`, `build`, `smoke`. Extra: `msrv`. Release queda en `release.yml`.

## Convenciones

- Ramas `feat/` `fix/` `docs/` `chore/` (los agentes Cloud usan `cursor/…`)
- Idioma: README/CONTRIBUTING/docs de guía en español
- No commitees `target/`, `__pycache__/`, `TASKS.md` ni secretos
- `TASKS.md` es local (`TASKS.example.md` es la plantilla)

## Layout

```
src/            CLI Rust (protocol, lamp, color, state, animation, cli)
gui/            Mesa de luz GTK4 (subproceso al CLI)
packaging/      udev, systemd, desktop, debian, AUR
scripts/        install.sh, smoke.sh
docs/           architecture/, guides/, runbooks/
po/             gettext (es)
```

## Definition of Done

- Criterios de la traza cumplidos
- Jobs `quality`, `test`, `build` y `smoke` verdes
- Docs canónicos actualizados si cambia el contrato
- Sin secretos en el diff
