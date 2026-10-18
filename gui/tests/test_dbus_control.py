"""Control D-Bus de la ventana: contrato, despacho y llamada desde la bandeja.

Sin GTK ni display. El bus es un `dbus-daemon` de sesión privado, así que los
tests no tocan el bus del usuario. El despacho se prueba in-process con una
conexión dedicada, y ``_llamar_control`` desde un proceso cliente aparte: es
el caso real (la bandeja habla con la ventana) y evita el caché de
``Gio.bus_get_sync``, que fija la dirección en la primera llamada del proceso.
"""

import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from xml.dom import minidom

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

from gui import main as gui_main  # noqa: E402

HELPER = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                      "dbus_service_helper.py")

try:
    import gi  # noqa: F401

    from gi.repository import Gio, GLib

    FLAGS_BUS = (Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT
                 | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION)
    GI_DISPONIBLE = True
except Exception:  # sin python3-gi: los tests de bus se saltan
    GI_DISPONIBLE = False

DAEMON = shutil.which("dbus-daemon")
BUS_DISPONIBLE = GI_DISPONIBLE and DAEMON is not None

# En CI se exige que estos tests corran de verdad: sin las dependencias, el
# job falla en vez de saltarse la cobertura del control D-Bus.
REQUERIDO = os.environ.get("KEYBACKCON_REQUIRE_DBUS") == "1"
if REQUERIDO and not BUS_DISPONIBLE:
    raise RuntimeError(
        "KEYBACKCON_REQUIRE_DBUS=1 pero falta python3-gi o dbus-daemon"
    )

_BUS = None
_DIRECCION = None
_ENV_PREVIO = None


def setUpModule():
    """Arranca un bus de sesión privado, hijo de este proceso.

    Se usa `dbus-daemon` en vez de `Gio.TestDBus` porque el finalizador de
    este último espera ~30 s cuando el proceso abrió una conexión con
    ``Gio.bus_get_sync`` y nadie itera el bucle de GLib.
    """
    global _BUS, _DIRECCION, _ENV_PREVIO
    if not BUS_DISPONIBLE:
        return
    _BUS = subprocess.Popen(
        [DAEMON, "--session", "--print-address", "--nofork"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
    )
    direccion = _BUS.stdout.readline().strip()
    if not direccion:
        _BUS.kill()
        _BUS.wait(timeout=5)
        _BUS = None
        raise RuntimeError("no se pudo arrancar el bus de sesión de prueba")
    _DIRECCION = direccion
    _ENV_PREVIO = os.environ.get("DBUS_SESSION_BUS_ADDRESS")
    os.environ["DBUS_SESSION_BUS_ADDRESS"] = direccion


def tearDownModule():
    global _BUS, _DIRECCION, _ENV_PREVIO
    if _BUS is not None:
        _BUS.terminate()
        try:
            _BUS.wait(timeout=5)
        except subprocess.TimeoutExpired:
            _BUS.kill()
            _BUS.wait(timeout=5)
        for flujo in (_BUS.stdout, _BUS.stderr):
            try:
                flujo.close()
            except Exception:
                pass
        _BUS = None
    _DIRECCION = None
    if _ENV_PREVIO is None:
        os.environ.pop("DBUS_SESSION_BUS_ADDRESS", None)
    else:
        os.environ["DBUS_SESSION_BUS_ADDRESS"] = _ENV_PREVIO
    _ENV_PREVIO = None


def _conexion_dedicada():
    """Conexión al bus privado del test.

    No usa ``Gio.bus_get_sync``: esa devuelve la conexión cacheada del proceso,
    que puede apuntar al bus del usuario si algo la tocó antes.
    """
    return Gio.DBusConnection.new_for_address_sync(_DIRECCION, FLAGS_BUS,
                                                   None, None)


def _esperar_lineas(path, cuantas, timeout=3.0):
    limite = time.monotonic() + timeout
    lineas = []
    while time.monotonic() < limite:
        lineas = _leer_lineas(path)
        if len(lineas) >= cuantas:
            break
        time.sleep(0.02)
    return lineas


def _leer_lineas(path):
    if not os.path.isfile(path):
        return []
    with open(path, encoding="utf-8") as fh:
        return [ln for ln in fh.read().splitlines() if ln]


class TestContratoControl(unittest.TestCase):
    """El XML del bus, las constantes y el despacho puro (sin GI)."""

    def test_xml_declara_la_interfaz_y_los_metodos(self):
        doc = minidom.parseString(gui_main.CONTROL_XML)
        ifaces = doc.getElementsByTagName("interface")
        self.assertEqual(len(ifaces), 1)
        self.assertEqual(ifaces[0].getAttribute("name"),
                         gui_main.DBUS_CONTROL_IFACE)
        metodos = tuple(n.getAttribute("name")
                        for n in doc.getElementsByTagName("method"))
        self.assertEqual(metodos, gui_main.CONTROL_METHODS)

    def test_camino_del_nombre_coincide_con_la_ruta(self):
        self.assertTrue(gui_main.DBUS_PATH.startswith("/"))
        esperado = gui_main.DBUS_PATH.replace("/", ".").lstrip(".")
        self.assertEqual(gui_main.DBUS_NAME, esperado)

    def test_control_action_mapea_y_descarta(self):
        acciones = {m: (lambda n=m: n) for m in gui_main.CONTROL_METHODS}
        self.assertIs(gui_main.control_action("Toggle", acciones),
                      acciones["Toggle"])
        self.assertIsNone(gui_main.control_action("Apagar", acciones))
        self.assertIsNone(gui_main.control_action("", acciones))

    def test_sin_conexion_no_registra(self):
        self.assertFalse(gui_main.register_control(None, {}))


@unittest.skipUnless(BUS_DISPONIBLE, "requiere Gio y dbus-daemon")
class TestDespachoControl(unittest.TestCase):
    """Toggle/Present/Quit llegan a su acción a través del bus."""

    @classmethod
    def setUpClass(cls):
        cls.conn = _conexion_dedicada()
        cls.llamadas = []
        # Se llama por el nombre único de la conexión: no hace falta poseer el
        # nombre del servicio, así no interfiere con el cliente en subproceso.
        cls.destino = cls.conn.get_unique_name()
        acciones = {m: (lambda n=m: cls.llamadas.append(n))
                    for m in gui_main.CONTROL_METHODS}
        cls.registrado = gui_main.register_control(cls.conn, acciones)

    @classmethod
    def tearDownClass(cls):
        cls.conn.close_sync(None)

    def setUp(self):
        if not self.registrado:
            self.fail("no se pudo registrar la interfaz de control")
        self.llamadas.clear()

    def _llamar(self, metodo, timeout_ms=1500):
        """Llama de forma asíncrona e itera el bucle hasta la respuesta."""
        loop = GLib.MainLoop()
        resultado = {"ok": False}
        estado = {}

        def done(src, res, _data=None):
            try:
                src.call_finish(res)
                resultado["ok"] = True
            except Exception:
                resultado["ok"] = False
            if estado.get("id"):
                GLib.source_remove(estado["id"])
                estado["id"] = None
            loop.quit()

        self.conn.call(self.destino, gui_main.DBUS_PATH,
                       gui_main.DBUS_CONTROL_IFACE, metodo, None, None,
                       Gio.DBusCallFlags.NONE, timeout_ms, None, done, None)
        estado["id"] = GLib.timeout_add(timeout_ms + 500,
                                        lambda: (loop.quit(), False)[1])
        loop.run()
        return resultado["ok"]

    def test_cada_metodo_delega_en_su_accion(self):
        for metodo in gui_main.CONTROL_METHODS:
            self.assertTrue(self._llamar(metodo), f"{metodo} sin respuesta")
        self.assertEqual(self.llamadas, list(gui_main.CONTROL_METHODS))

    def test_metodo_fuera_del_contrato_se_rechaza(self):
        self.assertFalse(self._llamar("Apagar"))
        self.assertEqual(self.llamadas, [])


@unittest.skipUnless(BUS_DISPONIBLE, "requiere Gio y dbus-daemon")
class TestLlamarControl(unittest.TestCase):
    """`_llamar_control` habla con la ventana de otro proceso."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.registro = os.path.join(self.tmp.name, "recibidos")

    def _servicio(self):
        proc = subprocess.Popen(
            [sys.executable, HELPER, "--servicio", self.registro],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.addCleanup(self._matar, proc)
        listo = proc.stdout.readline().strip()
        if listo != "listo":
            self.fail(f"el servicio no arrancó: {listo!r} "
                      f"{proc.stderr.read()!r}")
        return proc

    def _cliente(self, *metodos):
        """Llama a `_llamar_control` desde otro proceso y devuelve el mapa."""
        proc = subprocess.run(
            [sys.executable, HELPER, "--cliente", *metodos],
            capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        resultado = {}
        for linea in proc.stdout.splitlines():
            if linea.strip():
                metodo, valor = linea.split()
                resultado[metodo] = valor == "True"
        return resultado

    def _matar(self, proc):
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)
        for flujo in (proc.stdout, proc.stderr):
            try:
                flujo.close()
            except Exception:
                pass

    def test_sin_ventana_devuelve_false(self):
        self.assertEqual(self._cliente("Toggle"), {"Toggle": False})

    def test_con_ventana_devuelve_true_y_delega(self):
        self._servicio()
        self.assertEqual(self._cliente("Toggle", "Present"),
                         {"Toggle": True, "Present": True})
        self.assertEqual(_esperar_lineas(self.registro, 2),
                         ["Toggle", "Present"])
        self.assertEqual(self._cliente("Apagar"), {"Apagar": False})

    def test_ventana_cerrada_vuelve_a_false(self):
        proc = self._servicio()
        self.assertEqual(self._cliente("Toggle"), {"Toggle": True})
        self._matar(proc)
        self.assertEqual(self._cliente("Toggle"), {"Toggle": False})


if __name__ == "__main__":
    unittest.main()
