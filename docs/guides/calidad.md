# Guía: calidad y jobs de CI

### Propósito de este documento

- **Objetivos:** Fijar qué significa cada job del pipeline principal y cómo se mide la calidad sin inventar un gate de cobertura frágil.
- **Estructura:** Jobs `quality` / `test` / `build` / `smoke` → MSRV y release → cobertura.
- **Contenido a integrar según contexto:** Adapta Cargo y `scripts/smoke.sh` de este repo. No copies gates Vitest/Playwright de otro paquete. El job `msrv` y `release.yml` no se renombran.

## Jobs del pipeline principal

| Job | Qué hace |
| --- | -------- |
| `quality` | `cargo fmt --check`, `clippy --all-targets -D warnings`, rustdoc `-D warnings`, `py_compile gui/*.py`, shellcheck (o `bash -n`) |
| `test` | `cargo test -- --test-threads=1` (27 tests) |
| `build` | `cargo build --release` y artefacto del binario (hay `.deb`/tarball desplegable) |
| `smoke` | `scripts/smoke.sh`: `--version`/`--help`, GUI, udev, systemd, i18n, schema |

El job extra `msrv` comprueba toolchain 1.87. No es required-name del canon; se conserva porque el crate declara `rust-version = "1.87"`.

`release.yml` (tag `v*`) sigue aparte: `cargo test/build --locked`, suite Python (unittest + xvfb como `quality`), smoke, `.deb`, SBOM, attestation.

## Cobertura

No hay `llvm-cov` ni umbral porcentual en CI (cero dependencias, ADR 2 y 10). El mínimo de flota (≥ 70 %) se cubre con los tests de `cargo test` más el smoke. No se baja la barra de clippy/rustdoc para “arreglar” un porcentaje.

Fachada local: `make lint`, `make test`, `make smoke`, `make validate`.
