import json
import os
import shutil
import signal
import subprocess

try:
    from gui import log_exc
except ImportError:  # ejecución directa: gui/ en sys.path
    def log_exc(contexto):
        import sys

        tipo, exc, _tb = sys.exc_info()
        print(f"keybackcon-gui: {contexto}: {tipo.__name__}: {exc}",
              file=sys.stderr)


def find_binary():
    for cand in (
        shutil.which("keybackcon"),
        os.path.expanduser("~/.local/bin/keybackcon"),
        shutil.which("kbd-rgb"),
        os.path.expanduser("~/.local/bin/kbd-rgb"),
    ):
        if cand and os.path.isfile(cand) and os.access(cand, os.X_OK):
            return cand
    return "keybackcon"


def udev_install_argv(src: str, dst: str) -> list[str]:
    """Construye el argv de pkexec sin interpolar rutas en el script."""
    script = (
        'install -m644 "$1" "$2" && '
        "udevadm control --reload && "
        "udevadm trigger --subsystem-match=hidraw"
    )
    return ["pkexec", "sh", "-c", script, "sh", src, dst]


def _absolute_or_none(path: str | None) -> str | None:
    if not path or not str(path).strip():
        return None
    p = os.path.expanduser(str(path).strip())
    if not os.path.isabs(p):
        return None
    return p


def state_paths():
    base = _absolute_or_none(os.environ.get("XDG_STATE_HOME"))
    if base is None:
        home = _absolute_or_none(os.environ.get("HOME")) or os.path.expanduser("~")
        if not os.path.isabs(home):
            home = "/var/tmp"
        base = os.path.join(home, ".local/state")
    return (
        os.path.join(base, "keybackcon/state"),
        os.path.join(base, "kbd-rgb/state"),
    )


def pid_paths():
    base = _absolute_or_none(os.environ.get("XDG_RUNTIME_DIR"))
    if base is None:
        base = f"/tmp/keybackcon-{os.getuid()}"
    return (
        os.path.join(base, "keybackcon/animation.pid"),
        os.path.join(base, "keybackcon/anim.pid"),
        os.path.join(base, "kbd-rgb/anim.pid"),
    )


class KeybackconError(Exception):
    message: str

    def __init__(self, message: str):
        super().__init__(message)
        self.message = message


class KeybackconClient:
    def __init__(self, binary: str | None = None):
        self._bin = binary if binary else find_binary()
        self._proc = None

    def _run(self, args: list) -> subprocess.CompletedProcess:
        try:
            proc = subprocess.run(
                [self._bin] + args,
                capture_output=True,
                text=True,
                timeout=10,
            )
        except FileNotFoundError:
            raise KeybackconError(
                f"No encuentro el binario ({self._bin}). Instálalo primero."
            )
        except subprocess.TimeoutExpired:
            raise KeybackconError("El teclado tardó demasiado en responder.")
        except OSError as e:
            raise KeybackconError(f"El teclado no respondió: {e}")
        if proc.returncode != 0:
            err = (proc.stderr or proc.stdout or "").strip()
            if err:
                raise KeybackconError(err)
            raise KeybackconError(
                f"El teclado no respondió (código {proc.returncode})."
            )
        return proc

    def stop_animation(self) -> None:
        """Mata el proceso de animación lanzado, con tope de ~1 s.

        La animación arranca en su propio grupo de procesos
        (``start_new_session``), así que aquí se mata el grupo entero.
        """
        proc = self._proc
        self._proc = None
        if proc is None:
            return
        try:
            pgid = os.getpgid(proc.pid)
        except (ProcessLookupError, OSError):
            pgid = None
        # Solo matamos el grupo si el hijo es líder de sesión (es decir,
        # lo lanzamos nosotros con start_new_session); si no, terminate().
        propio_grupo = pgid is not None and pgid == proc.pid
        try:
            if propio_grupo:
                os.killpg(pgid, signal.SIGTERM)
            elif proc.poll() is None:
                proc.terminate()
        except (ProcessLookupError, PermissionError, OSError):
            pass  # carrera de salida: el proceso ya no existe
        try:
            proc.wait(timeout=1)
            return
        except subprocess.TimeoutExpired:
            pass  # sobrevive al TERM: escalamos a SIGKILL
        except (ChildProcessError, OSError):
            return  # zombie ya cosechado por otro waiter
        try:
            if propio_grupo:
                os.killpg(pgid, signal.SIGKILL)
            elif proc.poll() is None:
                proc.kill()
        except (ProcessLookupError, PermissionError, OSError):
            pass  # carrera de salida: el proceso ya no existe
        try:
            proc.wait(timeout=1)
        except Exception:
            log_exc("cosechar animación")  # el kernel recogerá el zombie

    def info(self) -> dict:
        """`info --json` como dict. Lanza KeybackconError si el binario falla.

        Si el binario es viejo y no habla JSON, devuelve {} para que la
        interfaz muestre «desconocido» en vez de romperse.
        """
        proc = self._run(["info", "--json"])
        try:
            lineas = (proc.stdout or "").strip().splitlines()
            data = json.loads(lineas[-1])
        except (ValueError, IndexError):
            return {}
        if not isinstance(data, dict):
            return {}
        return data

    def set_color(self, hex: str) -> None:
        self._run(["set", hex])

    def set_brightness(self, pct: int) -> None:
        self._run(["brightness", str(pct)])

    def animation(self, mode: str) -> None:
        self.stop_animation()
        self._run(["firmware-effects", "off"])
        try:
            self._proc = subprocess.Popen(
                [self._bin, "animation", mode],
                start_new_session=True,
            )
        except FileNotFoundError:
            raise KeybackconError(
                f"No encuentro el binario ({self._bin}). Instálalo primero."
            )
        except OSError as e:
            raise KeybackconError(f"El teclado no respondió: {e}")

    def stop(self) -> str:
        self.stop_animation()
        proc = self._run(["stop"])
        return proc.stdout.strip()

    def restore(self) -> str:
        self.stop_animation()
        proc = self._run(["restore"])
        return proc.stdout.strip()

    def off(self) -> None:
        self._run(["off"])

    def firmware_effects(self, on: bool) -> None:
        self._run(["firmware-effects", "on" if on else "off"])

    def get_state_file(self) -> tuple[str, int]:
        for path in state_paths():
            try:
                with open(path) as f:
                    raw = f.read()
                # Formato versionado (schema_version=…).
                if "schema_version=" in raw or "base_color=" in raw:
                    color, pct = "ffffff", 100
                    for line in raw.splitlines():
                        if "=" not in line:
                            continue
                        k, _, v = line.partition("=")
                        k, v = k.strip(), v.strip()
                        if k == "base_color":
                            color = v.lstrip("#").lower()
                        elif k == "brightness":
                            pct = int(v)
                    if len(color) == 6:
                        int(color, 16)
                        return color, max(0, min(100, pct))
                    continue
                parts = raw.split()
                color = parts[0].lstrip("#").lower()
                pct = int(parts[1]) if len(parts) > 1 else 100
                if len(color) != 6:
                    raise ValueError
                int(color, 16)
                return color, max(0, min(100, pct))
            except Exception:
                continue
        return "ffffff", 100

    def animation_running(self) -> int | None:
        for path in pid_paths():
            try:
                with open(path) as f:
                    pid = int(f.read().strip())
                if pid <= 0:
                    continue
                # /proc en lugar de kill(0): algunos sandboxes bloquean señales.
                if not os.path.exists(f"/proc/{pid}"):
                    os.unlink(path)
                    continue
                with open(f"/proc/{pid}/cmdline", "rb") as cf:
                    cmd = cf.read().decode(errors="ignore")
                if "keybackcon" in cmd or "kbd-rgb" in cmd:
                    return pid
                os.unlink(path)
            except Exception:
                continue
        return None
