import math
import os
import sys

try:
    from gui import load, log_exc, gettext_func
except ImportError:  # ejecución directa: gui/ en sys.path
    import importlib

    def load(nombre):
        return importlib.import_module(nombre)

    def gettext_func():
        mod = importlib.import_module("i18n")
        return mod._

    def log_exc(contexto):
        tipo, exc, _tb = sys.exc_info()
        print(f"keybackcon-gui: {contexto}: {tipo.__name__}: {exc}",
              file=sys.stderr)

_ = gettext_func()
KeybackconError = load("client").KeybackconError
_colors = load("colors")
hex_to_rgb = _colors.hex_to_rgb
hsv_to_rgb = _colors.hsv_to_rgb
COLORES = _colors.COLORES


class TrayUnavailable(Exception):
    pass


try:
    import gi

    gi.require_version("Gtk", "3.0")
    gi.require_version("AyatanaAppIndicator3", "0.1")
    from gi.repository import AyatanaAppIndicator3, Gtk, GLib
except Exception as exc:
    raise TrayUnavailable(
        "Bandeja no disponible: instala gir1.2-ayatanaappindicator3-0.1 (%s)"
        % exc
    ) from exc

try:
    import cairo
except Exception as exc:
    raise TrayUnavailable(
        "Bandeja no disponible: instala python3-cairo (%s)" % exc
    ) from exc


MODES = ("fijar", "breathe", "rainbow", "off")
BRILLO_PASO = 5


def _normalize_hex(color_hex):
    try:
        h = str(color_hex).strip().lstrip("#").lower()
        if len(h) != 6:
            return "ffffff"
        int(h, 16)
        return h
    except Exception:
        return "ffffff"


def _normalize_mode(mode):
    if mode in MODES:
        return mode
    if mode == "unknown":
        return "unknown"
    return "fijar"


def _clamp_brightness(value):
    try:
        v = int(value)
    except Exception:
        v = 100
    return max(0, min(100, v))


def _cache_dir():
    base = os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache"))
    path = os.path.join(base, "keybackcon")
    os.makedirs(path, exist_ok=True)
    return path


def render_tray_icon(color_hex, mode):
    color_hex = _normalize_hex(color_hex)
    mode = _normalize_mode(mode)
    path = os.path.join(_cache_dir(), "tray-%s-%s.png" % (color_hex, mode))
    size = 64
    cx = cy = size / 2.0
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, size, size)
    ctx = cairo.Context(surface)
    ctx.set_operator(cairo.OPERATOR_CLEAR)
    ctx.paint()
    ctx.set_operator(cairo.OPERATOR_OVER)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    if mode == "unknown":
        mode = "fijar"
    if mode == "off":
        ctx.set_source_rgb(0.16, 0.18, 0.22)
        ctx.arc(cx, cy, 20, 0, 2 * math.pi)
        ctx.fill()
        ctx.set_source_rgb(0.55, 0.58, 0.65)
        ctx.set_line_width(2.5)
        ctx.arc(cx, cy, 20, 0, 2 * math.pi)
        ctx.stroke()
        ctx.set_source_rgba(0.95, 0.94, 0.90, 0.9)
        ctx.set_line_width(3.5)
        ctx.move_to(cx - 10, cy + 10)
        ctx.line_to(cx + 10, cy - 10)
        ctx.stroke()
    else:
        r, g, b = hex_to_rgb(color_hex)
        if mode == "breathe":
            ctx.set_source_rgba(r, g, b, 0.25)
            ctx.arc(cx, cy, 26, 0, 2 * math.pi)
            ctx.fill()
            ctx.set_source_rgba(r, g, b, 0.55)
            ctx.set_line_width(2.0)
            ctx.arc(cx, cy, 23, 0, 2 * math.pi)
            ctx.stroke()
        if mode == "rainbow":
            for i in range(6):
                hr, hg, hb = hsv_to_rgb(i * 60.0, 1.0, 1.0)
                ctx.set_source_rgb(hr, hg, hb)
                ctx.set_line_width(4.0)
                a0 = math.radians(i * 60 + 4)
                a1 = math.radians((i + 1) * 60 - 4)
                ctx.arc(cx, cy, 24.5, a0, a1)
                ctx.stroke()
        ctx.set_source_rgb(r, g, b)
        ctx.arc(cx, cy, 20, 0, 2 * math.pi)
        ctx.fill()
        ctx.set_source_rgba(1, 1, 1, 0.9)
        ctx.set_line_width(2.5)
        ctx.arc(cx, cy, 20, 0, 2 * math.pi)
        ctx.stroke()
        ctx.set_source_rgba(1, 1, 1, 0.18)
        ctx.arc(cx - 6, cy - 7, 6, 0, 2 * math.pi)
        ctx.fill()
    surface.write_to_png(path)
    return path


class TrayIndicator:
    def __init__(self, client, on_toggle_window, on_preferences=None,
                 on_about=None, on_quit=None):
        self._client = client
        self._on_toggle_window = on_toggle_window
        self._on_preferences = on_preferences
        self._on_about = on_about
        self._on_quit = on_quit
        self._mode = "fijar"
        self._color = "ffffff"
        self._brightness = 100
        self._syncing_menu = False
        self._syncing_scale = False
        self._bright_source = None
        try:
            color, pct = client.get_state_file()
            self._color = _normalize_hex(color)
            self._brightness = _clamp_brightness(pct)
        except Exception:
            log_exc("estado inicial de bandeja")
        self._pending = []
        self._visible = True
        self._indicator = AyatanaAppIndicator3.Indicator.new(
            "keybackcon",
            "keybackcon",
            AyatanaAppIndicator3.IndicatorCategory.HARDWARE,
        )
        self._indicator.set_title("keybackcon")
        self._indicator.set_status(AyatanaAppIndicator3.IndicatorStatus.ACTIVE)
        self._build_menu()
        self._render_icon()
        try:
            self._indicator.connect("scroll-event", self._on_scroll)
        except Exception:
            log_exc("scroll del indicador")
        for signal in ("activate", "activate-event"):
            try:
                self._indicator.connect(signal, self._on_activate_signal)
                break
            except Exception:
                continue
        try:
            GLib.timeout_add_seconds(2, self._tick)
        except Exception:
            log_exc("temporizador de bandeja")

    # --- construcción del menú -------------------------------------------

    def _build_menu(self):
        menu = Gtk.Menu()
        self._open_item = Gtk.MenuItem.new_with_label(_("Abrir ventana"))
        self._open_item.connect("activate", self._ui_open)
        menu.append(self._open_item)
        menu.append(Gtk.SeparatorMenuItem())
        menu.append(self._build_brightness_item())
        self._colors_item = Gtk.MenuItem.new_with_label(_("Colores"))
        self._colors_item.set_submenu(self._build_colors_menu())
        menu.append(self._colors_item)
        self._anims_item = Gtk.MenuItem.new_with_label(_("Animaciones"))
        self._anims_item.set_submenu(self._build_modes_menu())
        menu.append(self._anims_item)
        self._off_item = Gtk.MenuItem.new_with_label(_("Apagar"))
        self._off_item.connect("activate", self._do_off)
        menu.append(self._off_item)
        menu.append(Gtk.SeparatorMenuItem())
        self._restore_item = Gtk.MenuItem.new_with_label(_("Restaurar"))
        self._restore_item.connect("activate", self._do_restore)
        menu.append(self._restore_item)
        self._prefs_item = Gtk.MenuItem.new_with_label(_("Preferencias"))
        if self._on_preferences is None:
            self._prefs_item.set_sensitive(False)
        else:
            self._prefs_item.connect("activate", self._ui_preferences)
        menu.append(self._prefs_item)
        self._about_item = Gtk.MenuItem.new_with_label(_("Acerca de"))
        if self._on_about is None:
            self._about_item.set_sensitive(False)
        else:
            self._about_item.connect("activate", self._ui_about)
        menu.append(self._about_item)
        self._quit_item = Gtk.MenuItem.new_with_label(_("Salir"))
        self._quit_item.connect("activate", self._ui_quit)
        menu.append(self._quit_item)
        menu.show_all()
        self._menu = menu
        self._indicator.set_menu(menu)
        self._sync_menus()
        try:
            self._indicator.set_secondary_activate_target(self._open_item)
        except Exception:
            log_exc("activación secundaria")

    def _build_brightness_item(self):
        self._scale = Gtk.Scale.new_with_range(
            Gtk.Orientation.HORIZONTAL, 0, 100, 1
        )
        self._scale.set_size_request(170, -1)
        self._scale.set_draw_value(False)
        self._scale.set_value(self._brightness)
        self._scale.connect("value-changed", self._on_scale_brillo)
        caja = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        caja.pack_start(Gtk.Label(label=_("Brillo")), False, False, 6)
        caja.pack_start(self._scale, True, True, 6)
        item = Gtk.MenuItem()
        item.add(caja)
        return item

    def _build_colors_menu(self):
        submenu = Gtk.Menu()
        self._color_items = {}
        grupo = None
        for nombre, hexv in COLORES:
            if grupo is None:
                item = Gtk.RadioMenuItem.new_with_label(None, _(nombre))
            else:
                item = Gtk.RadioMenuItem.new_with_label_from_widget(grupo, _(nombre))
            grupo = item
            item.connect("activate", self._on_color_activado, hexv)
            submenu.append(item)
            self._color_items[hexv] = item
        submenu.append(Gtk.SeparatorMenuItem())
        self._custom_item = Gtk.MenuItem.new_with_label(_("Tu tono…"))
        self._custom_item.connect("activate", self._do_custom_tone)
        submenu.append(self._custom_item)
        submenu.show_all()
        return submenu

    def _build_modes_menu(self):
        submenu = Gtk.Menu()
        self._mode_items = {}
        grupo = None
        for key, label in (
            ("fijar", _("Fijar")),
            ("breathe", _("Respirar")),
            ("rainbow", _("Arcoíris")),
        ):
            if grupo is None:
                item = Gtk.RadioMenuItem.new_with_label(None, label)
            else:
                item = Gtk.RadioMenuItem.new_with_label_from_widget(grupo, label)
            grupo = item
            item.connect("activate", self._on_mode_activado, key)
            submenu.append(item)
            self._mode_items[key] = item
        submenu.show_all()
        return submenu

    def _sync_menus(self):
        self._syncing_menu = True
        try:
            modo = self._mode if self._mode in self._mode_items else "fijar"
            for key, item in self._mode_items.items():
                item.set_active(key == modo)
            for hexv, item in self._color_items.items():
                item.set_active(hexv == self._color)
        except Exception:
            log_exc("sincronizar menús")
        finally:
            self._syncing_menu = False

    # --- infraestructura: reintentos, icono, salida limpia ---------------

    def _safe(self, fn):
        try:
            fn()
        except KeybackconError:
            self._pending.append(fn)
        except Exception:
            log_exc("acción de bandeja")

    def _reintentar_pendientes(self):
        if not self._pending:
            return
        rest = []
        for fn in self._pending:
            try:
                fn()
            except KeybackconError:
                rest.append(fn)
            except Exception:
                log_exc("reintento de bandeja")
        self._pending = rest

    def _tick(self):
        self._reintentar_pendientes()
        self._sync_desde_archivos()
        return True

    def _sync_desde_archivos(self):
        """Sincroniza con archivo de estado + pidfile (nunca subprocess)."""
        try:
            color, pct = self._client.get_state_file()
        except Exception:
            log_exc("estado de bandeja")
            return True
        color = _normalize_hex(color)
        pct = _clamp_brightness(pct)
        cambio = False
        if color != self._color:
            self._color = color
            cambio = True
        if pct != self._brightness and not self._syncing_scale:
            self._brightness = pct
            self._syncing_scale = True
            try:
                self._scale.set_value(pct)
            except Exception:
                log_exc("sincronizar brillo")
            finally:
                self._syncing_scale = False
        try:
            pid = self._client.animation_running()
        except Exception:
            log_exc("pid de animación")
            pid = None
        if pid:
            if self._mode in ("fijar", "off"):
                # No inventar breathe: preferir modo persistido o «desconocido».
                modo_archivo = None
                try:
                    info = getattr(self._client, "info", None)
                    if callable(info):
                        data = info()
                        if isinstance(data, dict):
                            modo_archivo = data.get("mode")
                except Exception:
                    log_exc("modo desde info")
                if modo_archivo in ("breathe", "rainbow"):
                    self._mode = modo_archivo
                else:
                    self._mode = "unknown"
                cambio = True
        elif self._mode in ("breathe", "rainbow", "unknown"):
            self._mode = "fijar"
            cambio = True
        if cambio:
            self._sync_menus()
            self._render_icon()
        return True

    def _render_icon(self):
        try:
            path = render_tray_icon(self._color, self._mode)
        except Exception:
            log_exc("icono de bandeja")
            return
        try:
            self._indicator.set_icon_full(path, "keybackcon")
        except Exception:
            try:
                self._indicator.set_icon(path)
            except Exception:
                log_exc("poner icono")

    # --- callbacks de la UI ------------------------------------------------

    def _ui_open(self, *args):
        try:
            self._on_toggle_window()
        except Exception:
            log_exc("abrir ventana")

    def _on_activate_signal(self, *args):
        self._ui_open()

    def _ui_preferences(self, *args):
        try:
            self._on_preferences()
        except Exception:
            log_exc("preferencias")

    def _ui_about(self, *args):
        try:
            self._on_about()
        except Exception:
            log_exc("acerca de")

    def _ui_quit(self, *args):
        self.quit()

    def _on_color_activado(self, item, hexv):
        if self._syncing_menu:
            return
        self._do_color(hexv)

    def _on_mode_activado(self, item, key):
        if self._syncing_menu:
            return
        if key == "fijar":
            self._do_fijar()
        else:
            self._do_animation(key)

    def _on_scale_brillo(self, scale):
        if self._syncing_scale:
            return
        try:
            pct = int(round(scale.get_value()))
        except Exception:
            log_exc("brillo de bandeja")
            return
        self._brightness = pct
        if self._bright_source is not None:
            try:
                GLib.source_remove(self._bright_source)
            except Exception:
                log_exc("cancelar brillo pendiente")
        self._bright_source = GLib.timeout_add(150, self._aplicar_brillo, pct)

    def _aplicar_brillo(self, pct):
        self._bright_source = None
        self._do_brightness(pct)
        return GLib.SOURCE_REMOVE

    # --- acciones sobre el teclado -----------------------------------------

    def _do_color(self, hexv):
        self._color = _normalize_hex(hexv)
        self._mode = "fijar"
        self._sync_menus()
        self._render_icon()
        self._safe(lambda: (self._client.stop(), self._client.set_color(self._color)))

    def _do_fijar(self, *args):
        self._do_color(self._color)

    def _do_animation(self, mode):
        self._mode = _normalize_mode(mode)
        if self._mode == "off":
            self._mode = "fijar"
        self._sync_menus()
        self._render_icon()
        self._safe(lambda: self._client.animation(self._mode))

    def _do_off(self, *args):
        self._mode = "off"
        self._sync_menus()
        self._render_icon()
        self._safe(lambda: self._client.off())

    def _do_restore(self, *args):
        def accion():
            self._client.restore()
            self._sync_desde_archivos()

        self._safe(accion)

    def _do_custom_tone(self, *args):
        dlg = None
        try:
            dlg = Gtk.ColorChooserDialog(title=_("Tu tono…"))
            respuesta = dlg.run()
            if respuesta == Gtk.ResponseType.OK:
                rgba = dlg.get_rgba()
                hexv = "%02x%02x%02x" % (
                    int(rgba.red * 255),
                    int(rgba.green * 255),
                    int(rgba.blue * 255),
                )
                self._do_color(hexv)
        except Exception:
            log_exc("elegir tono")
        finally:
            if dlg is not None:
                try:
                    dlg.destroy()
                except Exception:
                    log_exc("cerrar selector de tono")

    def _do_brightness(self, target):
        target = _clamp_brightness(target)
        if target != self._brightness:
            self._brightness = target
            self._syncing_scale = True
            try:
                self._scale.set_value(target)
            except Exception:
                log_exc("ajustar brillo")
            finally:
                self._syncing_scale = False
        self._safe(lambda: self._client.set_brightness(target))

    def _on_scroll(self, _indicator, steps, _orientation, *args):
        try:
            delta = BRILLO_PASO if int(steps) > 0 else -BRILLO_PASO
        except Exception:
            log_exc("rueda de brillo")
            return True
        self._do_brightness(self._brightness + delta)
        return True

    # --- API pública ---------------------------------------------------------

    def set_visible(self, visible):
        self._visible = bool(visible)
        try:
            if self._visible:
                self._indicator.set_status(
                    AyatanaAppIndicator3.IndicatorStatus.ACTIVE
                )
            else:
                self._indicator.set_status(
                    AyatanaAppIndicator3.IndicatorStatus.PASSIVE
                )
        except Exception:
            log_exc("visibilidad del indicador")

    def update_state(self, mode: str, color_hex: str, brightness: int):
        self._mode = _normalize_mode(mode)
        self._color = _normalize_hex(color_hex)
        self._brightness = _clamp_brightness(brightness)
        self._sync_menus()
        self._render_icon()

    def quit(self):
        """Salida limpia: para la animación y devuelve el control a quien
        posee el bucle GTK (callback), nunca ``os._exit``."""
        try:
            self._client.stop()
        except Exception:
            log_exc("parar al salir")
        try:
            self._indicator.set_status(
                AyatanaAppIndicator3.IndicatorStatus.PASSIVE
            )
        except Exception:
            log_exc("ocultar indicador")
        try:
            if Gtk.main_level() > 0:
                Gtk.main_quit()
        except Exception:
            log_exc("salir del bucle de bandeja")
        if self._on_quit is not None:
            try:
                self._on_quit()
            except Exception:
                log_exc("callback de salida")
