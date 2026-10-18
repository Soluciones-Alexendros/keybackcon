import os
import threading

try:
    from gui import load, log_exc, gettext_func
except ImportError:  # ejecución directa: gui/ en sys.path
    import importlib
    import sys

    def load(nombre):
        return importlib.import_module(nombre)

    def gettext_func():
        mod = importlib.import_module("i18n")
        return mod._

    def log_exc(contexto):
        tipo, exc, _tb = sys.exc_info()
        print(f"keybackcon-gui: {contexto}: {tipo.__name__}: {exc}",
              file=sys.stderr)

try:
    import gi

    gi.require_version("Gtk", "4.0")
    gi.require_version("Adw", "1")
    from gi.repository import Adw, Gio, Gtk, GLib
except Exception:
    log_exc("Gtk/Adw")
    Adw = None
    Gio = None
    Gtk = None
    GLib = None

_theme = load("theme")
get_theme = _theme.get_theme
set_theme = _theme.set_theme
lookup_settings = _theme.lookup_settings

try:
    from gui import __version__ as APP_VERSION
except ImportError:
    try:
        from . import __version__ as APP_VERSION
    except ImportError:
        APP_VERSION = "2.2.1"

_ = gettext_func()


SCHEMA_ID = "org.iniciativas.keybackcon"
EFFECTS_KEY = "effects"
RESTORE_KEY = "restore-on-login"
REPO_URL = "https://github.com/Soluciones-Alexendros/keybackcon"
AUTOSTART_NAME = "keybackcon-tray.desktop"
AUTOSTART_EXEC = "keybackcon-gui --tray"


def get_settings():
    if Gio is None:
        return None
    return lookup_settings(SCHEMA_ID)


def _pref_file(name):
    base = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    return os.path.join(base, "keybackcon", name)


def _read_bool_file(name, default):
    try:
        with open(_pref_file(name)) as f:
            text = f.read().strip().lower()
        if text in ("1", "true", "yes", "on"):
            return True
        if text in ("0", "false", "no", "off"):
            return False
    except OSError:
        pass  # ausente o ilegible: valor por defecto
    return default


def _write_bool_file(name, value):
    try:
        path = _pref_file(name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("true\n" if value else "false\n")
    except Exception:
        log_exc(f"guardar {name}")
    return bool(value)


def is_effects_enabled():
    settings = get_settings()
    if settings is not None:
        try:
            return bool(settings.get_boolean(EFFECTS_KEY))
        except Exception:
            log_exc("leer effects")
    return _read_bool_file("effects", True)


def set_effects_enabled(enabled):
    value = bool(enabled)
    settings = get_settings()
    if settings is not None:
        try:
            settings.set_boolean(EFFECTS_KEY, value)
            settings.sync()
        except Exception:
            log_exc("guardar effects")
    return _write_bool_file("effects", value)


def is_restore_enabled():
    settings = get_settings()
    if settings is not None:
        try:
            return bool(settings.get_boolean(RESTORE_KEY))
        except Exception:
            log_exc("leer restore-on-login")
    return _read_bool_file("restore-on-login", True)


def set_restore_enabled(enabled):
    value = bool(enabled)
    settings = get_settings()
    if settings is not None:
        try:
            settings.set_boolean(RESTORE_KEY, value)
            settings.sync()
        except Exception:
            log_exc("guardar restore-on-login")
    return _write_bool_file("restore-on-login", value)


def _autostart_file():
    base = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    return os.path.join(base, "autostart", AUTOSTART_NAME)


def _autostart_content():
    return (
        "[Desktop Entry]\n"
        "Type=Application\n"
        "Name=keybackcon\n"
        "Exec=%s\n"
        "Hidden=false\n"
        "X-GNOME-Autostart-enabled=true\n" % AUTOSTART_EXEC
    )


def is_autostart_enabled():
    try:
        with open(_autostart_file()) as f:
            content = f.read()
        if "Hidden=true" in content:
            return False
        return "Exec=%s" % AUTOSTART_EXEC in content
    except Exception:
        return False


def set_autostart_enabled(enabled):
    path = _autostart_file()
    if not enabled:
        try:
            os.unlink(path)
        except Exception:
            pass  # no existía: ya estaba desactivado
        return False
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(_autostart_content())
    except Exception:
        log_exc("autostart")
        return False
    return True


if Adw is not None:
    class SettingsDialog(Adw.PreferencesDialog):
        def __init__(self, on_effects_changed=None, client=None):
            super().__init__()
            self._on_effects_changed = on_effects_changed
            self._client = client
            self.set_title(_("Preferences"))
            self.add(self._appearance_page())
            self.add(self._behavior_page())
            self.add(self._about_page())

        def _appearance_page(self):
            page = Adw.PreferencesPage()
            page.set_title(_("Appearance"))
            page.set_icon_name("preferences-desktop-theme-symbolic")
            group = Adw.PreferencesGroup()
            page.add(group)
            theme_row = Adw.ComboRow()
            theme_row.set_title(_("Theme"))
            theme_row.set_model(Gtk.StringList.new([_("System"), _("Light"), _("Dark")]))
            modes = ("system", "light", "dark")
            try:
                current = get_theme()
            except Exception:
                log_exc("tema actual")
                current = "system"
            if current not in modes:
                current = "system"
            theme_row.set_selected(modes.index(current))
            theme_row.connect("notify::selected", self._on_theme_changed)
            group.add(theme_row)
            effects_row = Adw.SwitchRow()
            effects_row.set_title(_("Stage effects"))
            effects_row.set_subtitle(_("Particles and animation in the preview."))
            try:
                effects_row.set_active(is_effects_enabled())
            except Exception:
                log_exc("leer efectos")
                effects_row.set_active(True)
            effects_row.connect("notify::active", self._on_effects_toggled)
            group.add(effects_row)
            return page

        def _behavior_page(self):
            page = Adw.PreferencesPage()
            page.set_title(_("Behavior"))
            page.set_icon_name("preferences-system-symbolic")
            group = Adw.PreferencesGroup()
            page.add(group)
            autostart_row = Adw.SwitchRow()
            autostart_row.set_title(_("Launch at login"))
            autostart_row.set_subtitle(_("Open in the tray when the session starts."))
            try:
                autostart_row.set_active(is_autostart_enabled())
            except Exception:
                log_exc("leer autostart")
                autostart_row.set_active(False)
            autostart_row.connect("notify::active", self._on_autostart_toggled)
            group.add(autostart_row)
            restore_row = Adw.SwitchRow()
            restore_row.set_title(_("Restore color at startup"))
            restore_row.set_subtitle(_("Apply the last color when opening."))
            try:
                restore_row.set_active(is_restore_enabled())
            except Exception:
                log_exc("leer restore")
                restore_row.set_active(True)
            restore_row.connect("notify::active", self._on_restore_toggled)
            group.add(restore_row)
            restore_now = Adw.ActionRow()
            restore_now.set_title(_("Restore color now"))
            restore_now.set_subtitle(_("Applies the last saved color."))
            restore_now.set_activatable(True)
            restore_now.connect("activated", self._on_restore_now)
            group.add(restore_now)
            return page

        def _about_page(self):
            page = Adw.PreferencesPage()
            page.set_title(_("About"))
            page.set_icon_name("help-about-symbolic")
            group = Adw.PreferencesGroup()
            group.set_title("keybackcon")
            page.add(group)
            version_row = Adw.ActionRow()
            version_row.set_title(_("Version"))
            version_row.add_suffix(Gtk.Label(label=APP_VERSION))
            group.add(version_row)
            license_row = Adw.ActionRow()
            license_row.set_title(_("License"))
            license_row.add_suffix(Gtk.Label(label="MIT"))
            group.add(license_row)
            link_row = Adw.ActionRow()
            link_row.set_title(_("Source code"))
            link_row.set_subtitle(REPO_URL)
            link_row.set_activatable(True)
            link_row.connect("activated", self._on_repo_activated)
            group.add(link_row)
            device_row = Adw.ActionRow()
            device_row.set_title(_("Device"))
            device_row.set_subtitle(self._device_line())
            group.add(device_row)
            if self._client is not None:
                threading.Thread(
                    target=self._fetch_device_line, args=(device_row,),
                    daemon=True,
                ).start()
            return page

        def _on_theme_changed(self, row, _pspec):
            modes = ("system", "light", "dark")
            try:
                mode = modes[row.get_selected()]
            except Exception:
                log_exc("tema elegido")
                return
            try:
                set_theme(mode)
            except Exception:
                log_exc("aplicar tema")

        def _on_effects_toggled(self, row, _pspec):
            try:
                enabled = bool(row.get_active())
            except Exception:
                log_exc("efectos")
                return
            try:
                set_effects_enabled(enabled)
            except Exception:
                log_exc("guardar efectos")
            if self._on_effects_changed is not None:
                try:
                    self._on_effects_changed(enabled)
                except Exception:
                    log_exc("avisar efectos")

        def _on_autostart_toggled(self, row, _pspec):
            try:
                enabled = bool(row.get_active())
            except Exception:
                log_exc("autostart")
                return
            try:
                set_autostart_enabled(enabled)
            except Exception:
                log_exc("guardar autostart")

        def _on_restore_toggled(self, row, _pspec):
            try:
                enabled = bool(row.get_active())
            except Exception:
                log_exc("restore-on-login")
                return
            try:
                set_restore_enabled(enabled)
            except Exception:
                log_exc("guardar restore-on-login")

        def _on_repo_activated(self, _row):
            try:
                Gtk.show_uri(None, REPO_URL, 0)
            except Exception:
                log_exc("abrir repositorio")

        def _device_line(self):
            if self._client is None:
                return _("no keyboard in sight")
            return _("looking at the keyboard…")

        def _fetch_device_line(self, row):
            texto = _("no keyboard in sight")
            try:
                info = self._client.info()
            except Exception:
                log_exc("info del dispositivo")
                info = None
            if info:
                hid = str(info.get("device") or "?")
                lamps = info.get("lamps", "?")
                try:
                    texto = f"{hid} · {lamps} {_('zone(s)')}"
                except Exception:
                    log_exc("ficha de dispositivo")
            if GLib is not None:
                GLib.idle_add(self._set_device_line, row, texto)

        def _set_device_line(self, row, texto):
            try:
                row.set_subtitle(texto)
            except Exception:
                log_exc("ficha de dispositivo")
            return False

        def _on_restore_now(self, _row):
            try:
                if self._client is None:
                    return
                self._client.restore()
            except Exception:
                log_exc("restaurar ahora")
