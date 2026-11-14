import gettext
import locale
import os

try:
    from gi.repository import Gio
except Exception:
    Gio = None

APP_NAME = "ilutex"
SUPPORTED_LOCALES = {"es": "Español", "en": "English"}

_current_lang = "es"
_translation = gettext.NullTranslations()
_ = _translation.gettext

# Vigilancia del idioma de Omarchy (recarga en vivo).
_language_monitor = None
_language_callbacks = []


def get_locale_dir() -> str:
    here = os.path.join(os.path.dirname(os.path.abspath(__file__)), "locale")
    home = os.path.expanduser("~/.local/share/locale")
    system = "/usr/share/locale"
    for path in (here, home, system):
        if os.path.isdir(path):
            return path
    return here


def omarchy_language_file() -> str:
    """Ruta del fichero de idioma de Omarchy (Setup > Language)."""
    base = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
    return os.path.join(base, "omarchy", "language")


def _normalize_code(code: str) -> str:
    lang = (code or "es").replace("-", "_").split("_")[0].lower()
    if lang not in SUPPORTED_LOCALES:
        return "es"
    return lang


def set_language(code: str) -> str:
    global _current_lang, _translation, _
    lang = _normalize_code(code)
    _current_lang = lang
    try:
        _translation = gettext.translation(
            APP_NAME, localedir=get_locale_dir(), languages=[lang], fallback=True
        )
    except Exception:
        _translation = gettext.NullTranslations()
    _ = _translation.gettext
    return _current_lang


def get_language() -> str:
    return _current_lang


def N_(message: str) -> str:
    """Marca una cadena como traducible sin traducirla.

    Para literales que se traducen en el punto de uso (p. ej. los nombres de
    color de ``colors.py``): xgettext la extrae y el ``_()`` del consumidor
    resuelve el idioma activo.
    """
    return message


def _system_locale() -> str | None:
    try:
        return locale.getdefaultlocale()[0]
    except Exception:
        return None


def resolve_language(explicit: str | None = None) -> str:
    """Idioma efectivo: explícito > Omarchy > locale del sistema > español.

    Omarchy guarda el idioma elegido en ``~/.config/omarchy/language``
    (p. ej. ``es-MX``); se normaliza a ``es``/``en``.
    """
    if explicit:
        return _normalize_code(explicit)
    try:
        with open(omarchy_language_file()) as f:
            code = f.read().strip()
        if code:
            return _normalize_code(code)
    except OSError:
        pass  # sin Omarchy: seguimos con el locale del sistema
    return _normalize_code(_system_locale() or "es")


def setup_i18n(lang: str | None = None) -> str:
    return set_language(resolve_language(lang))


# --- Re-traducción en vivo ------------------------------------------------

def on_language_changed(callback):
    """Registra un callback al cambiar el idioma en Omarchy.

    El callback recibe el código normalizado (``es``/``en``). Devuelve la
    función para desregistrarlo.
    """
    _language_callbacks.append(callback)

    def unregister():
        try:
            _language_callbacks.remove(callback)
        except ValueError:
            pass

    return unregister


def _notify_language(new_lang: str):
    for cb in list(_language_callbacks):
        try:
            cb(new_lang)
        except Exception:
            pass


def _on_language_file_changed(_monitor, _file, _other, event):
    """Re-lee el idioma cuando Omarchy cambia el fichero."""
    global _language_monitor
    if Gio is not None and event in (
        Gio.FileMonitorEvent.CHANGED,
        Gio.FileMonitorEvent.CREATED,
        Gio.FileMonitorEvent.DELETED,
    ):
        if event in (Gio.FileMonitorEvent.CREATED, Gio.FileMonitorEvent.DELETED):
            _setup_language_monitor()
        new_lang = setup_i18n(None)
        _notify_language(new_lang)


def _setup_language_monitor():
    """Vigila ``~/.config/omarchy/language`` (y su directorio). Re-armable."""
    global _language_monitor
    if Gio is None:
        return
    path = omarchy_language_file()
    directory = os.path.dirname(path)
    if not os.path.isdir(directory):
        return  # sin Omarchy instalado: nada que vigilar
    try:
        if _language_monitor is not None:
            _language_monitor.cancel()
    except Exception:
        pass
    try:
        monitor = Gio.File.new_for_path(directory).monitor_directory(
            Gio.FileMonitorFlags.NONE, None
        )
        monitor.connect("changed", _on_language_file_changed)
        _language_monitor = monitor
    except Exception:
        _language_monitor = None


def watch_language():
    """Arranca la vigilancia del idioma de Omarchy. Idempotente."""
    if _language_monitor is None:
        _setup_language_monitor()
