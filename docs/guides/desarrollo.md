# Desarrollo

### Propósito de este documento

- **Objetivos:** Arrancar el repo en local, pasar la fachada `make` y saber qué documento actualizar según el tipo de cambio.
- **Estructura:** Checklist → calidad (lints, rustdoc, benches) → empaquetado y release → dónde documentar.
- **Contenido a integrar según contexto:** Adapta Cargo, GUI GTK y scripts de este repo. No copies un setup npm/pnpm ni crates de HID ajenos. Un cambio de protocolo o de dependencias exige ADR en [`../architecture/decisions/DECISIONS.md`](../architecture/decisions/DECISIONS.md).

Checklist ejecutable (en orden; la fachada es `make validate`):

- [ ] `make lint` — fmt, clippy `--all-targets -D warnings`, rustdoc, `py_compile` de `gui/*.py`
- [ ] `make test` — `cargo test -- --test-threads=1` (43 tests; los de pid usan `XDG_RUNTIME_DIR` temporal)
- [ ] `make i18n` — regenera `po/keybackcon.pot` y compila los `.mo` en `gui/locale/` (los tests de i18n los necesitan)
- [ ] `python3 -m unittest discover -s gui/tests` — 66 tests de la GUI (sin display ni hardware; el control D-Bus necesita `python3-gi` y `dbus-daemon`)
- [ ] `make check-i18n` — coherencia `.pot`/`.po` y sin cadenas sin traducir
- [ ] `make smoke` — `./scripts/smoke.sh` (binario + GUI + packaging + tests unitarios)
- [ ] `./scripts/install.sh` — instala en `~/.local` + udev (sudo) + unidades de usuario

## Calidad

- **Tests de la GUI** (`gui/tests/`, `unittest` de stdlib): `python3 -m
  unittest discover -s gui/tests`. Cubren `client` (con un binario fake en un
  directorio ejecutable — ojo: algunos `/tmp` son `noexec`), `colors`,
  `theme` (vía archivo, con GSettings aislado), `i18n` (catálogos `po/*.po`;
  los que cargan `.mo` hacen skip si no se ejecutó `make i18n`), `compat`
  (detección de distro con `/etc/os-release` emulado), los helpers de la
  bandeja y el control D-Bus (`test_dbus_control.py`). Los
  tests de bandeja hacen skip si Gtk3/Ayatana faltan o si Gtk4 ya está
  cargado en el proceso (incompatibilidad GTK3/GTK4 documentada); en ese
  caso corren aislados: `python3 -m unittest gui.tests.test_tray_helpers`.
- **Control D-Bus**: `test_dbus_control.py` arranca un `dbus-daemon` de
  sesión privado (nunca toca el bus del usuario) y comprueba el contrato de
  `CONTROL_XML`, el despacho de `Toggle`/`Present`/`Quit` y `_llamar_control`
  contra un servicio en otro proceso. Requiere `python3-gi` y
  `dbus-daemon`; en CI (`KEYBACKCON_REQUIRE_DBUS=1`) su ausencia falla en vez
  de saltarse la cobertura.
- **Lints**: `Cargo.toml` activa `missing_docs` y el grupo
  `clippy::pedantic`; deben quedar limpios con
  `cargo clippy --all-targets -- -D warnings` (incluidos ejemplos y tests)
  y sin warnings de rustc. Los pocos `#[allow]` puntuales (matemática HSV,
  casts acotados de la animación y despacho del CLI) existen donde un cambio
  de tipos alteraría el comportamiento.
- **Rustdoc**: `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` valida los
  `//!` de módulo y los `///` de los elementos públicos. Los mensajes del
  **CLI Rust** están en español a propósito (identidad del proyecto); la
  **GUI** sí es traducible: msgids en inglés con catálogos `po/es.po` y
  `po/en.po` (ver ADR 14).
- **Benchmarks**: `cargo run --release --example bench_colors` imprime una
  tabla de ns/op de `parse_color`, `scale`, `hsv`, `to_hex` y los
  constructores de informes, usando solo `std::time::Instant` y
  `std::hint::black_box` (sin `criterion`, ver ADR 10 en
  [`DECISIONS.md`](../architecture/decisions/DECISIONS.md)).
- **Property tests**: `src/color.rs` y `src/protocol.rs` incluyen pruebas
  pseudoaleatorias deterministas con un xorshift64 de semilla fija (sin
  `proptest`, ver ADR 10); corren con el resto de `cargo test`.
- **Cobertura**: no hay gate `llvm-cov` en este crate (cero dependencias,
  ADR 2 y 10). El mínimo de flota (≥ 70 %) se cubre con los 43 tests de
  `cargo test` más el smoke. No se añade `criterion`/`proptest`/`tarpaulin`
  para inventar un porcentaje.

Commits convencionales (`feat:`, `fix:`, `docs:`…). El CHANGELOG.md es un
resumen curado a mano por release; las notas del release las genera GitHub
(`generate_release_notes` en `release.yml`).
La versión canónica vive en `Cargo.toml` y está espejada en
`gui/__init__.py` (`__version__`) y `packaging/aur/PKGBUILD` (`pkgver`); el
workflow `.github/workflows/version.yml` las sincroniza automáticamente en
cada push a `main`: calcula el bump con `git-cliff --bumped-version` desde
los commits convencionales desde el último tag y, si hay bump, actualiza las
tres fuentes, abre un PR de release en auto-merge y crea el commit
`chore(release)` y el tag `vX.Y.Z`. El push del tag (con el secret
`RELEASE_PAT`) dispara Release y NPM por `on: push: tags: v*` (un push hecho
con `GITHUB_TOKEN` no encadenaría workflows). Si falta `RELEASE_PAT`, el
workflow termina en verde con un notice y no publica. El workflow de release
verifica que el tag coincida con las tres fuentes. Los commits `chore` no
bumpan (`skip` en `cliff.toml`).

## Empaquetado y release

- **Debian/Ubuntu (`.deb`)**: `cargo install cargo-deb --locked && cargo deb`
  produce `target/debian/keybackcon_<versión>-1_amd64.deb` a partir de
  `[package.metadata.deb]` en `Cargo.toml`. Los scripts `postinst`/`prerm`
  viven en `packaging/debian/` y recargan udev/systemd; se instala con
  `sudo apt install ./keybackcon_*.deb`.
- **Arch Linux (AUR)**: `packaging/aur/PKGBUILD` compila con
  `cargo build --release --locked` e instala binario, GUI, lanzador, icono,
  regla udev, unidades de usuario y licencia. Antes de publicar hay que
  rellenar el mantenedor, calcular `sha256sums` (`updpkgsums`) y regenerar
  `.SRCINFO` (`makepkg --printsrcinfo`); ver `packaging/aur/README.md`.
- **GitHub Releases**: al empujar un tag `vX.Y.Z`,
  `.github/workflows/release.yml` publica el tarball
  `keybackcon-vX.Y.Z-linux-x86_64.tar.gz`, el `.deb`, un SBOM CycloneDX
  (`cargo-cyclonedx`), `SHA256SUMS` y una attestation de procedencia
  (`actions/attest-build-provenance`).
- **npm**: `.github/workflows/npm.yml` compila x86_64 y arm64 y publica el
  meta-paquete `keybackcon` (lanzador + postinstalador de regla udev y
  unidades systemd de usuario) junto a los paquetes de binario por
  plataforma `keybackcon-linux-x64` y `keybackcon-linux-arm64` (nombres
  planos: la cuenta del token no tiene el scope `@keybackcon` y npm
  devuelve E404 al publicar en un scope inexistente). Las fuentes
  viven en `npm/`; el binario se copia en `npm/platforms/*/bin/` solo en CI
  (ignorado por git). Requiere el secret `NPM_TOKEN`. Prueba local con
  `npm pack` en `npm/` y en `npm/platforms/linux-x64/`.

## Dónde documentar

| Cambio | Documento |
| ------ | --------- |
| Comando CLI o uso | `README.md` |
| Protocolo HID o capas | `ARCHITECTURE.md` + ADR si cambia el contrato |
| Fallo operativo (udev, dispositivo) | runbook en `docs/runbooks/` |
| Empaquetado / CI | `docs/runbooks/empaquetado.md` y `docs/guides/calidad.md` |
