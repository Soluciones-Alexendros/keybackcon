"""Bootstrap único: versión y API compartida (KBC-37 / KBC-38)."""

import os
import re
import unittest

from gui import UNKNOWN_VERSION, __version__, api, app_version, ensure_package

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _cargo_version():
    """Versión de Cargo.toml: la GUI la espeja, no la inventa."""
    with open(os.path.join(ROOT, "Cargo.toml"), encoding="utf-8") as f:
        for line in f:
            match = re.match(r'^version\s*=\s*"([^"]+)"', line)
            if match:
                return match.group(1)
    raise AssertionError("Cargo.toml sin versión")


class BootstrapTests(unittest.TestCase):
    def test_version_espeja_paquete(self):
        esperada = _cargo_version()
        self.assertEqual(__version__, esperada)
        self.assertEqual(app_version(), esperada)
        self.assertEqual(UNKNOWN_VERSION, "desconocida")
        self.assertNotEqual(UNKNOWN_VERSION, "2.2.1")
        self.assertNotEqual(esperada, "2.2.1")

    def test_api_devuelve_helpers(self):
        self.assertTrue(ensure_package())
        load, log_exc, gettext_func = api()
        self.assertTrue(callable(load))
        self.assertTrue(callable(log_exc))
        self.assertTrue(callable(gettext_func))
        self.assertEqual(gettext_func()("x"), "x")


if __name__ == "__main__":
    unittest.main()
