# Matriz de compatibilidad

### Propósito de este documento

- **Objetivos:** Declarar qué combinaciones de distribución, GUI, sesión y
  arquitectura están verificadas, candidatas o fuera de alcance.
- **Estructura:** Ejes → distribuciones → bibliotecas gráficas → arquitecturas
  → Hyprland → estados.
- **Contenido a integrar según contexto:** Actualizar solo tras pruebas
  registradas. No anunciar un canal como instalable sin evidencia.

Estados: **probado** (evidencia en commit/artefacto), **candidato** (objetivo
del cierre, aún sin acreditación completa), **experimental** (documentado,
sin certificación), **no soportado** (excluido del alcance anunciado).

## Ejes distintos

1. Ejecutable CLI y dependencias del sistema.
2. GUI (GTK4 / libadwaita ≥ 1.5).
3. Bandeja, D-Bus, autostart y autorización de sesión.
4. Protocolo HID con el teclado AERO X16 admitido.

## Distribuciones

| Familia | Versión | CLI | GUI | Canal | Estado |
|---|---|---|---|---|---|
| Debian | 12 | Sí | No (libadwaita 1.2 &lt; 1.5) | `.deb` / script | CLI **candidato**; GUI **no soportada** (exclusión ratificada; no hay vía GTK3) |
| Debian | 13 | Sí | Si Adw ≥ 1.5 | `.deb` | **candidato** |
| Ubuntu | 22.04 | Sí | No anunciada | `.deb` / script | CLI **candidato**; GUI **no anunciada** hasta medir libadwaita ≥ 1.5 en esa versión |
| Ubuntu | 24.04 | Sí | Si Adw ≥ 1.5 | `.deb` | **candidato** |
| Ubuntu | 26.04 | Sí | Si Adw ≥ 1.5 | `.deb` | **candidato** |
| Fedora | Versión registrada en la release | Sí | Si Adw ≥ 1.5 | script / RPM futuro | **candidato**, empaquetado RPM **no verificado** |
| Arch Linux | Snapshot de repos registrado | Sí | Si Adw ≥ 1.5 | AUR / nativo | **candidato**; AUR **no anunciado** hasta checksum y chroot |
| openSUSE | — | — | — | — | **fuera del alcance** anunciado en esta revisión |

## Bibliotecas gráficas

Mínimo de la mesa de luz: **GTK 4** y **libadwaita 1.5** (`Adw.ToolbarView`,
`Adw.PreferencesDialog`, `Adw.AlertDialog`). Sin capa paralela de widgets
para Adw 1.2 en este cierre. La bandeja (Ayatana/GTK3) es dependencia
separada: sin host de bandeja la ventana sigue siendo usable.

## Arquitecturas

| Arquitectura | Alcance |
|---|---|
| x86_64 | Objetivo principal del hardware identificado (AERO X16). |
| ARM64 | Objetivo de software (binario); no se atribuye teclado de portátil sin caso real. |

Sistemas sin systemd: CLI/GUI manuales si el entorno lo permite; restauración
automática solo con un mecanismo alternativo probado y documentado.

## Hyprland

Hasta completar el perfil de pruebas (con y sin UWSM, con y sin bandeja,
con y sin agente polkit): **experimental**. No se edita `hyprland.conf` ni
Waybar automáticamente. Esta estabilización no lo pasa a soportado.

## Alcance ratificado en la estabilización

- Debian 12: la mesa de luz queda fuera de soporte. El preflight que
  rechaza libadwaita 1.2 no es una compatibilidad pendiente de recuperar
  en este cierre.
- Ubuntu 22.04: el CLI es candidato. La GUI no se anuncia.
- Varios usuarios: la exclusión mutua del candado en `/run/keybackcon`
  (ADR 13) es el contrato. No se anuncia una política distinta de «sesión
  gráfica activa» mientras el dispositivo siga gobernado por `uaccess`.
- El tag `v2.3.1` no se mueve. Su `Cargo.lock` no coincide con el
  `Cargo.toml` de ese tag y no es fuente de paquetes nuevos.

## Hardware

Perfil admitido: teclado AERO X16 vía HID LampArray con la firma y VID/PID
documentados en el protocolo. LampArray genérico de terceros:
**no soportado** en el cierre inicial.
