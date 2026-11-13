#!/usr/bin/env python3
import os
import sys
import subprocess
import unicodedata

os.environ["GDK_BACKEND"] = "x11"
os.environ.pop("WAYLAND_DISPLAY", None)

REPO_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if REPO_DIR not in sys.path:
    sys.path.insert(0, REPO_DIR)

if os.path.basename(sys.path[0]) == "gui":
    pass
else:
    gui_dir = os.path.join(REPO_DIR, "gui")
    if gui_dir not in sys.path:
        sys.path.insert(0, gui_dir)

FAILURES = []


def report(name, ok, detail=""):
    if ok:
        sys.stdout.write("PASS: %s\n" % name)
    else:
        msg = str(detail).replace("\n", " ").strip()[:280]
        if msg:
            sys.stdout.write("FAIL: %s - %s\n" % (name, msg))
        else:
            sys.stdout.write("FAIL: %s\n" % name)
    sys.stdout.flush()
    if not ok:
        FAILURES.append(name)
    return ok


def ensure_display():
    if os.environ.get("DISPLAY"):
        return None
    import shutil
    import time
    import atexit
    if shutil.which("Xvfb") is None:
        return None
    for n in (99, 98, 97, 96, 95):
        disp = ":%d" % n
        sock = "/tmp/.X11-unix/X%d" % n
        if os.path.exists(sock):
            os.environ["DISPLAY"] = disp
            return None
        try:
            proc = subprocess.Popen(
                ["Xvfb", disp, "-screen", "0", "1024x768x24", "-nolisten", "tcp"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except Exception:
            continue
        found = False
        for _ in range(40):
            time.sleep(0.1)
            if proc.poll() is not None:
                break
            if os.path.exists(sock):
                found = True
                break
        if found and proc.poll() is None:
            os.environ["DISPLAY"] = disp

            def _cleanup(p=proc):
                try:
                    p.terminate()
                except Exception:
                    pass

            atexit.register(_cleanup)
            return proc
    return None


_XVFB = ensure_display()

SNIPPET = (
    "import builtins, sys\n"
    "_real = builtins.__import__\n"
    "def _hook(name, g=None, l=None, f=(), level=0):\n"
    "    if 'AyatanaAppIndicator3' in str(name):\n"
    "        raise ImportError('simulated no ayatana')\n"
    "    try:\n"
    "        fl = list(f or ())\n"
    "    except Exception:\n"
    "        fl = []\n"
    "    if any('AyatanaAppIndicator3' in str(x) for x in fl):\n"
    "        raise ImportError('simulated no ayatana')\n"
    "    return _real(name, g, l, f, level)\n"
    "builtins.__import__ = _hook\n"
    "try:\n"
    "    import gui.tray\n"
    "    sys.exit(10)\n"
    "except BaseException as e:\n"
    "    if type(e).__name__ == 'TrayUnavailable':\n"
    "        sys.exit(0)\n"
    "    sys.exit(11)\n"
)


def check_no_ayatana():
    env = dict(os.environ)
    env["GDK_BACKEND"] = "x11"
    env.pop("WAYLAND_DISPLAY", None)
    try:
        p = subprocess.run(
            [sys.executable, "-c", SNIPPET],
            cwd=REPO_DIR,
            env=env,
            capture_output=True,
            text=True,
            timeout=60,
        )
        ok = p.returncode == 0
        detail = "" if ok else ("exit=%s" % p.returncode)
        return report("sin_ayatana_TrayUnavailable", ok, detail)
    except Exception as e:
        return report("sin_ayatana_TrayUnavailable", False, e)


try:
    import gui.tray as tray_mod
except ImportError:
    import tray as tray_mod

from gi.repository import Gtk


class FakeClient:
    def __init__(self, color="ffffff", brightness=100):
        self.calls = []
        self._color = color
        self._brightness = brightness

    def get_state_file(self):
        return (self._color, self._brightness)

    def animation_running(self):
        return None

    def set_color(self, hex):
        self.calls.append(("set_color", str(hex)))

    def set_brightness(self, pct):
        self.calls.append(("set_brightness", int(pct)))

    def animation(self, mode):
        self.calls.append(("animation", str(mode)))

    def off(self):
        self.calls.append(("off",))

    def stop(self):
        self.calls.append(("stop",))
        return "stopped"

    def restore(self):
        self.calls.append(("restore",))
        return "restored"

    def clear(self):
        del self.calls[:]


def make_indicator(color="ffffff", brightness=100, toggle=None, prefs=None, about=None):
    fake = FakeClient(color, brightness)
    if toggle is None:
        toggle = lambda: None
    ind = tray_mod.TrayIndicator(fake, toggle, on_preferences=prefs, on_about=about)
    return ind, fake


def norm_label(s):
    try:
        t = str(s).lower()
    except Exception:
        return ""
    t = unicodedata.normalize("NFD", t)
    return "".join(c for c in t if unicodedata.category(c) != "Mn")


def root_menu(ind):
    try:
        m = getattr(ind, "_menu", None)
        if m is not None:
            return m
    except Exception:
        pass
    try:
        return ind._indicator.get_menu()
    except Exception:
        return None


def menu_children(menu):
    try:
        if menu is not None:
            return list(menu.get_children())
    except Exception:
        pass
    return []


def iter_items(menu):
    for it in menu_children(menu):
        yield it
        try:
            sub = it.get_submenu()
        except Exception:
            sub = None
        if sub is not None:
            for hijo in iter_items(sub):
                yield hijo


def label_of(item):
    try:
        lab = item.get_label()
        if lab:
            return lab
    except Exception:
        pass
    try:
        for ch in item.get_children():
            try:
                if hasattr(ch, "get_text"):
                    t = ch.get_text()
                    if t:
                        return t
            except Exception:
                continue
    except Exception:
        pass
    return ""


def find_item(ind, key):
    want = norm_label(key)
    for it in iter_items(root_menu(ind)):
        lab = norm_label(label_of(it))
        if want and want in lab:
            return it
    return None


def activate(item):
    try:
        item.emit("activate")
        return True
    except Exception:
        pass
    try:
        item.activate()
        return True
    except Exception:
        return False


def last_call(fake, name):
    for entry in reversed(fake.calls):
        if entry and entry[0] == name:
            return entry
    return None


def set_brightness_state(ind, fake, value):
    try:
        ind.update_state("fijar", "ffffff", value)
    except Exception:
        pass
    try:
        ind._brightness = value
    except Exception:
        pass
    try:
        fake.clear()
    except Exception:
        pass


class QuitPatch:
    """Inyecta el callback de salida limpia (sustituto de os._exit)."""

    def __init__(self, ind):
        self.hit = {}
        self._ind = ind

    def __enter__(self):
        self._ind._on_quit = lambda: self.hit.setdefault("quit_cb", True)
        return self.hit

    def __exit__(self, *exc):
        self._ind._on_quit = None
        return False


def check_menu_abrir():
    hit = {}
    ind, fake = make_indicator(toggle=lambda: hit.setdefault("toggle", True))
    item = find_item(ind, "abrir")
    if item is None:
        return report("menu_abrir", False, "item Abrir no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_abrir", False, e)
    return report("menu_abrir", hit.get("toggle") is True, "on_toggle_window no llamado")


def check_menu_fijar():
    ind, fake = make_indicator(color="1a2b3c", brightness=100)
    try:
        ind.update_state("fijar", "1a2b3c", 100)
    except Exception:
        pass
    try:
        ind._color = "1a2b3c"
    except Exception:
        pass
    fake.clear()
    item = find_item(ind, "fijar")
    if item is None:
        return report("menu_fijar", False, "item Fijar no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_fijar", False, e)
    got = last_call(fake, "set_color")
    ok = got is not None and str(got[1]).strip().lstrip("#").lower() == "1a2b3c"
    return report("menu_fijar", ok, "calls=%s" % (fake.calls,))


def check_menu_respirar():
    ind, fake = make_indicator()
    fake.clear()
    item = find_item(ind, "respir")
    if item is None:
        return report("menu_respirar", False, "item Respirar no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_respirar", False, e)
    got = last_call(fake, "animation")
    ok = got is not None and str(got[1]).lower() == "breathe"
    return report("menu_respirar", ok, "calls=%s" % (fake.calls,))


def check_menu_arcoiris():
    ind, fake = make_indicator()
    fake.clear()
    item = find_item(ind, "arco")
    if item is None:
        return report("menu_arcoiris", False, "item Arcoiris no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_arcoiris", False, e)
    got = last_call(fake, "animation")
    ok = got is not None and str(got[1]).lower() == "rainbow"
    return report("menu_arcoiris", ok, "calls=%s" % (fake.calls,))


def check_menu_apagar():
    ind, fake = make_indicator()
    fake.clear()
    item = find_item(ind, "apagar")
    if item is None:
        return report("menu_apagar", False, "item Apagar no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_apagar", False, e)
    got = last_call(fake, "off")
    return report("menu_apagar", got is not None, "calls=%s" % (fake.calls,))


def check_menu_restaurar():
    ind, fake = make_indicator()
    fake.clear()
    item = find_item(ind, "restaurar")
    if item is None:
        return report("menu_restaurar", False, "item Restaurar no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_restaurar", False, e)
    got = last_call(fake, "restore")
    return report("menu_restaurar", got is not None, "calls=%s" % (fake.calls,))


def check_menu_color_rojo():
    ind, fake = make_indicator()
    fake.clear()
    item = find_item(ind, "rojo")
    if item is None:
        return report("menu_color_rojo", False, "item Rojo no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_color_rojo", False, e)
    got = last_call(fake, "set_color")
    ok = got is not None and str(got[1]).strip().lstrip("#").lower() == "ff0000"
    return report("menu_color_rojo", ok, "calls=%s" % (fake.calls,))


def check_menu_preferencias():
    hit = {}
    ind, fake = make_indicator(prefs=lambda: hit.setdefault("prefs", True), about=lambda: None)
    item = find_item(ind, "prefer")
    if item is None:
        return report("menu_preferencias", False, "item Preferencias no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_preferencias", False, e)
    return report("menu_preferencias", hit.get("prefs") is True, "callback no llamado")


def check_menu_acerca():
    hit = {}
    ind, fake = make_indicator(prefs=lambda: None, about=lambda: hit.setdefault("about", True))
    item = find_item(ind, "acerca")
    if item is None:
        return report("menu_acerca", False, "item Acerca no encontrado")
    try:
        activate(item)
    except Exception as e:
        return report("menu_acerca", False, e)
    return report("menu_acerca", hit.get("about") is True, "callback no llamado")


def check_menu_salir():
    ind, fake = make_indicator()
    item = find_item(ind, "salir")
    if item is None:
        return report("menu_salir", False, "item Salir no encontrado")
    fake.clear()
    try:
        with QuitPatch(ind) as hit:
            activate(item)
        ok = hit.get("quit_cb") is True and last_call(fake, "stop") is not None
        return report("menu_salir", ok, "quit_cb=%s calls=%s" % (hit.get("quit_cb"), fake.calls))
    except Exception as e:
        return report("menu_salir", False, e)


def check_quit_stop():
    ind, fake = make_indicator()
    fake.clear()
    try:
        with QuitPatch(ind) as hit:
            ind.quit()
        ok = hit.get("quit_cb") is True and last_call(fake, "stop") is not None
        return report("quit_stop", ok, "quit_cb=%s calls=%s" % (hit.get("quit_cb"), fake.calls))
    except Exception as e:
        return report("quit_stop", False, e)


def check_slider_continuo():
    ind, fake = make_indicator(brightness=33)
    scale = getattr(ind, "_scale", None)
    if scale is None:
        return report("slider_continuo", False, "sin Gtk.Scale")
    try:
        adj = scale.get_adjustment()
        bajo, alto = adj.get_lower(), adj.get_upper()
    except Exception as e:
        return report("slider_continuo", False, e)
    ok = bajo == 0 and alto == 100
    return report("slider_continuo", ok, "rango %s-%s" % (bajo, alto))


def check_slider_valor():
    ind, fake = make_indicator(brightness=50)
    fake.clear()
    try:
        ind._scale.set_value(42)
    except Exception as e:
        return report("slider_valor", False, e)
    ok = ind._brightness == 42 and last_call(fake, "set_brightness") is None
    return report("slider_valor", ok,
                  "brightness=%s calls=%s (la escritura va con retardo)" % (ind._brightness, fake.calls))


def check_scroll(name, start, steps, expected):
    ind, fake = make_indicator(brightness=start)
    set_brightness_state(ind, fake, start)
    fn = getattr(ind, "_on_scroll", None)
    if fn is None:
        return report(name, False, "sin _on_scroll")
    try:
        try:
            base = getattr(ind, "_indicator", None)
            fn(base, steps, 0)
        except TypeError:
            fn(steps)
    except Exception as e:
        return report(name, False, e)
    got = last_call(fake, "set_brightness")
    val = got[1] if got is not None else None
    return report(name, val == expected, "esperado set_brightness(%s) obtenido %s calls=%s" % (expected, val, fake.calls))


def check_modo_activo():
    ind, fake = make_indicator()
    try:
        ind.update_state("breathe", "ffffff", 100)
        ok = bool(ind._mode_items["breathe"].get_active())
        ok = ok and not bool(ind._mode_items["fijar"].get_active())
    except Exception as e:
        return report("modo_activo", False, e)
    return report("modo_activo", ok, "radio de modo no sincronizado")


def expected_png(color_hex, mode):
    try:
        d = tray_mod._cache_dir()
    except Exception:
        base = os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache"))
        d = os.path.join(base, "ilutex")
    try:
        ch = tray_mod._normalize_hex(color_hex)
    except Exception:
        ch = str(color_hex).strip().lstrip("#").lower()
    try:
        mo = tray_mod._normalize_mode(mode)
    except Exception:
        mo = mode
    return os.path.join(d, "tray-%s-%s.png" % (ch, mo))


def check_update_state(mode, color):
    name = "update_state_%s" % str(mode)
    ind, fake = make_indicator()
    try:
        ind.update_state(mode, color, 50)
    except Exception as e:
        return report(name, False, e)
    path = expected_png(color, mode)
    ok = os.path.isfile(path) and os.path.getsize(path) > 0
    return report(name, ok, "png ausente %s" % path)


def main():
    check_no_ayatana()
    check_menu_abrir()
    check_slider_continuo()
    check_slider_valor()
    check_menu_fijar()
    check_menu_respirar()
    check_menu_arcoiris()
    check_menu_apagar()
    check_menu_color_rojo()
    check_menu_restaurar()
    check_menu_preferencias()
    check_menu_acerca()
    check_menu_salir()
    check_scroll("scroll_subir_80_85", 80, 1, 85)
    check_scroll("scroll_bajar_80_75", 80, -1, 75)
    check_scroll("scroll_subir_100_100", 100, 1, 100)
    check_scroll("scroll_bajar_0_0", 0, -1, 0)
    check_modo_activo()
    check_update_state("fijar", "ff0000")
    check_update_state("breathe", "00ff00")
    check_update_state("rainbow", "0000ff")
    check_update_state("off", "ffffff")
    check_quit_stop()
    if FAILURES:
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
