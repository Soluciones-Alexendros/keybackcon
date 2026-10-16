"""Abrir/construir la ventana no debe escribir en el HID (H-02)."""
import os
import sys
import unittest
from unittest import mock

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)


class FakeClient:
    def __init__(self):
        self.calls = []

    def get_state_file(self):
        return "ff7800", 60

    def info(self):
        return {"device": "/dev/hidraw0", "lamps": 1}

    def stop_animation(self):
        self.calls.append("stop_animation")

    def set_color(self, hexv):
        self.calls.append(("set", hexv))

    def set_brightness(self, pct):
        self.calls.append(("brightness", pct))

    def animation(self, mode):
        self.calls.append(("animation", mode))

    def stop(self):
        self.calls.append("stop")
        return "ok"

    def animation_running(self):
        return None


@unittest.skipUnless(
    os.environ.get("KEYBACKCON_GUI_TESTS") == "1",
    "requiere GTK/Adw en display (KEYBACKCON_GUI_TESTS=1)",
)
class TestWindowInitSinEscrituras(unittest.TestCase):
    def test_construir_no_llama_al_cli_de_escritura(self):
        import gi

        gi.require_version("Gtk", "4.0")
        gi.require_version("Adw", "1")
        from gi.repository import Adw

        from gui.preflight import check_gui_requirements

        ok, _msg = check_gui_requirements()
        if not ok:
            self.skipTest("libadwaita insuficiente para la ventana")

        from gui.window import MesaWindow

        app = Adw.Application(application_id="org.iniciativas.keybackcon.test")
        client = FakeClient()

        def _activate(a):
            win = MesaWindow(a, client=client)
            self.assertEqual(
                [c for c in client.calls if c != "stop_animation" or True],
                [],
                f"escrituras inesperadas al construir: {client.calls}",
            )
            self.assertEqual(client.calls, [])
            win.destroy()
            a.quit()

        app.connect("activate", _activate)
        app.run([])


class TestPreflight(unittest.TestCase):
    def test_check_devuelve_tupla(self):
        from gui.preflight import check_gui_requirements

        ok, msg = check_gui_requirements()
        self.assertIsInstance(ok, bool)
        self.assertIsInstance(msg, str)
        self.assertTrue(msg)


class TestBackendQueue(unittest.TestCase):
    def test_consolida_brillo(self):
        import time
        from gui.backend import BackendQueue, Intent

        class C:
            def __init__(self):
                self.vals = []

            def set_brightness(self, v):
                self.vals.append(v)

        c = C()
        q = BackendQueue(c)
        done = []
        q.submit(Intent("brightness", 10, lambda ok, e: done.append(1)))
        q.submit(Intent("brightness", 20, lambda ok, e: done.append(2)))
        q.submit(Intent("brightness", 30, lambda ok, e: done.append(3)))
        time.sleep(0.3)
        q.close()
        time.sleep(0.1)
        self.assertEqual(c.vals[-1:], [30])


if __name__ == "__main__":
    unittest.main()
