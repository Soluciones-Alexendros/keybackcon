"""Comprobación de versiones mínimas de GTK/libadwaita antes de la ventana."""
from __future__ import annotations


MIN_ADW_MINOR = 5  # libadwaita 1.5 (ToolbarView, PreferencesDialog, AlertDialog)
MIN_GTK_MAJOR = 4


def adw_version_tuple():
    """Devuelve (major, minor, micro) de Adw o None si no está disponible."""
    try:
        import gi

        gi.require_version("Adw", "1")
        from gi.repository import Adw

        major = int(Adw.get_major_version())
        minor = int(Adw.get_minor_version())
        micro = int(Adw.get_micro_version())
        return major, minor, micro
    except Exception:
        return None


def gtk_version_tuple():
    try:
        import gi

        gi.require_version("Gtk", "4.0")
        from gi.repository import Gtk

        return (
            int(Gtk.get_major_version()),
            int(Gtk.get_minor_version()),
            int(Gtk.get_micro_version()),
        )
    except Exception:
        return None


def check_gui_requirements() -> tuple[bool, str]:
    """(ok, mensaje). Si ok es False, no se debe construir la mesa de luz."""
    gtk = gtk_version_tuple()
    if gtk is None:
        return False, "GTK 4 no está disponible (python3-gi / gir1.2-gtk-4.0)."
    if gtk[0] < MIN_GTK_MAJOR:
        return False, f"Se necesita GTK {MIN_GTK_MAJOR}.x; encontrado {gtk[0]}.{gtk[1]}."
    adw = adw_version_tuple()
    if adw is None:
        return False, "libadwaita no está disponible (gir1.2-adw-1)."
    if adw[0] < 1 or (adw[0] == 1 and adw[1] < MIN_ADW_MINOR):
        return (
            False,
            f"Se necesita libadwaita ≥ 1.{MIN_ADW_MINOR} (API ToolbarView/"
            f"PreferencesDialog/AlertDialog). Encontrado {adw[0]}.{adw[1]}.{adw[2]}. "
            "En Debian 12 la mesa de luz no está soportada; usa el CLI `keybackcon`.",
        )
    return True, f"GTK {gtk[0]}.{gtk[1]} · Adw {adw[0]}.{adw[1]}.{adw[2]}"
