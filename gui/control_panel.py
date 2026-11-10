#!/usr/bin/env python3
import os

try:
    from gui import api
except ImportError:  # ejecución directa: gui/ en sys.path
    import sys

    _here = os.path.dirname(os.path.abspath(__file__))
    _root = os.path.dirname(_here)
    if _root not in sys.path:
        sys.path.insert(0, _root)
    from gui import api

load, log_exc, _gettext_func = api(__file__)

_window = load("window")
MesaWindow = _window.MesaWindow
main = _window.main
on_activate = _window.on_activate
APP_ID = _window.APP_ID
APP_TITLE = _window.APP_TITLE
_colors = load("colors")
COLORES = _colors.COLORES
hex_to_rgb = _colors.hex_to_rgb
scale_rgb = _colors.scale_rgb
hsv_to_rgb = _colors.hsv_to_rgb
_client = load("client")
KeybackconClient = _client.KeybackconClient
KeybackconError = _client.KeybackconError
find_binary = _client.find_binary
state_paths = _client.state_paths
pid_paths = _client.pid_paths
animations_enabled = load("preview").animations_enabled


BIN = find_binary()


def get_state():
    return KeybackconClient().get_state_file()


def animation_running():
    return KeybackconClient().animation_running()


def device_info():
    try:
        return KeybackconClient().info()
    except Exception:
        log_exc("info del dispositivo")
        return None


if __name__ == "__main__":
    main()
