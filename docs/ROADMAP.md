# Hoja de ruta

### Propósito de este documento

- **Objetivos:** Dejar visible qué está hecho y qué queda, ordenado por horizonte (en curso → corto → medio → largo → histórico).
- **Estructura:** Horizontes con objetivos marcables y lista de hitos logrados al final.
- **Contenido a integrar según contexto:** No copies un roadmap de otro producto. Los ADR y el protocolo no se mueven a este archivo.

Próximos objetivos, ordenados por horizonte. Un objetivo solo entra aquí
cuando está listo para trabajar (criterio claro) o bloqueado (se indica por
qué). Los hitos ya logrados van al final como histórico.

## En curso

Versionado automático de los `feat` que ya están en `main` y todavía no
tienen tag en esta historia (`5234969`, `dd191bc`). v2.3.0 y v2.3.1 están
publicadas. El tag `v2.3.1` (`de3a65d`) no es ancestro de `main`; en `main`
la cadena 2.3.1 entró con `8bbdf37`. El bump lo calcula `version.yml` al
empujar a `main`. No se fuerza el número ni se mueve el tag. AUR y la
primera instalación npm siguen en el corto plazo.

## Corto plazo

- [ ] **Publicar en AUR** — el `PKGBUILD` está listo; falta rellenar
      `sha256sums` reales (`updpkgsums`), generar `.SRCINFO`
      (`makepkg --printsrcinfo`) y subir con cuenta AUR. Bloqueado: cuenta.
- [ ] **Primer release npm real** — `npm install -g keybackcon` de prueba en
      una máquina limpia (Debian y Arch) validando el postinstalador.
      Bloqueado: `NPM_TOKEN` en GitHub Secrets.
- [ ] **CI: tests de la GUI** — la suite `gui/tests` ya corre en el job
      `gui-packaging`; añadir cobertura de `main.py` (D-Bus de control) con
      un bus de sesión de prueba si se ve útil.

## Medio plazo

- [ ] **RPM (spec + COPR)** o **Flatpak** — Flatpak queda limitado porque la
      regla udev necesita un paso manual fuera del sandbox; si se hace, hay
      que documentarlo. Decisión pendiente según demanda.
- [ ] **i18n real** — hoy los msgids son español con traducción identidad.
      Migrar msgids a inglés y tener `es.po`/`en.po` completos (decisión:
      migración completa vs. añadir `en.po` con msgids españoles).
- [ ] **Más teclados** — hacer configurables la firma del descriptor HID y
      la regla udev (VID/PID) vía variable de entorno o flag, si aparece
      demanda de otros modelos LampArray.

## Largo plazo / condicionado

- [ ] **Multi-zona** — solo si aparece firmware con `LampCount > 1`; el
      protocolo y la GUI ya separan "zona(s)" en la ficha del dispositivo.
- [ ] **Modo "seguir el escritorio"** — leer el color de acento del sistema
      y ofrecerlo como filtro. Solo si se pide (ver ADR pendiente).
- [ ] **Autostart vía unidad systemd del usuario** para la bandeja en vez
      del `.desktop` XDG, si se confirma que mejora el arranque en GNOME.

## Histórico

- [x] Probar en hardware AERO X16 cada comando + animaciones a 60 fps
- [x] Empaquetado en repo: metadata `cargo-deb`, `PKGBUILD` de AUR y release con `.deb` + SBOM + attestation
- [x] Publicar `.deb` en GitHub Releases (el workflow ya lo construye al empujar el tag)
- [x] Icono en la barra superior de GNOME (indicador Ayatana): verificado visible junto al resto de iconos con la extensión AppIndicator
- [x] v2.0.0 — Cargo por módulos, GUI mesa de luz, packaging, CI/release
- [x] v2.1.0 — auditoría integral: robustez HID, CLI estructurada sin dependencias, protocolo centralizado, calidad y empaquetado .deb/AUR preparado
- [x] v2.2.0 — comando restore, lanzador GUI por paquete, GUI acabada y empaquetado al día
- [x] v2.2.1 — restore systemd `/usr/bin`, udev argv seguro, CLI endurecido, CI MSRV
- [x] v2.3.0 (código) — parada sin bloqueos (zombies), estado atómico, validación estricta, `info --json`, cap de FPS, semántica de stop unificada, bandeja rediseñada, ventana Adwaita con apertura instantánea desde la bandeja (control D-Bus), autoversionado git-cliff, publicación npm, tests unitarios de la GUI (47) y compatibilidad multi-distribución
