# Matriz de accesibilidad (a11y) — iLutex

**Objetivo**: WCAG 2.2 AA / EN 301 549 para la GUI GTK4/Adwaita.
**Alcance**: Ventana principal (`MesaWindow`), bandeja (`TrayIndicator`), diálogos (`SettingsDialog`).

---

## 1. Roles ATK / AccessibleRole

| Componente | Role esperado | Implementación actual | Estado |
|---|---|---|---|
| Ventana principal | `window` (implícito `Adw.ApplicationWindow`) | OK | ✅ PASS |
| Toggle "Steady/Breathe/Rainbow" | `radio` en grupo | `Gtk.ToggleButton` con `AccessibleRole.RADIO` | ✅ PASS |
| Botón "Stop motion" | `button` | `Gtk.Button` | ✅ PASS |
| Slider brillo (0–100) | `slider` + `LABEL` + `VALUE_TEXT` | `Gtk.Scale` con `LABEL` y `VALUE_TEXT` dinámico | ✅ PASS |
| Botones − / + brillo | `button` + tooltip | `Gtk.Button` con tooltip | ✅ PASS |
| Label valor brillo ("60%") | `static` (texto de apoyo) | `Gtk.Label` mono-font | ✅ PASS |
| Swatches color (6×N) | `button` con `LABEL` accesible | `Gtk.Button` + `update_property` + `ACCESSIBLE_ROLE.BUTTON` | ✅ PASS |
| Selector tono personalizado | `color-chooser` / `button` | `Gtk.ColorDialogButton` | ✅ PASS |
| Botón "Preferencias" (header) | `button` + tooltip | `Gtk.Button` + tooltip | ✅ PASS |
| Estado conexión (dot + texto) | `status` (live region) | `Gtk.Label` con `AccessibleRole.STATUS` | ✅ PASS |
| Toast overlay | `alert` / `status` (live region) | `Adw.ToastOverlay` + `Adw.Toast` | ✅ PASS |
| Diálogo error (`Adw.AlertDialog`) | `dialog` + `alert` | `Adw.AlertDialog` | ✅ PASS |
| SettingsDialog (`Adw.PreferencesDialog`) | `dialog` | `Adw.PreferencesDialog` | ✅ PASS |

---

## 2. Orden de foco (Tab / Shift+Tab)

| Flujo | Estado |
|---|---|
| Header bar (prefs) → Stage preview → Segmento movimiento (3 toggles) → Stop → Slider brillo → − / + → Swatches (FlowBox) → Tono personalizado → Cerrar | ⚠️ PARTIAL (FlowBox no enfocable por teclado por defecto) |
| Escape cierra diálogo / toast | ✅ PASS |
| Foco visible (outline) | ✅ PASS (Adwaita) |

---

## 3. Contraste (WCAG 2.2 AA)

| Elemento | Ratio mínimo | Medición | Estado |
|---|---|---|---|
| Texto principal sobre fondo (`@window_fg_color` / `@window_bg_color`) | 4.5:1 | Adwaita nativo | ✅ PASS |
| Texto secundario (hints, mono-font 0.75 alpha) | 4.5:1 | `alpha(@window_fg_color, 0.75)` ≈ 3.5:1 | ❌ FAIL |
| Swatch seleccionado (borde `@accent_color`) | 3:1 (UI components) | OK si acento ≥ #666 | ⚠️ PARTIAL |
| Slider brillo (track/thumb) | 3:1 | Adwaita nativo | ✅ PASS |
| Toast / alertas | 4.5:1 | Adwaita nativo | ✅ PASS |

---

## 4. Lector de pantalla (Orca / AT-SPI)

| Caso | Estado |
|---|---|
| Nombres accesibles en swatches ("Naranja · #ff7800") | ✅ PASS (via `update_property LABEL`) |
| Slider brillo anuncia valor al mover | ⚠️ PARTIAL (`VALUE_TEXT` seteado solo al inicio) |
| Toggle movimiento anuncia estado (pressed/unchecked) | ❌ FAIL (falta grupo radio + estado) |
| Estado conexión anunciado al cambiar | ❌ FAIL (no live region) |
| Toast anunciado automáticamente | ✅ PASS (`Adw.Toast`) |
| Diálogo error anunciado | ✅ PASS (`Adw.AlertDialog`) |
| Tono personalizado (`ColorDialogButton`) | ✅ PASS (GTK nativo) |

---

## 5. Reduced Motion (`prefers-reduced-motion`)

| Área | Estado |
|---|---|
| Preview partículas (breathe/rainbow) | ⚠️ PARTIAL (`animations_enabled()` lee `gtk-enable-animations` pero no respeta `prefers-reduced-motion` media query) |
| Toast/transiciones Adwaita | ✅ PASS (GTK respeta setting) |
| CSS animations | ✅ PASS (pocas, en preview) |

---

## 6. Acciones correctivas (prioridad)

### Corregido

1. **A1-1**: Swatches en `FlowBox` enfocables (`focusable`, `can_focus`) con navegación por flechas.
2. **A1-2**: Segmento movimiento con `Gtk.AccessibleRole.RADIO`. Sin `update_state`: con un
   booleano de Python dispara un warning de `GValue`, y GTK ya deduce el estado del `ToggleButton`.
3. **A1-3**: Slider brillo con `VALUE_TEXT` dinámico en `notify::value`.
4. **A1-4**: Estado de conexión con `Gtk.AccessibleRole.STATUS`.
5. **A1-5**: `.section-hint` pasa de `alpha(…, 0.75)` a `0.85`.

Ojo con la API: `Gtk.AccessibleProperty` **no** tiene `LIVE` ni `ROLE` en GTK 4.24, y
usarlos dentro de un `except` silencioso deja la accesibilidad sin aplicar. El rol se
fija con `set_accessible_role()`.

### Alto
6. **A2-1**: `animations_enabled()` — consultar `Gtk.Settings:gtk-enable-animations` Y media query `prefers-reduced-motion`.
7. **A2-2**: Tooltip en swatches — ya OK, verificar longitud.

### Medio
8. **A3-1**: Documentar atajos de teclado en `README.md` y overlay `?` (módulo `omarchy-keyboard`).
9. **A3-2**: Verificar navegación completa solo teclado (Tab/Shift+Tab/Enter/Espacio/Flechas).

---

## 7. Verificación

```bash
# Tests unitarios existentes
python3 -m unittest discover -s gui/tests

# Inspección manual con Accerciser (GTK Inspector)
# 1. Ejecutar: GTK_DEBUG=interactive ilutex-gui
# 2. Ver árbol ATK, roles, estados, relaciones

# Contraste: usar GNOME Settings → Accessibility → High Contrast
# Reduced motion: Settings → Accessibility → Reduce Motion
# Orca: orca --replace & ilutex-gui
```

---

## 8. Referencias

- [WCAG 2.2 AA](https://www.w3.org/TR/WCAG22/)
- [EN 301 549](https://www.etsi.org/deliver/etsi_en/301500_301599/301549/03.01.01_60/en_301549v030101p.pdf)
- [GTK4 Accessibility](https://docs.gtk.org/gtk4/a11y.html)
- [Adwaita a11y](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/a11y.html)