"""Bootstrap único: versión y API compartida (KBC-37 / KBC-38)."""

import unittest

from gui import UNKNOWN_VERSION, __version__, api, app_version, ensure_package


class BootstrapTests(unittest.TestCase):
    def test_version_espeja_paquete(self):
        self.assertEqual(__version__, "2.5.0")
        self.assertEqual(app_version(), "2.5.0")
        self.assertEqual(UNKNOWN_VERSION, "desconocida")
        self.assertNotEqual(UNKNOWN_VERSION, "2.2.1")

    def test_api_devuelve_helpers(self):
        self.assertTrue(ensure_package())
        load, log_exc, gettext_func = api()
        self.assertTrue(callable(load))
        self.assertTrue(callable(log_exc))
        self.assertTrue(callable(gettext_func))
        self.assertEqual(gettext_func()("x"), "x")


if __name__ == "__main__":
    unittest.main()
