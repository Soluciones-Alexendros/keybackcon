# Hoja de ruta

Próximos objetivos, ordenados por horizonte. Un objetivo solo entra aquí
cuando está listo para trabajar (criterio claro) o bloqueado (se indica por
qué). Los hitos ya logrados van al final como histórico.

## En curso

- [ ] **Estabilización post-auditoría (2026-11-10)** — lotes P1/P2 en vuelo
      (permisos/candado, estado concurrente, GUI async, CI gráfico, identidad
      de procesos, docs). Criterio: hallazgos canónicos con PR mergeado y
      evidencia de prueba; no declarar «sin bloqueos» mientras queden P1
      abiertos. Trazas en el registro Notion de hallazgos/correcciones.
- [ ] **v2.5.0 publicada** — ya en `main`, GitHub Releases y npm; el
      versionado etiqueta desde el job que abre el PR de release.

## Corto plazo

- [ ] **Cerrar deuda documental y de empaquetado menor** — AUR (`sha256sums`
      + `.SRCINFO` + cuenta), prueba de `npm install -g` en Debian/Arch
      limpios, y hallazgos docs/CI P2 restantes con criterio de aceptación.
- [ ] **Decisiones de contrato pendientes** — ítems clasificados como
      decisión (CLI/restore multi-autoridad, etc.) no se implementan hasta
      ADR o ratificación explícita.

## Medio plazo

- [ ] **RPM (spec + COPR)** o **Flatpak** — Flatpak queda limitado porque la
      regla udev necesita un paso manual fuera del sandbox; si se hace, hay
      que documentarlo. Decisión pendiente según demanda.
- [ ] **Más teclados** — hacer configurables la firma del descriptor HID y
      la regla udev (VID/PID) vía variable de entorno o flag, si aparece
      demanda de otros modelos LampArray. Requiere Fase 5 / hardware.

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
- [x] v2.4.0 — un solo escritor HID, tag en la historia de `main`, `.deb` + tarball + SBOM en GitHub Releases
- [x] v2.4.1 — `getgrnam` portable (aarch64), paquete npm publicado
- [x] i18n real de la GUI — msgids en inglés y catálogos `es`/`en` completos (ADR 14)
- [x] CI: tests de la GUI — cobertura del control D-Bus de `main.py` (`Toggle`/`Present`/`Quit`) con un bus de sesión privado en la suite
- [x] v2.5.0 — i18n real (es/en), tests D-Bus de la ventana y release idempotente (`.deb` + tarball + SBOM + npm)
