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


# El job quality exige la suite gráfica con KEYBACKCON_REQUIRE_GUI_SUITE=1.
# Smoke y otros entornos con CI=true no la fuerzan (sin GTK/xvfb).
_GUI_SUITE = os.environ.get("KEYBACKCON_GUI_TESTS") == "1"
if os.environ.get("KEYBACKCON_REQUIRE_GUI_SUITE") == "1" and not _GUI_SUITE:
    raise RuntimeError(
        "suite gráfica obligatoria: define KEYBACKCON_GUI_TESTS=1 "
        "(junto a KEYBACKCON_REQUIRE_GUI_SUITE=1)"
    )


@unittest.skipUnless(
    _GUI_SUITE,
    "requiere GTK/Adw en display (KEYBACKCON_GUI_TESTS=1)",
)
class TestWindowInitSinEscrituras(unittest.TestCase):
    def test_construir_no_llama_al_cli_de_escritura(self):
        import gi

        gi.require_version("Gtk", "4.0")
        gi.require_version("Adw", "1")
        from gi.repository import Adw

        from gui.preflight import check_gui_requirements

        ok, msg = check_gui_requirements()
        if not ok:
            self.fail(f"KEYBACKCON_GUI_TESTS=1 pero GUI no usable: {msg}")

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


# Consolidación BackendQueue: gui.tests.test_backend_async (KBC-32, sin sleeps).


if __name__ == "__main__":
    unittest.main()
