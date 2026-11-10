# Runbook: dispositivo HID y udev

| Síntoma | Comprobación | Acción |
| --- | --- | --- |
| No encuentra el teclado | `keybackcon info`; USB conectado; descriptor `05 59 09 01 A1 01` | Revisa el cable; solo AERO X16 (LampArray). Spec en [`../architecture/protocol.md`](../architecture/protocol.md). |
| Permiso denegado en `/dev/hidraw*` | `ls -l /dev/hidraw*`; regla `packaging/udev/70-keybackcon.rules` | `sudo udevadm trigger` y **vuelve a iniciar sesión**. El paquete instala la regla; `install.sh` también. |
| La animación no para | Pidfile `$XDG_RUNTIME_DIR/keybackcon/animation.pid` | `keybackcon stop`. Si el pidfile está huérfano, el CLI lo reclama (ADR 3). |
| «ya hay una escritura» | `/run/keybackcon/writer.lock` y grupo `keybackcon` | Otro comando del producto tiene el candado. Si el directorio no existe, reinstala el paquete (`tmpfiles.d` / `sysusers.d`) y entra en una sesión donde el grupo ya esté activo. |
| Color no sobrevive al reinicio | Unidad `keybackcon.service` (usuario) | El `.deb`/AUR apunta a `/usr/bin/keybackcon restore`. Tras `install.sh`, las unidades usan `~/.local/bin`. |

La GUI nunca toca hidraw: habla con el CLI por subproceso (ADR 9).
