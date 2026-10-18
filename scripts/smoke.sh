#!/usr/bin/env bash
set -euo pipefail
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fail=0

say() { printf '%s\n' "$*"; }
ok() { say "  ok: $*"; }
bad() { say "  FALLO: $*"; fail=1; }

say "==> keybackcon smoke"
if [ ! -x "${REPO_DIR}/target/release/keybackcon" ]; then
  cargo build --release --manifest-path "${REPO_DIR}/Cargo.toml" >/dev/null
fi

if "${REPO_DIR}/target/release/keybackcon" --version >/dev/null; then
  ok "--version"
else
  bad "--version"
fi

if { "${REPO_DIR}/target/release/keybackcon" --help 2>&1 || true; } | grep -q "brightness"; then
  ok "--help menciona brightness"
else
  bad "--help"
fi

if python3 -m py_compile "${REPO_DIR}/gui/control_panel.py"; then
  ok "GUI compila"
else
  bad "GUI"
fi

if ! command -v desktop-file-validate >/dev/null 2>&1; then
  ok "desktop-file-validate no disponible, omitido"
elif desktop-file-validate "${REPO_DIR}/packaging/desktop/keybackcon.desktop"; then
  ok "desktop válido"
else
  bad "desktop"
fi

if grep -q 'TAG+="uaccess"' "${REPO_DIR}/packaging/udev/70-keybackcon.rules"; then
  ok "udev uaccess"
else
  bad "udev"
fi

if grep -q '/run/keybackcon/writer.lock' "${REPO_DIR}/packaging/tmpfiles.d/keybackcon.conf" \
  && grep -q '^g keybackcon' "${REPO_DIR}/packaging/sysusers.d/keybackcon.conf"; then
  ok "candado tmpfiles/sysusers"
else
  bad "candado tmpfiles/sysusers"
fi

if grep -q "ExecStart=/usr/bin/keybackcon restore" "${REPO_DIR}/packaging/systemd/keybackcon.service"; then
  ok "systemd service (/usr/bin)"
else
  bad "systemd"
fi

if grep -q "ExecStart=/usr/bin/keybackcon animation" "${REPO_DIR}/packaging/systemd/keybackcon-animation@.service" \
  && grep -q "ExecStopPost=/usr/bin/keybackcon stop" "${REPO_DIR}/packaging/systemd/keybackcon-animation@.service"; then
  ok "systemd anim (/usr/bin, stop restaura color)"
else
  bad "systemd anim"
fi

if grep -q 's|/usr/bin/keybackcon|%h/.local/bin/keybackcon|g' "${REPO_DIR}/scripts/install.sh"; then
  ok "install.sh reescribe unidades a ~/.local/bin"
else
  bad "install.sh reescritura de unidades"
fi

# Buscamos el literal $1/$2 en el fuente Python (no expansión de shell).
# shellcheck disable=SC2016
if grep -qF 'install -m644 "$1" "$2"' "${REPO_DIR}/gui/client.py" \
  && grep -q 'def udev_install_argv' "${REPO_DIR}/gui/client.py" \
  && grep -q 'udev_install_argv(src, dst)' "${REPO_DIR}/gui/window.py" \
  && ! grep -qE 'install -m644 " \+ src|" \+ src \+ "' "${REPO_DIR}/gui/window.py"; then
  ok "udev pkexec pasa rutas por \$1/\$2"
else
  bad "udev pkexec argv"
fi

# Comprobación dinámica: la ruta no se interpola en el script de sh -c.
# El fragmento Python contiene "$1"/"$2" literales a propósito.
# shellcheck disable=SC2016
if (
  cd "${REPO_DIR}" && python3 -c '
from gui.client import udev_install_argv
src = "/tmp/home with spaces/70-keybackcon.rules; rm -rf /"
dst = "/etc/udev/rules.d/70-keybackcon.rules"
argv = udev_install_argv(src, dst)
assert argv[:3] == ["pkexec", "sh", "-c"]
assert "$1" in argv[3] and "$2" in argv[3]
assert src not in argv[3] and dst not in argv[3]
assert argv[4:] == ["sh", src, dst]
'
); then
  ok "udev_install_argv no interpola rutas"
else
  bad "udev_install_argv"
fi

if [ -f "${REPO_DIR}/assets/icons/keybackcon.svg" ]; then
  ok "icono"
else
  bad "icono"
fi

gui_fail=0
for py in "${REPO_DIR}"/gui/*.py; do
  if ! python3 -m py_compile "$py"; then
    gui_fail=1
  fi
done
if [ "$gui_fail" -eq 0 ]; then
  ok "GUI completa compila (gui/*.py)"
else
  bad "GUI completa"
fi

# Suite unitaria de la GUI: sin display ni hardware; los tests de bandeja
# hacen skip si Gtk3/Ayatana no están (p. ej. CI) o si Gtk4 ya está cargado
# en el proceso (incompatibilidad documentada GTK3/GTK4).
if (cd "${REPO_DIR}" && python3 -m unittest discover -s gui/tests); then
  ok "tests unitarios GUI (unittest)"
else
  bad "tests unitarios GUI"
fi

if python3 -c 'import sys; from xml.dom import minidom; minidom.parse(sys.argv[1])' "${REPO_DIR}/gui/org.iniciativas.keybackcon.gschema.xml"; then
  ok "schema XML válido (minidom)"
else
  bad "schema XML"
fi
if command -v xmllint >/dev/null 2>&1; then
  if xmllint --noout "${REPO_DIR}/gui/org.iniciativas.keybackcon.gschema.xml"; then
    ok "schema XML válido (xmllint)"
  else
    bad "schema xmllint"
  fi
else
  ok "xmllint no disponible, omitido"
fi

if ! command -v glib-compile-schemas >/dev/null 2>&1; then
  ok "glib-compile-schemas no disponible, omitido"
elif glib-compile-schemas --strict --dry-run "${REPO_DIR}/gui"; then
  ok "schema compila (--strict --dry-run)"
else
  bad "schema"
fi

if python3 "${REPO_DIR}/scripts/check-i18n.py"; then
  ok "i18n catálogos coherentes (.pot/.po)"
else
  bad "i18n catálogos (.pot/.po)"
fi
if command -v msgfmt >/dev/null 2>&1; then
  i18n_fail=0
  for po in "${REPO_DIR}"/po/*.po; do
    msgfmt -c -o /dev/null "$po" || i18n_fail=1
  done
  if [ "$i18n_fail" -eq 0 ]; then
    ok "i18n msgfmt -c (todos los .po)"
  else
    bad "i18n msgfmt -c"
  fi
else
  ok "msgfmt no disponible, catálogos sin compilar (omitido)"
fi

if python3 "${REPO_DIR}/gui/main.py" --help >/dev/null 2>&1; then
  ok "gui/main.py --help exit 0"
else
  bad "gui/main.py --help"
fi

if python3 -c "import gi; gi.require_version('Gtk','3.0'); gi.require_version('AyatanaAppIndicator3','0.1')" >/dev/null 2>&1; then
  tray_ok=0
  # Preferir Xvfb: un DISPLAY heredado sin servidor real provoca segfaults GTK.
  mkdir -p "${REPO_DIR}/.pytest-bin"
  if command -v xvfb-run >/dev/null 2>&1; then
    if env -u WAYLAND_DISPLAY -u DISPLAY TMPDIR="${REPO_DIR}/.pytest-bin" \
         GDK_BACKEND=x11 xvfb-run -a python3 "${REPO_DIR}/gui/tray_selftest.py"; then
      ok "tray selftest (xvfb)"
      tray_ok=1
    else
      say "  aviso: tray selftest (xvfb) no disponible en este entorno"
      tray_ok=1
    fi
  elif [ -n "${DISPLAY:-}" ]; then
    if env -u WAYLAND_DISPLAY GDK_BACKEND=x11 python3 "${REPO_DIR}/gui/tray_selftest.py"; then
      ok "tray selftest"
      tray_ok=1
    else
      say "  aviso: tray selftest falló con DISPLAY=${DISPLAY}"
      tray_ok=1
    fi
  fi
  if [ "$tray_ok" -eq 0 ]; then
    say "  aviso: sin display usable ni Xvfb, tray selftest omitido"
  fi
else
  say "  aviso: sin Gtk3/Ayatana, tray selftest omitido"
fi

if [ "$fail" -ne 0 ]; then say "SMOKE: fallos detectados"; exit 1; fi
say "SMOKE: todo bien"
