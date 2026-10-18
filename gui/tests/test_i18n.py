"""i18n: msgids en inglés, catálogos es/en y resolución de idioma."""
import gettext
import os
import unittest
from unittest import mock

from gui import i18n

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
LOCALE_DIR = os.path.join(ROOT, "gui", "locale")
POT = os.path.join(ROOT, "po", "keybackcon.pot")


def _catalogo(lang):
    """Traducción compilada del repo; salta si falta el `.mo` (`make i18n`)."""
    mo = os.path.join(LOCALE_DIR, lang, "LC_MESSAGES", "keybackcon.mo")
    if not os.path.isfile(mo):
        raise unittest.SkipTest(f"catálogo ausente: {mo}; ejecuta `make i18n`")
    return gettext.translation("keybackcon", localedir=LOCALE_DIR, languages=[lang])


def _requiere_catalogo(lang):
    """Salta si el `.mo` del idioma no está compilado."""
    _catalogo(lang)


class TestI18n(unittest.TestCase):
    def tearDown(self):
        i18n.setup_i18n("es")

    def test_setup_devuelve_idioma_soportado(self):
        self.assertEqual(i18n.setup_i18n("es"), "es")
        self.assertEqual(i18n.get_language(), "es")

    def test_codigo_con_region_y_mayusculas(self):
        self.assertEqual(i18n.setup_i18n("ES-ES"), "es")
        self.assertEqual(i18n.setup_i18n("en_US"), "en")

    def test_idioma_no_soportado_ca_a_es(self):
        self.assertEqual(i18n.setup_i18n("fr"), "es")

    def test_nulo_usa_el_locale_del_sistema(self):
        with mock.patch("gui.i18n.locale.getdefaultlocale", return_value=("en_US", "UTF-8")):
            self.assertEqual(i18n.setup_i18n(None), "en")
        with mock.patch("gui.i18n.locale.getdefaultlocale", return_value=(None, None)):
            self.assertEqual(i18n.setup_i18n(None), "es")

    def test_normalize_code_directo(self):
        self.assertEqual(i18n._normalize_code("EN-gb"), "en")
        self.assertEqual(i18n._normalize_code(""), "es")
        self.assertEqual(i18n._normalize_code("pt_BR"), "es")

    def test_translation_object_con_gettext(self):
        i18n.setup_i18n("es")
        self.assertTrue(hasattr(i18n._translation, "gettext"))

    def test_catalogo_es_traduce_del_ingles(self):
        tr = _catalogo("es")
        self.assertEqual(tr.gettext("Preferences"), "Preferencias")
        self.assertEqual(tr.gettext("White"), "Blanco")
        self.assertEqual(tr.gettext("Steady"), "Fijar")
        self.assertEqual(
            tr.gettext("{mode} motion running"), "Movimiento {mode} en marcha"
        )

    def test_catalogo_en_es_identidad(self):
        tr = _catalogo("en")
        self.assertEqual(tr.gettext("Preferences"), "Preferences")
        self.assertEqual(tr.gettext("White"), "White")

    def test_setup_i18n_rebinda_el_guion_bajo(self):
        _requiere_catalogo("es")
        _requiere_catalogo("en")
        with mock.patch.object(i18n, "get_locale_dir", return_value=LOCALE_DIR):
            i18n.setup_i18n("es")
            self.assertEqual(i18n._("Preferences"), "Preferencias")
            i18n.setup_i18n("en")
            self.assertEqual(i18n._("Preferences"), "Preferences")

    def test_sin_catalogo_la_traduccion_es_identidad(self):
        with mock.patch.object(i18n, "get_locale_dir", return_value="/no/existe"):
            i18n.setup_i18n("es")
            self.assertEqual(i18n._("Preferences"), "Preferences")

    def test_N_marca_sin_traducir(self):
        self.assertEqual(i18n.N_("Preferences"), "Preferences")

    def test_los_colores_usan_msgid_extraible(self):
        from gui import colors

        with open(POT, encoding="utf-8") as fh:
            pot = fh.read()
        for nombre, _hexv in colors.COLORES:
            self.assertIn(f'msgid "{nombre}"', pot, f"{nombre} no está en el .pot")


if __name__ == "__main__":
    unittest.main()
