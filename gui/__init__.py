import importlib
import sys

__version__ = "2.4.1"


def log_exc(contexto):
    """Imprime la excepción activa en stderr con contexto.

    Sustituye los `except Exception: pass` silenciosos: nada de tragar
    errores sin dejar rastro.
    """
    tipo, exc, _tb = sys.exc_info()
    if tipo is None:
        return
    print(f"keybackcon-gui: {contexto}: {tipo.__name__}: {exc}", file=sys.stderr)


def load(nombre):
    """Importa un módulo hermano en cualquiera de los modos soportados.

    Orden: paquete instalado (``gui.<nombre>``), ejecución suelta con
    ``gui/`` en ``sys.path`` (``<nombre>``) e, en último caso, import
    relativo. Re-lanza el último ImportError si nada funciona.
    """
    ultimo = None
    for candidato in (f"gui.{nombre}", nombre):
        try:
            return importlib.import_module(candidato)
        except ImportError as exc:
            ultimo = exc
    if __package__:
        try:
            return importlib.import_module(f".{nombre}", __package__)
        except ImportError as exc:
            ultimo = exc
    raise ultimo


def gettext_func():
    """Devuelve la función ``_()`` de i18n (identidad si no hay catálogo)."""
    try:
        return load("i18n")._
    except Exception:
        log_exc("i18n")
        return lambda s: s
