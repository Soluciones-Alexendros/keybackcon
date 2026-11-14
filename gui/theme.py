import os
import re

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
    from gi.repository import Adw, Gio, GLib
except Exception:
    log_exc("Adw/Gio/GLib")
    Adw = None
    Gio = None
    GLib = None


VALID_MODES = ("system", "light", "dark")
SCHEMA_ID = "org.alexendros.ilutex"
THEME_KEY = "theme"
_schema_cache = {}

# Acento Omarchy (mismos valores que gui/constants.py, importados sin GTK).
OMARCHY_THEME_DIR = os.path.expanduser("~/.local/state/omarchy/current")
OMARCHY_COLORS_TOML = os.path.join(OMARCHY_THEME_DIR, "theme", "colors.toml")
OMARCHY_ACCENT_FALLBACK = "#FFD60A"

# Acento Omarchy
_accent = None
_accent_monitor = None
_accent_monitor_path = None
_accent_changed_callbacks = []


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
    return os.path.join(base, "ilutex", "theme")


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


# --- Acento Omarchy ---

def _read_omarchy_accent(path: str) -> str:
    """Lee el acento de colors.toml. Devuelve fallback si falla."""
    try:
        with open(path, "r", encoding="utf-8") as f:
            content = f.read()
        # Buscar: accent = "#xxxxxx" o accent = '#xxxxxx'
        match = re.search(r'accent\s*=\s*["\']?(#[0-9a-fA-F]{6})["\']?', content)
        if match:
            return match.group(1)
    except Exception:
        pass
    return OMARCHY_ACCENT_FALLBACK


def _accent_changed():
    """Notifica a todos los callbacks registrados."""
    global _accent
    for cb in _accent_changed_callbacks:
        try:
            cb(_accent)
        except Exception:
            log_exc("callback acento")


def _on_accent_file_changed(_monitor, _file, _other, event):
    """Callback cuando cambia el archivo colors.toml o su directorio."""
    global _accent
    if event in (Gio.FileMonitorEvent.CHANGED, Gio.FileMonitorEvent.CREATED, Gio.FileMonitorEvent.DELETED):
        # Re-armar el monitor si el archivo fue recreado (tema nuevo = symlink reemplazado)
        if event == Gio.FileMonitorEvent.CREATED or event == Gio.FileMonitorEvent.DELETED:
            _setup_accent_monitor()
        new_accent = _read_omarchy_accent(OMARCHY_COLORS_TOML)
        if new_accent != _accent:
            _accent = new_accent
            _accent_changed()


def _setup_accent_monitor():
    """Configura el FileMonitor para el acento Omarchy. Re-armable."""
    global _accent_monitor, _accent_monitor_path
    if Gio is None:
        return
    # Vigilar el directorio padre (theme/) para detectar reemplazo de symlinks
    theme_dir = os.path.dirname(OMARCHY_COLORS_TOML)
    if not os.path.isdir(theme_dir):
        return
    try:
        if _accent_monitor is not None:
            _accent_monitor.cancel()
    except Exception:
        pass
    try:
        f = Gio.File.new_for_path(theme_dir)
        _accent_monitor = f.monitor_directory(Gio.FileMonitorFlags.NONE, None)
        _accent_monitor.connect("changed", _on_accent_file_changed)
        _accent_monitor_path = theme_dir
    except Exception:
        log_exc("monitor acento Omarchy")


def init_omarchy_accent():
    """Inicializa la lectura y vigilancia del acento Omarchy. Llama una vez al arranque."""
    global _accent
    _accent = _read_omarchy_accent(OMARCHY_COLORS_TOML)
    _setup_accent_monitor()
    return _accent


def get_omarchy_accent() -> str:
    """Devuelve el acento actual (inicializa si es necesario)."""
    global _accent
    if _accent is None:
        return init_omarchy_accent()
    return _accent


def on_accent_changed(callback):
    """Registra un callback que se llama cuando cambia el acento.
    El callback recibe el nuevo acento (str)."""
    _accent_changed_callbacks.append(callback)
    # Devolver función para desregistrar
    def unregister():
        try:
            _accent_changed_callbacks.remove(callback)
        except ValueError:
            pass
    return unregister