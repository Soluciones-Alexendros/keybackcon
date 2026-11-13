import os
import shutil
import tempfile
import unittest
from unittest import mock

from gui import theme


class EntornoConfig:
    def __init__(self):
        self._prev = os.environ.get("XDG_CONFIG_HOME")
        self.tmp = tempfile.mkdtemp(prefix="kbc-test-config-")
        os.environ["XDG_CONFIG_HOME"] = self.tmp

    def restaurar(self):
        if self._prev is None:
            os.environ.pop("XDG_CONFIG_HOME", None)
        else:
            os.environ["XDG_CONFIG_HOME"] = self._prev
        shutil.rmtree(self.tmp, ignore_errors=True)


class TestTema(unittest.TestCase):
    def setUp(self):
        self.env = EntornoConfig()
        theme._schema_cache.clear()
        # Los tests cubren la vía de archivo; GSettings depende de si el
        # schema está instalado en la máquina, así que se aísla.
        self._sin_gsettings = mock.patch.object(
            theme, "lookup_settings", return_value=None
        )
        self._sin_gsettings.start()

    def tearDown(self):
        self._sin_gsettings.stop()
        theme._schema_cache.clear()
        self.env.restaurar()

    def test_default_system_sin_nada(self):
        self.assertEqual(theme.get_theme(), "system")

    def test_roundtrip_archivo(self):
        self.assertEqual(theme.set_theme("dark"), "dark")
        self.assertEqual(theme.get_theme(), "dark")
        theme.set_theme("LIGHT")  # mayúsculas: se normaliza
        self.assertEqual(theme.get_theme(), "light")

    def test_valor_invalido_ca_a_system(self):
        self.assertEqual(theme.set_theme("neon"), "system")
        self.assertEqual(theme.get_theme(), "system")

    def test_valor_invalido_en_archivo_ignorado(self):
        path = os.path.join(self.env.tmp, "ilutex", "theme")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("chicle\n")
        self.assertEqual(theme.get_theme(), "system")

    def test_lookup_settings_con_schema_inexistente_devuelve_none(self):
        # Un schema que no existe en ningún sistema siempre da None.
        self._sin_gsettings.stop()
        try:
            theme._schema_cache.clear()
            self.assertIsNone(
                theme.lookup_settings("org.alexendros.ilutex.inexistente")
            )
        finally:
            theme._schema_cache.clear()
            self._sin_gsettings.start()


class TestAcentoOmarchy(unittest.TestCase):
    """Lectura del acento de Omarchy desde colors.toml."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="kbc-test-accent-")
        self.path = os.path.join(self.tmp, "colors.toml")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _escribir(self, contenido):
        with open(self.path, "w", encoding="utf-8") as f:
            f.write(contenido)

    def test_lee_acento_con_comillas_dobles(self):
        self._escribir('mode = "dark"\naccent = "#dcd7ba"\n')
        self.assertEqual(theme._read_omarchy_accent(self.path), "#dcd7ba")

    def test_lee_acento_con_comillas_simples(self):
        self._escribir("accent = '#ff7800'\n")
        self.assertEqual(theme._read_omarchy_accent(self.path), "#ff7800")

    def test_lee_acento_sin_comillas(self):
        self._escribir("accent = #a000ff\n")
        self.assertEqual(theme._read_omarchy_accent(self.path), "#a000ff")

    def test_fichero_ausente_devuelve_fallback(self):
        self.assertEqual(
            theme._read_omarchy_accent(os.path.join(self.tmp, "nada.toml")),
            theme.OMARCHY_ACCENT_FALLBACK,
        )

    def test_sin_clave_accent_devuelve_fallback(self):
        self._escribir('mode = "dark"\nred = "#c34043"\n')
        self.assertEqual(
            theme._read_omarchy_accent(self.path), theme.OMARCHY_ACCENT_FALLBACK
        )

    def test_acento_invalido_devuelve_fallback(self):
        self._escribir('accent = "rojo"\n')
        self.assertEqual(
            theme._read_omarchy_accent(self.path), theme.OMARCHY_ACCENT_FALLBACK
        )

    def test_callbacks_se_registran_y_notifican(self):
        recibidos = []
        unregister = theme.on_accent_changed(recibidos.append)
        try:
            theme._accent_changed_callbacks_backup = list(
                theme._accent_changed_callbacks
            )
            theme._accent_changed()
            self.assertEqual(len(recibidos), 1)
        finally:
            unregister()
        theme._accent_changed()
        self.assertEqual(len(recibidos), 1)  # ya desregistrado


if __name__ == "__main__":
    unittest.main()
