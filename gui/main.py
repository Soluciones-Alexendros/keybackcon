#!/usr/bin/env python3
import os
import sys

APP_ID = "org.iniciativas.keybackcon"

DBUS_NAME = "org.iniciativas.keybackcon"
DBUS_PATH = "/org/iniciativas/keybackcon"
DBUS_CONTROL_IFACE = "org.iniciativas.keybackcon.Control"
CONTROL_XML = """
<node>
  <interface name="org.iniciativas.keybackcon.Control">
    <method name="Toggle"/>
    <method name="Present"/>
    <method name="Quit"/>
  </interface>
</node>
"""

USAGE = """Uso: main.py [--tray] [--tray-child] [--preferences] [-h | --help]

Keyboard Backlight Controls — ventana y bandeja del sistema.

Opciones:
  --tray          Arranca en la bandeja del sistema, sin ventana visible.
  --tray-child    Ventana hija de la bandeja: el botón X la oculta en vez
                  de cerrarla y expone el control D-Bus de la ventana.
  --preferences   Abre la ventana mostrando Preferencias (sin --tray).
  -h, --help      Muestra esta ayuda y sale con código 0."""


def _fix_path():
    here = os.path.dirname(os.path.abspath(__file__))
    root = os.path.dirname(here)
    for path in (root, here):
        if path not in sys.path:
            sys.path.insert(0, path)


def _ensure_gui_package():
    """Garantiza que `import gui` funcione aunque main.py corra suelto.

    El lanzador instalado ejecuta `python3 <dir>/main.py`, sin paquete
    `gui/` alrededor; en ese caso cargamos __init__.py como módulo `gui`.
    """
    try:
        import gui  # noqa: F401

        return
    except ImportError:
        pass
    here = os.path.dirname(os.path.abspath(__file__))
    init = os.path.join(here, "__init__.py")
    if not os.path.isfile(init):
        return
    try:
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "gui", init, submodule_search_locations=[here]
        )
        if spec is None or spec.loader is None:
            return
        module = importlib.util.module_from_spec(spec)
        sys.modules["gui"] = module
        spec.loader.exec_module(module)
    except Exception:
        print("keybackcon-gui: no se pudo cargar el paquete gui", file=sys.stderr)


_fix_path()
_ensure_gui_package()

try:
    from gui import load, log_exc, gettext_func
except ImportError:  # último recurso: ejecutar gui/ como directorio suelto
    def load(nombre):
        import importlib

        return importlib.import_module(nombre)

    def gettext_func():
        return load("i18n")._

    def log_exc(contexto):
        print(f"keybackcon-gui: {contexto}: error de importación", file=sys.stderr)


def print_usage(out=None):
    stream = sys.stdout if out is None else out
    stream.write(USAGE + "\n")


def _app_version():
    try:
        from gui import __version__ as version

        return version
    except Exception:
        return "2.2.1"


def _setup_i18n():
    return load("i18n").setup_i18n()


def _load_client():
    return load("client").KeybackconClient()


def _load_tray():
    tray = load("tray")
    return tray.TrayIndicator, tray.TrayUnavailable


def _load_window():
    return load("control_panel").MesaWindow


def _open_preferences(parent=None, on_effects_changed=None):
    try:
        SettingsDialog = load("settings").SettingsDialog
    except Exception:
        log_exc("cargar preferencias")
        return False
    try:
        dialog = SettingsDialog(on_effects_changed=on_effects_changed)
        dialog.present(parent)
        return True
    except Exception:
        log_exc("abrir preferencias")
        return False


def _stop_client(client):
    try:
        client.stop()
    except Exception:
        log_exc("parar cliente")


def _aviso_falta(que, exc, rol):
    """Mensaje claro (sin traceback) con el comando de instalación de la distro."""
    print(f"keybackcon-gui: hace falta {que}.", file=sys.stderr)
    try:
        sugerencia = load("compat").sugerencia_instalacion(rol)
        print(f"  instala: {sugerencia}", file=sys.stderr)
    except Exception:
        log_exc("sugerencia de paquetes")
    print(f"  ({exc})", file=sys.stderr)


def _llamar_control(metodo):
    """Llama a la ventana viva por D-Bus. Devuelve True si respondió;
    False si no hay ventana (o no respondió), en cuyo caso toca lanzarla."""
    try:
        from gi.repository import Gio

        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.call_sync(
            DBUS_NAME,
            DBUS_PATH,
            DBUS_CONTROL_IFACE,
            metodo,
            None,
            None,
            Gio.DBusCallFlags.NONE,
            1500,
            None,
        )
        return True
    except Exception:
        return False


def _run_window(open_preferences=False, hide_on_close=False):
    try:
        import gi

        gi.require_version("Gtk", "4.0")
        gi.require_version("Adw", "1")
        from gi.repository import Adw, GLib, Gio
    except Exception as exc:
        _aviso_falta("GTK4/libadwaita (python3-gi)", exc, "gui")
        return 1

    try:
        from gui.preflight import check_gui_requirements
    except ImportError:
        try:
            from preflight import check_gui_requirements
        except ImportError:
            check_gui_requirements = None
    if check_gui_requirements is not None:
        ok, msg = check_gui_requirements()
        if not ok:
            print(f"keybackcon-gui: {msg}", file=sys.stderr)
            try:
                sugerencia = load("compat").sugerencia_instalacion("gui")
                print(f"  instala: {sugerencia}", file=sys.stderr)
            except Exception:
                log_exc("sugerencia de paquetes")
            return 1

    MesaWindow = _load_window()
    app = Adw.Application(application_id=APP_ID)
    holder = {}

    def _toggle_window():
        win = holder.get("win")
        if win is None:
            return False
        try:
            if win.is_visible():
                win.set_visible(False)
            else:
                win.present()
        except Exception:
            log_exc("alternar ventana")
        return False

    def _present_window():
        win = holder.get("win")
        if win is not None:
            try:
                win.present()
            except Exception:
                log_exc("presentar ventana")
        return False

    def _register_control():
        if not hide_on_close:
            # Solo la ventana hija de la bandeja acepta control remoto; la
            # instancia del menú de apps es autónoma.
            return
        try:
            conn = app.get_dbus_connection()
            if conn is None:
                return
            info = Gio.DBusNodeInfo.new_for_xml(CONTROL_XML)
            iface = info.interfaces[0]

            def method_call(conn, _sender, _path, _iface, method, _params,
                            invocation):
                if method == "Toggle":
                    GLib.idle_add(_toggle_window)
                elif method == "Present":
                    GLib.idle_add(_present_window)
                elif method == "Quit":
                    GLib.idle_add(lambda: (app.quit(), False)[1])
                invocation.return_value(None)

            import warnings

            with warnings.catch_warnings():
                warnings.filterwarnings(
                    "ignore", message=".*register_object is deprecated.*"
                )
                conn.register_object(DBUS_PATH, iface, method_call)
        except Exception:
            log_exc("registrar control D-Bus")

    def on_activate(app):
        def on_preferences():
            win = holder.get("win")
            if win is None:
                _open_preferences(None, None)
            else:
                win.open_preferences()

        win = MesaWindow(app, on_preferences=on_preferences,
                         hide_on_close=hide_on_close)
        holder["win"] = win
        _register_control()
        win.present()
        if open_preferences:
            GLib.idle_add(win.open_preferences)

    app.connect("activate", on_activate)
    return app.run(None)


def _run_tray():
    try:
        TrayIndicator, _TrayUnavailable = _load_tray()
    except Exception as exc:
        print("Aviso: bandeja no disponible (%s); abriendo ventana." % exc,
              file=sys.stderr)
        try:
            print("  instala: %s" % load("compat").sugerencia_instalacion("bandeja"),
                  file=sys.stderr)
        except Exception:
            log_exc("sugerencia de bandeja")
        return _run_window()
    client = _load_client()
    return _run_tray_only(client, TrayIndicator)


def _run_tray_only(client, TrayIndicator):
    import atexit
    import subprocess

    try:
        import gi

        gi.require_version("Gtk", "3.0")
        from gi.repository import Gtk, GLib
    except Exception as exc:
        _aviso_falta("GTK3 para la bandeja", exc, "bandeja")
        print("keybackcon-gui: abriendo la ventana en su lugar.",
              file=sys.stderr)
        return _run_window()

    here = os.path.dirname(os.path.abspath(__file__))
    ventanas = []

    def reap_ventanas():
        ventanas[:] = [p for p in ventanas if p.poll() is None]

    def abrir_ventana(*args, preferencias=False):
        # 1) ¿Ya hay una ventana viva? Se la pide por D-Bus: instantáneo y
        #    sin procesos fantasma (instancia única de Gtk.Application).
        if _llamar_control("Present" if preferencias else "Toggle"):
            return
        # 2) No hay ventana: lanzamos la hija persistente (--tray-child), que
        #    se oculta en vez de cerrarse y atiende Toggle/Present/Quit.
        reap_ventanas()
        if ventanas:
            return
        argv = [sys.executable, os.path.join(here, "main.py"), "--tray-child"]
        if preferencias:
            argv.append("--preferences")
        try:
            ventanas.append(subprocess.Popen(argv))
        except Exception:
            log_exc("lanzar ventana")

    def on_toggle_window(*args):
        abrir_ventana()

    def on_preferences(*args):
        abrir_ventana(preferencias=True)

    def matar_ventanas():
        for p in list(ventanas):
            if p.poll() is None:
                try:
                    p.terminate()
                except Exception:
                    log_exc("terminar ventana hija")
        for p in list(ventanas):
            try:
                p.wait(timeout=2)
            except Exception:
                try:
                    p.kill()
                except Exception:
                    log_exc("matar ventana hija")

    atexit.register(matar_ventanas)

    def on_about(*args):
        try:
            dialog = Gtk.MessageDialog(
                message_type=Gtk.MessageType.INFO,
                buttons=Gtk.ButtonsType.CLOSE,
                text="keybackcon %s" % _app_version(),
            )
            dialog.format_secondary_text("Keyboard Backlight Controls")
            dialog.run()
            dialog.destroy()
        except Exception:
            log_exc("diálogo acerca de")

    def _gtk_main_quit():
        try:
            if Gtk.main_level() > 0:
                Gtk.main_quit()
        except Exception:
            log_exc("salir del bucle GTK")

    tray = TrayIndicator(client, on_toggle_window, on_preferences, on_about,
                         on_quit=_gtk_main_quit)

    def on_signal(*args):
        _stop_client(client)
        try:
            if Gtk.main_level() > 0:
                Gtk.main_quit()
        except Exception:
            log_exc("señal de salida")
        return False

    try:
        import signal

        try:
            from gi.repository import GLibUnix
        except Exception:
            GLibUnix = None
        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                if GLibUnix is not None:
                    GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, sig, on_signal)
                else:
                    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, sig, on_signal)
            except Exception:
                log_exc("señal %s" % sig)
    except Exception:
        log_exc("manejadores de señal")
    try:
        Gtk.main()
    except KeyboardInterrupt:
        pass  # Ctrl+C: la limpieza de abajo (finally) ya se ejecuta
    finally:
        matar_ventanas()
        _stop_client(client)
    return 0


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    if "-h" in args or "--help" in args:
        print_usage()
        return 0
    _setup_i18n()
    if "--tray" in args:
        return _run_tray()
    return _run_window(
        open_preferences="--preferences" in args,
        hide_on_close="--tray-child" in args,
    )


if __name__ == "__main__":
    sys.exit(main())
