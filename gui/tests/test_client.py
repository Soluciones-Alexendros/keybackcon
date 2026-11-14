import json
import os
import shutil
import stat
import subprocess
import tempfile
import unittest
from unittest import mock

from gui.client import (
    KeybackconClient,
    KeybackconError,
    find_binary,
    pid_paths,
    state_paths,
    udev_install_argv,
)


class BinarioFake:
    """Crea un ejecutable de mentira en un directorio ejecutable.

    Usa XDG_RUNTIME_DIR (o /var/tmp) porque algunos /tmp están montados
    con noexec y el binario no podría arrancar.
    """

    # Directorio del repo: /tmp y XDG_RUNTIME_DIR suelen ir con noexec.
    BASE_EJECUTABLE = os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
        ".pytest-bin",
    )

    def __init__(self, contenido="#!/bin/sh\nexit 0\n"):
        os.makedirs(self.BASE_EJECUTABLE, exist_ok=True)
        self.dir = tempfile.mkdtemp(prefix="kbc-test-bin-", dir=self.BASE_EJECUTABLE)
        self.ruta = os.path.join(self.dir, "ilutex")
        with open(self.ruta, "w") as f:
            f.write(contenido)
        os.chmod(self.ruta, os.stat(self.ruta).st_mode | stat.S_IEXEC)

    def limpiar(self):
        shutil.rmtree(self.dir, ignore_errors=True)


class EntornoXdg:
    """XDG_STATE_HOME/XDG_RUNTIME_DIR temporales, restaurados al salir."""

    def __init__(self):
        self._guardados = {}
        for var in ("XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
            self._guardados[var] = os.environ.get(var)
        self.tmp = tempfile.mkdtemp(prefix="kbc-test-xdg-")
        os.environ["XDG_STATE_HOME"] = os.path.join(self.tmp, "state")
        os.environ["XDG_RUNTIME_DIR"] = os.path.join(self.tmp, "run")
        os.makedirs(os.environ["XDG_STATE_HOME"], exist_ok=True)
        os.makedirs(os.environ["XDG_RUNTIME_DIR"], exist_ok=True)

    def restaurar(self):
        for var, valor in self._guardados.items():
            if valor is None:
                os.environ.pop(var, None)
            else:
                os.environ[var] = valor
        shutil.rmtree(self.tmp, ignore_errors=True)


class TestRutas(unittest.TestCase):
    def test_state_paths_respeta_xdg(self):
        env = EntornoXdg()
        try:
            rutas = state_paths()
            self.assertIsNotNone(rutas)
            assert rutas is not None
            nuevo, legado = rutas
            self.assertTrue(nuevo.startswith(env.tmp))
            self.assertTrue(nuevo.endswith("ilutex/state"))
            self.assertTrue(legado.endswith("keybackcon/state"))
        finally:
            env.restaurar()

    def test_state_paths_sin_home_absoluto_no_usa_var_tmp(self):
        guardados = {k: os.environ.get(k) for k in ("HOME", "XDG_STATE_HOME")}
        try:
            os.environ.pop("XDG_STATE_HOME", None)
            os.environ["HOME"] = "relativo/no-absoluto"
            self.assertIsNone(state_paths())
            os.environ.pop("HOME", None)
            self.assertIsNone(state_paths())
        finally:
            for clave, valor in guardados.items():
                if valor is None:
                    os.environ.pop(clave, None)
                else:
                    os.environ[clave] = valor

    def test_pid_paths_respeta_runtime(self):
        env = EntornoXdg()
        try:
            paths = pid_paths()
            self.assertTrue(paths[0].startswith(env.tmp))
            self.assertTrue(paths[0].endswith("ilutex/animation.pid"))
            self.assertEqual(len(paths), 4)
        finally:
            env.restaurar()

    def test_udev_install_argv_no_interpolo_rutas(self):
        argv = udev_install_argv("/tmp/src rules", "/etc/udev/rules.d/70 x.rules")
        self.assertEqual(argv[0:3], ["pkexec", "sh", "-c"])
        # Las rutas viajan como argumentos tras 'sh', nunca interpoladas.
        self.assertEqual(argv[4:], ["sh", "/tmp/src rules", "/etc/udev/rules.d/70 x.rules"])
        self.assertNotIn("/tmp/src rules", argv[3])
        self.assertIn('"$1"', argv[3])
        self.assertIn('"$2"', argv[3])


class TestFindBinary(unittest.TestCase):
    def test_prefiere_path_y_exigente(self):
        fake = BinarioFake()
        otro = os.path.join(fake.dir, "no-exe")
        with open(otro, "w") as f:
            f.write("#!/bin/sh\n")
        with mock.patch("gui.client.shutil.which", return_value=fake.ruta):
            self.assertEqual(find_binary(), fake.ruta)
        fake.limpiar()

    def test_fallback_cuando_nada_existe(self):
        with mock.patch("gui.client.shutil.which", return_value=None), mock.patch(
            "os.path.isfile", return_value=False
        ):
            self.assertEqual(find_binary(), "ilutex")


class TestRunErrores(unittest.TestCase):
    def test_binario_inexistente(self):
        cliente = KeybackconClient(binary="/no/existe/ilutex")
        with self.assertRaises(KeybackconError) as ctx:
            cliente.info()
        self.assertIn("Cannot find the binary", ctx.exception.message)

    def test_error_con_stderr(self):
        fake = BinarioFake("#!/bin/sh\necho 'teclado furioso' >&2\nexit 1\n")
        cliente = KeybackconClient(binary=fake.ruta)
        with self.assertRaises(KeybackconError) as ctx:
            cliente.set_color("ff0000")
        self.assertIn("teclado furioso", ctx.exception.message)
        fake.limpiar()

    def test_error_sin_salida(self):
        fake = BinarioFake("#!/bin/sh\nexit 3\n")
        cliente = KeybackconClient(binary=fake.ruta)
        with self.assertRaises(KeybackconError) as ctx:
            cliente.off()
        self.assertIn("code 3", ctx.exception.message)
        fake.limpiar()

    def test_timeout(self):
        cliente = KeybackconClient(binary="/usr/bin/true")

        def timeout_run(*_a, **_k):
            raise subprocess.TimeoutExpired(cmd="ilutex", timeout=10)

        with mock.patch("gui.client.subprocess.run", side_effect=timeout_run):
            with self.assertRaises(KeybackconError) as ctx:
                cliente.restore()
        self.assertIn("took too long to respond", ctx.exception.message)


class TestInfoJson(unittest.TestCase):
    def test_json_valido(self):
        datos = {"version": "2.5.0", "lamps": 1, "state": "00ffff", "brightness": 95}
        fake = BinarioFake('#!/bin/sh\necho \'%s\'\n' % json.dumps(datos))
        cliente = KeybackconClient(binary=fake.ruta)
        self.assertEqual(cliente.info(), datos)
        fake.limpiar()

    def test_binario_viejo_sin_json_devuelve_vacio(self):
        fake = BinarioFake("#!/bin/sh\necho 'nº lámparas : 1'\n")
        cliente = KeybackconClient(binary=fake.ruta)
        self.assertEqual(cliente.info(), {})
        fake.limpiar()

    def test_basura_devuelve_vacio(self):
        fake = BinarioFake("#!/bin/sh\necho '{no-es-json'\n")
        cliente = KeybackconClient(binary=fake.ruta)
        self.assertEqual(cliente.info(), {})
        fake.limpiar()


class TestEstadoYPidfile(unittest.TestCase):
    def test_get_state_file_parsing_y_clamp(self):
        env = EntornoXdg()
        try:
            dir_state = os.path.join(env.tmp, "state", "ilutex")
            os.makedirs(dir_state, exist_ok=True)
            with open(os.path.join(dir_state, "state"), "w") as f:
                f.write("FF7800 140\n")
            self.assertEqual(KeybackconClient().get_state_file(), ("ff7800", 100))
            with open(os.path.join(dir_state, "state"), "w") as f:
                f.write("00ffff -5\n")
            self.assertEqual(KeybackconClient().get_state_file(), ("00ffff", 0))
        finally:
            env.restaurar()

    def test_get_state_file_malformado_no_cae_a_legado(self):
        """KBC-12/16: primario corrupto no consulta legado ni inventa color real."""
        env = EntornoXdg()
        try:
            base = env.tmp
            nuevo = os.path.join(base, "state", "ilutex")
            legado = os.path.join(base, "state", "kbd-rgb")
            os.makedirs(nuevo, exist_ok=True)
            os.makedirs(legado, exist_ok=True)
            with open(os.path.join(nuevo, "state"), "w") as f:
                f.write("basura sin sentido\n")
            with open(os.path.join(legado, "state"), "w") as f:
                f.write("ff0000 60\n")
            self.assertEqual(KeybackconClient().get_state_file(), ("ffffff", 100))
        finally:
            env.restaurar()

    def test_get_state_file_ausente_devuelve_defaults(self):
        env = EntornoXdg()
        try:
            self.assertEqual(KeybackconClient().get_state_file(), ("ffffff", 100))
        finally:
            env.restaurar()

    def test_get_state_file_solo_legado(self):
        env = EntornoXdg()
        try:
            legado = os.path.join(env.tmp, "state", "keybackcon")
            os.makedirs(legado, exist_ok=True)
            with open(os.path.join(legado, "state"), "w") as f:
                f.write("ff0000 60\n")
            self.assertEqual(KeybackconClient().get_state_file(), ("ff0000", 60))
        finally:
            env.restaurar()

    def test_kbc16_oraculo_compartido_testdata(self):
        from gui.client import parse_state_text

        root = os.path.join(
            os.path.dirname(__file__), "..", "..", "testdata", "state"
        )
        oracle = os.path.join(root, "oracle.txt")
        with open(oracle) as f:
            lines = f.readlines()
        for line in lines:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split()
            kind, name = parts[0], parts[1]
            with open(os.path.join(root, name)) as f:
                raw = f.read()
            got_kind, payload = parse_state_text(raw)
            self.assertEqual(got_kind, kind, name)
            if kind == "ok":
                self.assertEqual(payload, (parts[2], int(parts[3])), name)
            elif kind == "unsupported":
                self.assertEqual(payload, int(parts[2]), name)
            else:
                self.assertIsNone(payload, name)

    def test_animation_running_detecta_proceso_propio(self):
        env = EntornoXdg()
        try:
            pidfile = os.path.join(env.tmp, "run", "ilutex", "animation.pid")
            os.makedirs(os.path.dirname(pidfile), exist_ok=True)
            # Wrapper cuyo cmdline contiene 'ilutex' tras el exec.
            # Script con argv0 implícito en el nombre del fichero.
            fake = BinarioFake("#!/bin/sh\nexec sleep 30\n")
            wrapper = subprocess.Popen([fake.ruta], start_new_session=True)
            try:
                with open(pidfile, "w") as f:
                    f.write("%d\n" % wrapper.pid)
                encontrado = KeybackconClient().animation_running()
                self.assertEqual(encontrado, wrapper.pid)
            finally:
                try:
                    wrapper.kill()
                except OSError:
                    pass
                try:
                    wrapper.wait(timeout=5)
                except Exception:
                    pass
                fake.limpiar()
        finally:
            env.restaurar()

    def test_animation_running_pidfile_ausente(self):
        env = EntornoXdg()
        try:
            self.assertIsNone(KeybackconClient().animation_running())
        finally:
            env.restaurar()


class TestAnimacionGrupo(unittest.TestCase):
    def test_stop_animation_mata_el_grupo(self):
        fake = BinarioFake("#!/bin/sh\nsleep 5\n")
        cliente = KeybackconClient(binary=fake.ruta)
        cliente._proc = subprocess.Popen(
            [fake.ruta], start_new_session=True
        )
        pid = cliente._proc.pid
        cliente.stop_animation()
        self.assertIsNone(cliente._proc)
        # Si el entorno permite señales, el proceso ya no está; si no, al menos
        # el cliente olvidó la referencia (sandboxes que bloquean kill).
        if os.path.exists(f"/proc/{pid}"):
            try:
                os.kill(pid, 9)
            except OSError:
                pass
        fake.limpiar()

    def test_stop_animation_idempotente_sin_proceso(self):
        cliente = KeybackconClient(binary="/no/existe")
        cliente.stop_animation()  # no debe lanzar excepción


if __name__ == "__main__":
    unittest.main()
