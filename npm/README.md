# keybackcon (npm) — solo CLI

Keyboard Backlight Controls — control RGB del teclado AERO X16 vía HID
LampArray. Este paquete instala el **binario precompilado** (Linux
x86_64/arm64). **No** incluye la mesa de luz GTK ni sus recursos.

```sh
npm install -g keybackcon
```

El postinstalador, si el entorno lo permite:

- instala la regla udev de acceso al hidraw (vía `sudo`/`pkexec`; se omite con
  `KEYBACKCON_SKIP_UDEV=1` o si ya existe);
- instala unidades systemd de usuario y ejecuta `daemon-reload`.
  **No** habilita la restauración si el usuario la desactivó en el estado
  persistido (`restore --on-login`).

Para la GUI: instala el `.deb` de la
[release](https://github.com/Soluciones-Alexendros/keybackcon/releases) o usa
`./scripts/install.sh` (requiere GTK 4 y libadwaita ≥ 1.5).

Fuente y documentación: https://github.com/Soluciones-Alexendros/keybackcon
