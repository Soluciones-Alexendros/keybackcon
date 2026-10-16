#!/usr/bin/env bash
set -euo pipefail
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DST="${HOME}/.local/bin"
UNIT_DST="${HOME}/.config/systemd/user"
DESK_DST="${HOME}/.local/share/applications"
ICON_DST="${HOME}/.local/share/icons/hicolor/scalable/apps"

# Gestor de paquetes de la distro (para pistas de instalación concretas).
pkg_manager() {
  local id=""
  if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    id=" ${ID:-} ${ID_LIKE:-} "
  fi
  case "$id" in
    *arch*|*manjaro*|*endeavouros*) echo pacman ;;
    *fedora*|*rhel*|*centos*|*nobara*) echo dnf ;;
    *opensuse*|*suse*) echo zypper ;;
    *debian*|*ubuntu*|*mint*|*pop*) echo apt ;;
    *) echo "" ;;
  esac
}

# pkg_hint <gettext|glib|tray>: comando concreto para instalar el rol.
pkg_hint() {
  local mgr
  mgr="$(pkg_manager)"
  case "${mgr}:$1" in
    apt:gettext)    echo "sudo apt install gettext" ;;
    apt:glib)       echo "sudo apt install libglib2.0-bin" ;;
    apt:tray)       echo "sudo apt install gir1.2-ayatanaappindicator3-0.1 python3-cairo" ;;
    pacman:gettext) echo "sudo pacman -S gettext" ;;
    pacman:glib)    echo "sudo pacman -S glib2" ;;
    pacman:tray)    echo "sudo pacman -S libayatana-appindicator python-cairo" ;;
    dnf:gettext)    echo "sudo dnf install gettext" ;;
    dnf:glib)       echo "sudo dnf install glib2" ;;
    dnf:tray)       echo "sudo dnf install libayatana-appindicator-gtk3 python3-cairo" ;;
    zypper:gettext) echo "sudo zypper install gettext" ;;
    zypper:glib)    echo "sudo zypper install glib2" ;;
    zypper:tray)    echo "sudo zypper install libayatana-appindicator3-1 python3-cairo" ;;
    *:gettext)      echo "instala gettext con tu gestor de paquetes" ;;
    *:glib)         echo "instala glib2 (glib-compile-schemas) con tu gestor de paquetes" ;;
    *:tray)         echo "instala AyatanaAppIndicator3 y pycairo con tu gestor de paquetes" ;;
    *)              echo "instala '$1' con tu gestor de paquetes" ;;
  esac
}

echo "==> keybackcon: compilar (release)"
cargo build --release --manifest-path "${REPO_DIR}/Cargo.toml"

echo "==> keybackcon: instalar binario en ${BIN_DST}/keybackcon"
mkdir -p "${BIN_DST}"
install -m755 "${REPO_DIR}/target/release/keybackcon" "${BIN_DST}/keybackcon"

echo "==> keybackcon: instalar GUI en ${BIN_DST}/keybackcon-gui"
GUI_DST="${HOME}/.local/share/keybackcon/gui"
mkdir -p "${GUI_DST}"
install -m644 "${REPO_DIR}"/gui/*.py "${GUI_DST}/"
install -m644 "${REPO_DIR}/packaging/udev/70-keybackcon.rules" "${HOME}/.local/share/keybackcon/70-keybackcon.rules"
install -m755 "${REPO_DIR}/packaging/keybackcon-gui" "${BIN_DST}/keybackcon-gui"

echo "==> keybackcon: compilar traducciones (es)"
MO_SRC="${REPO_DIR}/po/es.po"
MO_BUILD="${REPO_DIR}/gui/locale/es/LC_MESSAGES/keybackcon.mo"
MO_DST="${HOME}/.local/share/locale/es/LC_MESSAGES/keybackcon.mo"
if command -v msgfmt >/dev/null 2>&1; then
  mkdir -p "$(dirname "${MO_BUILD}")"
  msgfmt -o "${MO_BUILD}" "${MO_SRC}"
  echo "    .mo generado en ${MO_BUILD}"
else
  echo "    aviso: msgfmt no disponible, se omite la compilación del .mo (la GUI usará el idioma por defecto)" >&2
  echo "    pista: $(pkg_hint gettext)" >&2
fi
if [ -f "${MO_BUILD}" ]; then
  mkdir -p "$(dirname "${MO_DST}")"
  install -m644 "${MO_BUILD}" "${MO_DST}"
  echo "    locale instalado en ${MO_DST}"
else
  echo "    aviso: sin .mo compilado, se omite la instalación del locale" >&2
fi

echo "==> keybackcon: instalar schema GSettings"
SCHEMA_SRC="${REPO_DIR}/gui/org.iniciativas.keybackcon.gschema.xml"
SCHEMA_DST_DIR="${HOME}/.local/share/glib-2.0/schemas"
mkdir -p "${SCHEMA_DST_DIR}"
install -m644 "${SCHEMA_SRC}" "${SCHEMA_DST_DIR}/org.iniciativas.keybackcon.gschema.xml"
if command -v glib-compile-schemas >/dev/null 2>&1; then
  glib-compile-schemas "${SCHEMA_DST_DIR}" || true
else
  echo "    aviso: glib-compile-schemas no disponible, se omite la compilación de schemas" >&2
  echo "    pista: $(pkg_hint glib)" >&2
fi
# Nota: sin autostart global. El autostart es opt-in desde el diálogo de
# preferencias (gui/settings.py), que crea ~/.config/autostart/keybackcon-tray.desktop bajo demanda.

echo "==> keybackcon: instalar lanzador + icono"
mkdir -p "${DESK_DST}" "${ICON_DST}"
install -m644 "${REPO_DIR}/packaging/desktop/keybackcon.desktop" "${DESK_DST}/keybackcon.desktop"
install -m644 "${REPO_DIR}/assets/icons/keybackcon.svg" "${ICON_DST}/keybackcon.svg"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${DESK_DST}" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f "${HOME}/.local/share/icons/hicolor" || true
fi

echo "==> keybackcon: instalar unidades de usuario"
mkdir -p "${UNIT_DST}"
# Preferencia previa: no reactivar si el usuario deshabilitó o enmascaró.
PREV_ENABLE=""
if command -v systemctl >/dev/null 2>&1; then
  PREV_ENABLE=$(systemctl --user is-enabled keybackcon.service 2>/dev/null || true)
fi
HAD_UNIT=0
[ -f "${UNIT_DST}/keybackcon.service" ] && HAD_UNIT=1
# Las unidades del paquete apuntan a /usr/bin; la instalación local las
# reescribe a ~/.local/bin (mismo binario que acaba de instalarse arriba).
sed 's|/usr/bin/keybackcon|%h/.local/bin/keybackcon|g' \
  "${REPO_DIR}/packaging/systemd/keybackcon.service" \
  > "${UNIT_DST}/keybackcon.service"
sed 's|/usr/bin/keybackcon|%h/.local/bin/keybackcon|g' \
  "${REPO_DIR}/packaging/systemd/keybackcon-animation@.service" \
  > "${UNIT_DST}/keybackcon-animation@.service"
chmod 644 "${UNIT_DST}/keybackcon.service" "${UNIT_DST}/keybackcon-animation@.service"
if command -v systemctl >/dev/null 2>&1; then
  systemctl --user daemon-reload || true
  case "${PREV_ENABLE}" in
    disabled|masked)
      echo "==> keybackcon: restauración ${PREV_ENABLE}; no se reactiva (H-26/H-27)"
      ;;
    enabled|enabled-runtime)
      ;;
    *)
      if [ "${HAD_UNIT}" = "0" ] || [ "${KEYBACKCON_ENABLE_RESTORE:-0}" = "1" ]; then
        systemctl --user enable keybackcon.service || true
      fi
      ;;
  esac
fi

if [ "${KEYBACKCON_SKIP_UDEV:-0}" = "1" ]; then
  echo "==> keybackcon: regla udev omitida (KEYBACKCON_SKIP_UDEV=1)"
elif [ -f /etc/udev/rules.d/70-keybackcon.rules ]; then
  echo "==> keybackcon: regla udev ya presente, nada que hacer"
else
  echo "==> keybackcon: instalar regla udev (pide sudo)"
  sudo install -m644 "${REPO_DIR}/packaging/udev/70-keybackcon.rules" /etc/udev/rules.d/70-keybackcon.rules
  sudo udevadm control --reload
  sudo udevadm trigger --subsystem-match=hidraw || true
fi

echo "==> keybackcon: migrar estado heredado kbd-rgb (si existe)"
for d in "${XDG_STATE_HOME:-${HOME}/.local/state}" "${HOME}/.local/state"; do
  if [ -f "$d/kbd-rgb/state" ] && [ ! -f "$d/keybackcon/state" ]; then
    mkdir -p "$d/keybackcon"
    cp "$d/kbd-rgb/state" "$d/keybackcon/state"
    echo "    migrado $d/kbd-rgb/state -> $d/keybackcon/state"
    break
  fi
done

echo "==> keybackcon: comprobar dependencias de la GUI y la bandeja"
tray_ok=1
python3 -c 'import gi; gi.require_version("Gtk", "3.0"); gi.require_version("AyatanaAppIndicator3", "0.1"); from gi.repository import AyatanaAppIndicator3; import cairo' >/dev/null 2>&1 || tray_ok=0
gui_ok=1
python3 -c 'import gi; gi.require_version("Gtk", "4.0"); gi.require_version("Adw", "1"); from gi.repository import Gtk, Adw' >/dev/null 2>&1 || gui_ok=0
if [ "$gui_ok" = 0 ]; then
  echo "    aviso: la ventana no arrancará hasta instalar sus dependencias" >&2
  case "$(pkg_manager)" in
    pacman) echo "    pista: sudo pacman -S python-gobject gtk4 libadwaita" >&2 ;;
    dnf)    echo "    pista: sudo dnf install python3-gobject gtk4 libadwaita" >&2 ;;
    zypper) echo "    pista: sudo zypper install python3-gobject gtk4 libadwaita" >&2 ;;
    *)      echo "    pista: sudo apt install python3-gi gir1.2-gtk-4.0 gir1.2-adw-1" >&2 ;;
  esac
fi
if [ "$tray_ok" = 0 ]; then
  echo "    aviso: la bandeja (--tray) no estará disponible hasta instalar sus dependencias" >&2
  echo "    pista: $(pkg_hint tray)" >&2
fi

echo "OK: keybackcon instalado. Prueba con: keybackcon info"
