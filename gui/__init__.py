"""Paquete GUI: versión, logging y carga de módulos hermanos.

Contrato de bootstrap (instalado vs checkout):

1. Los entrypoints (`main.py`, lanzador) aseguran que ``import gui``
   funcione vía :func:`ensure_package`.
2. El resto de módulos obtiene ``load`` / ``log_exc`` / ``gettext_func``
   desde este paquete (o :func:`api` tras reintentar el path).
3. No reimplementar fallbacks locales de import/gettext/logging.
"""

from __future__ import annotations

import importlib
import importlib.util
import os
import sys

__version__ = "2.5.0"

# Si falla la importación del paquete, no inventar una versión antigua.
UNKNOWN_VERSION = "desconocida"


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


def ensure_package(script_file=None):
    """Garantiza que ``import gui`` funcione aunque un script corra suelto.

    El lanzador instalado ejecuta ``python3 <dir>/main.py`` sin el paquete
    alrededor; en ese caso se carga ``__init__.py`` como módulo ``gui``.
    """
    try:
        import gui  # noqa: F401

        return True
    except ImportError:
        pass

    here = os.path.dirname(os.path.abspath(script_file or __file__))
    init = os.path.join(here, "__init__.py")
    if not os.path.isfile(init):
        return False
    try:
        spec = importlib.util.spec_from_file_location(
            "gui", init, submodule_search_locations=[here]
        )
        if spec is None or spec.loader is None:
            return False
        module = importlib.util.module_from_spec(spec)
        sys.modules["gui"] = module
        spec.loader.exec_module(module)
        return True
    except Exception:
        print("keybackcon-gui: no se pudo cargar el paquete gui", file=sys.stderr)
        return False


def api(script_file=None):
    """Devuelve ``(load, log_exc, gettext_func)`` tras asegurar el paquete.

    Si ``import gui`` falla, inserta el directorio padre del paquete en
    ``sys.path`` y reintenta. Evita reimplementar helpers en cada módulo.
    """
    if ensure_package(script_file):
        import gui as pkg

        return pkg.load, pkg.log_exc, pkg.gettext_func

    here = os.path.dirname(os.path.abspath(script_file or __file__))
    parent = os.path.dirname(here)
    if parent not in sys.path:
        sys.path.insert(0, parent)
    ensure_package(script_file or os.path.join(here, "__init__.py"))
    import gui as pkg

    return pkg.load, pkg.log_exc, pkg.gettext_func


def app_version():
    """Versión publicada del paquete GUI (espejo de Cargo.toml)."""
    return __version__
