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

echo "==> ilutex: compilar (release)"
cargo build --release --manifest-path "${REPO_DIR}/Cargo.toml"

echo "==> ilutex: instalar binario en ${BIN_DST}/ilutex"
mkdir -p "${BIN_DST}"
install -m755 "${REPO_DIR}/target/release/ilutex" "${BIN_DST}/ilutex"

echo "==> ilutex: instalar GUI en ${BIN_DST}/ilutex-gui"
GUI_DST="${HOME}/.local/share/ilutex/gui"
mkdir -p "${GUI_DST}"
install -m644 "${REPO_DIR}"/gui/*.py "${GUI_DST}/"
install -m644 "${REPO_DIR}/packaging/udev/70-ilutex.rules" "${HOME}/.local/share/ilutex/70-ilutex.rules"
install -m755 "${REPO_DIR}/packaging/ilutex-gui" "${BIN_DST}/ilutex-gui"

echo "==> ilutex: compilar traducciones (es, en)"
MO_SRC_DIR="${REPO_DIR}/po"
if command -v msgfmt >/dev/null 2>&1; then
  for MO_SRC in "${MO_SRC_DIR}"/*.po; do
    [ -f "${MO_SRC}" ] || continue
    LANG_CODE="$(basename "${MO_SRC}" .po)"
    MO_BUILD="${REPO_DIR}/gui/locale/${LANG_CODE}/LC_MESSAGES/ilutex.mo"
    MO_DST="${HOME}/.local/share/locale/${LANG_CODE}/LC_MESSAGES/ilutex.mo"
    mkdir -p "$(dirname "${MO_BUILD}")"
    msgfmt -o "${MO_BUILD}" "${MO_SRC}"
    echo "    .mo generado en ${MO_BUILD}"
    mkdir -p "$(dirname "${MO_DST}")"
    install -m644 "${MO_BUILD}" "${MO_DST}"
    echo "    locale instalado en ${MO_DST}"
  done
else
  echo "    aviso: msgfmt no disponible, se omite la compilación de los .mo (la GUI usará el idioma por defecto)" >&2
  echo "    pista: $(pkg_hint gettext)" >&2
fi

echo "==> ilutex: instalar schema GSettings"
SCHEMA_SRC="${REPO_DIR}/gui/org.alexendros.ilutex.gschema.xml"
SCHEMA_DST_DIR="${HOME}/.local/share/glib-2.0/schemas"
mkdir -p "${SCHEMA_DST_DIR}"
install -m644 "${SCHEMA_SRC}" "${SCHEMA_DST_DIR}/org.alexendros.ilutex.gschema.xml"
if command -v glib-compile-schemas >/dev/null 2>&1; then
  glib-compile-schemas "${SCHEMA_DST_DIR}" || true
else
  echo "    aviso: glib-compile-schemas no disponible, se omite la compilación de schemas" >&2
  echo "    pista: $(pkg_hint glib)" >&2
fi
# Nota: sin autostart global. El autostart es opt-in desde el diálogo de
# preferencias (gui/settings.py), que crea ~/.config/autostart/ilutex-tray.desktop bajo demanda.

echo "==> ilutex: instalar lanzador + icono"
mkdir -p "${DESK_DST}" "${ICON_DST}"
install -m644 "${REPO_DIR}/packaging/desktop/ilutex.desktop" "${DESK_DST}/ilutex.desktop"
install -m644 "${REPO_DIR}/assets/icons/ilutex.svg" "${ICON_DST}/ilutex.svg"

echo "==> ilutex: instalar unidades systemd de usuario"
mkdir -p "${UNIT_DST}"
for unit in "${REPO_DIR}/packaging/systemd"/*.service; do
  [ -f "$unit" ] || continue
  sed "s|__ILUTEX_BIN__|${BIN_DST}/ilutex|g" "$unit" > "${UNIT_DST}/$(basename "$unit")"
done
systemctl --user daemon-reload 2>/dev/null || true

# Restauración al login: solo si la unidad es nueva o se fuerza (ILUTEX_ENABLE_RESTORE=1).
prev_enable="$(systemctl --user is-enabled ilutex.service 2>/dev/null || true)"
had_unit=false
[ -f "${UNIT_DST}/ilutex.service" ] && had_unit=true
if [ "$prev_enable" = "disabled" ] || [ "$prev_enable" = "masked" ]; then
  echo "==> ilutex: restauración ${prev_enable}; no se reactiva"
elif [ "$prev_enable" = "enabled" ] || [ "$prev_enable" = "enabled-runtime" ]; then
  : # ya activa
else
  force_enable="${ILUTEX_ENABLE_RESTORE:-0}"
  if [ "$had_unit" = false ] || [ "$force_enable" = 1 ]; then
    systemctl --user enable ilutex.service
    echo "==> ilutex: restauración al login habilitada (systemctl --user enable ilutex.service)"
  fi
fi

echo "==> ilutex: regla udev (opcional, root)"
if [ "${ILUTEX_SKIP_UDEV:-0}" = "1" ]; then
  echo "    omitida (ILUTEX_SKIP_UDEV=1)"
else
  udev_ok=1
  if ! sudo install -D -m644 "${REPO_DIR}/packaging/udev/70-ilutex.rules" /etc/udev/rules.d/70-ilutex.rules; then
    echo "    error: no se pudo instalar udev/70-ilutex.rules" >&2
    udev_ok=0
  fi
  if ! sudo udevadm control --reload; then
    echo "    error: udevadm control --reload falló" >&2
    udev_ok=0
  fi
  if ! sudo udevadm trigger --subsystem-match=hidraw; then
    echo "    error: udevadm trigger falló" >&2
    udev_ok=0
  fi
  if [ "$udev_ok" = 1 ]; then
    echo "==> ilutex: regla udev instalada (root)"
  else
    echo "==> ilutex: regla udev incompleta" >&2
  fi
fi

echo "==> ilutex: candado de escritura (root)"
writer_lock_ok=1
if ! sudo install -D -m644 "${REPO_DIR}/packaging/sysusers.d/ilutex.conf" /usr/lib/sysusers.d/ilutex.conf; then
  echo "    error: no se pudo instalar sysusers.d/ilutex.conf" >&2
  writer_lock_ok=0
fi
if ! sudo install -D -m644 "${REPO_DIR}/packaging/tmpfiles.d/ilutex.conf" /usr/lib/tmpfiles.d/ilutex.conf; then
  echo "    error: no se pudo instalar tmpfiles.d/ilutex.conf" >&2
  writer_lock_ok=0
fi
if command -v systemd-sysusers >/dev/null 2>&1; then
  if ! sudo systemd-sysusers ilutex.conf; then
    echo "    error: systemd-sysusers ilutex.conf falló" >&2
    writer_lock_ok=0
  fi
else
  echo "    aviso: systemd-sysusers no disponible; crea el grupo ilutex a mano" >&2
  writer_lock_ok=0
fi
if command -v systemd-tmpfiles >/dev/null 2>&1; then
  if ! sudo systemd-tmpfiles --create ilutex.conf; then
    echo "    error: systemd-tmpfiles --create ilutex.conf falló" >&2
    writer_lock_ok=0
  fi
else
  echo "    aviso: systemd-tmpfiles no disponible; crea /run/ilutex a mano" >&2
  writer_lock_ok=0
fi
if [ ! -d /run/ilutex ] || [ ! -e /run/ilutex/writer.lock ]; then
  echo "    error: falta /run/ilutex/writer.lock tras tmpfiles" >&2
  writer_lock_ok=0
fi
if [ "$writer_lock_ok" = 1 ]; then
  echo "==> ilutex: candado instalado (root:ilutex 0660; dir 0770)"
else
  echo "==> ilutex: candado incompleto; sin él el CLI no escribe" >&2
fi
# Instalar ≠ autorizar: no añadimos usuarios al grupo automáticamente.
if id -nG "${USER}" 2>/dev/null | tr ' ' '\n' | grep -qx ilutex; then
  echo "==> ilutex: ${USER} ya está en el grupo ilutex"
else
  echo "==> ilutex: autorización de escritura pendiente"
  echo "    Instalar no te mete en el grupo. Para poder escribir:"
  echo "      sudo usermod -aG ilutex ${USER}"
  echo "    Luego cierra sesión y vuelve a entrar (o: newgrp ilutex)."
fi

echo "==> ilutex: migrar estado heredado kbd-rgb/keybackcon (si existe)"
for d in "${XDG_STATE_HOME:-${HOME}/.local/state}" "${HOME}/.local/state"; do
  # kbd-rgb -> ilutex
  if [ -f "$d/kbd-rgb/state" ] && [ ! -f "$d/ilutex/state" ]; then
    mkdir -p "$d/ilutex"
    cp "$d/kbd-rgb/state" "$d/ilutex/state"
    echo "    migrado $d/kbd-rgb/state -> $d/ilutex/state"
    break
  fi
  # keybackcon -> ilutex
  if [ -f "$d/keybackcon/state" ] && [ ! -f "$d/ilutex/state" ]; then
    mkdir -p "$d/ilutex"
    cp "$d/keybackcon/state" "$d/ilutex/state"
    echo "    migrado $d/keybackcon/state -> $d/ilutex/state"
    break
  fi
done

echo "==> ilutex: comprobar dependencias de la GUI y la bandeja"
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

if id -nG "${USER}" 2>/dev/null | tr ' ' '\n' | grep -qx ilutex; then
  echo "OK: ilutex instalado. Prueba con: ilutex info"
else
  echo "OK: ilutex instalado (binarios/udev/candado)."
  echo "    Escritura HID pendiente de autorización (grupo ilutex). Ver mensajes arriba."
  echo "    Consulta sin escribir: ilutex info"
fi