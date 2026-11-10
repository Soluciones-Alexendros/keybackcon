import os

try:
    from gui import api
except ImportError:  # ejecución directa: gui/ en sys.path
    import sys

    _here = os.path.dirname(os.path.abspath(__file__))
    _root = os.path.dirname(_here)
    if _root not in sys.path:
        sys.path.insert(0, _root)
    from gui import api

load, log_exc, _gettext_func = api(__file__)

try:
    import gi

    gi.require_version("Adw", "1")
    from gi.repository import Adw
except Exception:
    log_exc("Adw")
    Adw = None


VALID_MODES = ("system", "light", "dark")
SCHEMA_ID = "org.iniciativas.keybackcon"
THEME_KEY = "theme"
_schema_cache = {}


def lookup_settings(schema_id):
    """Gio.Settings para schema_id instalado, o None si no está disponible."""
    cached = _schema_cache.get(schema_id)
    if cached is not None:
        return cached
    try:
        from gi.repository import Gio

        source = Gio.SettingsSchemaSource.get_default()
        if source is None or source.lookup(schema_id, True) is None:
            return None
        settings = Gio.Settings.new(schema_id)
    except Exception:
        log_exc(f"gsettings {schema_id}")
        return None
    _schema_cache[schema_id] = settings
    return settings


def _config_file():
    base = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    return os.path.join(base, "keybackcon", "theme")


def get_theme() -> str:
    settings = lookup_settings(SCHEMA_ID)
    if settings is not None:
        try:
            mode = settings.get_string(THEME_KEY).strip().lower()
            if mode in VALID_MODES:
                return mode
        except Exception:
            log_exc("leer tema")
    try:
        with open(_config_file()) as f:
            mode = f.read().strip().lower()
            if mode in VALID_MODES:
                return mode
    except Exception:
        pass  # archivo ausente: es la vía normal, no un error
    return "system"


def _apply(mode):
    if Adw is None:
        return mode
    try:
        manager = Adw.StyleManager.get_default()
        if manager is None:
            return mode
        if mode == "light":
            manager.set_color_scheme(Adw.ColorScheme.FORCE_LIGHT)
        elif mode == "dark":
            manager.set_color_scheme(Adw.ColorScheme.FORCE_DARK)
        else:
            manager.set_color_scheme(Adw.ColorScheme.DEFAULT)
    except Exception:
        log_exc("aplicar tema")
    return mode


def set_theme(mode: str):
    normalized = (mode or "system").strip().lower()
    if normalized not in VALID_MODES:
        normalized = "system"
    try:
        path = _config_file()
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(normalized + "\n")
    except Exception:
        log_exc("guardar tema")
    settings = lookup_settings(SCHEMA_ID)
    if settings is not None:
        try:
            settings.set_string(THEME_KEY, normalized)
            settings.sync()
        except Exception:
            log_exc("tema en gsettings")
    return _apply(normalized)


def apply_saved_theme() -> str:
    return _apply(get_theme())
