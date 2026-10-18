# Runbook: empaquetado y release

### Propósito de este documento

- **Objetivos:** Publicar un tag `vX.Y.Z` sin desalineación Cargo/tag y diagnosticar fallos de `.deb`/AUR.
- **Estructura:** Preflight → tag → artefactos → AUR → fallos típicos.
- **Contenido a integrar según contexto:** Adapta `cargo-deb` y `release.yml` de este repo. No copies semantic-release de un sitio web. No subas secretos ni `SHA256SUMS` a mano si el workflow ya los genera.

## Preflight

1. `make validate` verde.
2. Versión en `Cargo.toml` = tag `vX.Y.Z` (el workflow lo exige).
3. CHANGELOG: `git-cliff` es la fuente; no edites a mano en el release.

## Tag

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

`.github/workflows/release.yml` publica tarball, `.deb`, SBOM CycloneDX, `SHA256SUMS` y attestation.

## Fallos típicos

| Síntoma | Qué mirar |
| --- | --- |
| Tag no coincide con Cargo.toml | El job de release falla a propósito. Alinea versión y retaguea. |
| Falta `.mo` en el `.deb` | El workflow corre `msgfmt` antes de `cargo deb` para cada `po/*.po`. En local: `make i18n` (regenera el `.pot` y compila `gui/locale/<idioma>/LC_MESSAGES/keybackcon.mo`). |
| `.pot` desincronizado del código | `make check-i18n` compara `.pot` y `.po` (msgids, vacíos, duplicados). `make i18n` regenera con `xgettext` (`--keyword=_ --keyword=N_`). |
| AUR sin `sha256sums` | `updpkgsums` y `makepkg --printsrcinfo` antes de publicar (ver `packaging/aur/README.md`). |
| Unidades systemd en `~/.local` | `install.sh` reescribe `/usr/bin` → `%h/.local/bin`. El paquete debe quedar en `/usr/bin`. |
