import gettext
import locale
import os

APP_NAME = "keybackcon"
SUPPORTED_LOCALES = {"es": "Español", "en": "English"}

_current_lang = "es"
_translation = gettext.NullTranslations()
_ = _translation.gettext


def get_locale_dir() -> str:
    here = os.path.join(os.path.dirname(os.path.abspath(__file__)), "locale")
    home = os.path.expanduser("~/.local/share/locale")
    system = "/usr/share/locale"
    for path in (here, home, system):
        if os.path.isdir(path):
            return path
    return here


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


def setup_i18n(lang: str | None = None) -> str:
    if lang is None:
        try:
            default = locale.getdefaultlocale()[0]
        except Exception:
            default = None
        lang = default if default else "es"
    return set_language(lang)
