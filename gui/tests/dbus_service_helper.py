"""Proceso auxiliar de los tests del control D-Bus de la ventana.

Dos modos, siempre en un proceso aparte (como en producción: la bandeja y la
ventana no comparten proceso):

- ``--servicio <fichero>``: posee ``DBUS_NAME``, registra la interfaz de
  control y anota en el fichero cada método que recibe. Imprime ``listo``
  cuando ya atiende y sirve hasta que el padre lo termina (o 30 s).
- ``--cliente <metodo>…``: llama a ``_llamar_control`` con cada método e
  imprime ``<metodo> <True|False>``. Se hace en un proceso nuevo para no
  arrastrar la conexión cacheada por ``Gio.bus_get_sync``.
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

from gi.repository import Gio, GLib  # noqa: E402

from gui import main as gui_main  # noqa: E402

TIMEOUT_MS = 30_000
LLAMADA_MS = 5_000


def _poseer_nombre(conn):
    reply = conn.call_sync(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "RequestName",
        GLib.Variant("(su)", (gui_main.DBUS_NAME, 0)),
        None,
        Gio.DBusCallFlags.NONE,
        LLAMADA_MS,
        None,
    )
    return reply.unpack()[0] == 1  # DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER


def _modo_servicio(registro):
    conn = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    if not _poseer_nombre(conn):
        print("no se pudo poseer el nombre", file=sys.stderr)
        return 1

    def registrar(nombre):
        with open(registro, "a", encoding="utf-8") as fh:
            fh.write(nombre + "\n")
        return False

    acciones = {m: (lambda n=m: registrar(n)) for m in gui_main.CONTROL_METHODS}
    if not gui_main.register_control(conn, acciones):
        print("no se pudo registrar el control", file=sys.stderr)
        return 1

    loop = GLib.MainLoop()
    GLib.timeout_add(TIMEOUT_MS, lambda: (loop.quit(), False)[1])
    print("listo", flush=True)
    loop.run()
    return 0


def _modo_cliente(metodos):
    for metodo in metodos:
        print(f"{metodo} {gui_main._llamar_control(metodo)}", flush=True)
    return 0


def main(argv):
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    if argv[0] == "--servicio" and len(argv) == 2:
        return _modo_servicio(argv[1])
    if argv[0] == "--cliente" and len(argv) > 1:
        return _modo_cliente(argv[1:])
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
