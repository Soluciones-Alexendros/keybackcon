# Keybackcon en Hyprland

### Propósito

Integración de sesión Wayland en Hyprland **sin** editar automáticamente
`hyprland.conf` ni Waybar. Copia solo lo que necesites.

Estado: **experimental** hasta completar los perfiles HY-A…HY-D con evidencias
registradas. Ver [`docs/compatibility/matriz.md`](../compatibility/matriz.md).

## Perfiles

| Perfil | Ventana | Bandeja | Arranque |
|---|---|---|---|
| HY-A · UWSM + Waybar/tray | Wayland nativo | Indicador | Un mecanismo de sesión |
| HY-B · sin UWSM + Waybar/tray | Wayland nativo | Indicador | Manual u opt-in documentado |
| HY-C · sin host de bandeja | Completa | No | Manual; aviso si falta bandeja |
| HY-D · sin agente polkit | Completa si hay permisos | Según host | Reparación de permisos explicada |

## Wayland

No fuerces `GDK_BACKEND=x11` en el uso normal. Identidad de aplicación:
`org.iniciativas.keybackcon` (no cambiar sin plan de migración).

```bash
keybackcon-gui
# o solo bandeja, si hay host StatusNotifier:
keybackcon-gui --tray
```

## Waybar (módulo tray)

El módulo `tray` de Waybar debe estar en **tu** configuración; no sustituimos
el archivo. Ejemplo a fusionar (ajusta a tu versión de Waybar):

```jsonc
"modules-right": ["tray", "clock"],
"tray": {
  "icon-size": 18,
  "spacing": 8
}
```

Si reinicias Waybar con Keybackcon ya en marcha, el indicador debe
re-registrarse; si no hay host, abre la ventana (`keybackcon-gui`).

## UWSM y sin UWSM

- **Con UWSM:** deja que gestione el autostart XDG; no actives a la vez la
  unidad de usuario, el autostart XDG y una entrada `exec-once` del compositor.
- **Sin UWSM:** arranque manual o una sola línea opt-in en tu `hyprland.conf`,
  por ejemplo (sintaxis según tu versión de Hyprland):

```conf
# Opt-in; no lo añadimos nosotros.
exec-once = keybackcon-gui --tray
```

Para restauración al login con systemd de usuario (si tu sesión lo expone):

```bash
systemctl --user enable --now keybackcon.service
```

Comprueba que `systemctl --user` habla con el gestor de **tu** sesión gráfica;
tener el binario `systemctl` no basta.

## Polkit

El control cotidiano no necesita root tras instalar la regla udev. Un agente
(`hyprpolkitagent` u otro) solo hace falta para la reparación gráfica de
permisos. Si cancelas la autorización, no hay éxito ficticio.

## Qué no hacemos

- Sobrescribir `DISPLAY`, `WAYLAND_DISPLAY` o `XDG_CURRENT_DESKTOP`.
- Instalar un segundo agente polkit si ya hay uno.
- Ampliar permisos a `0666` ni pedir ejecutar la GUI como root.
