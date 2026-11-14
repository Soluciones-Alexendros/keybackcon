# Receta para `omacom/omarchy-pkgs`

Copia de la receta que se publica en el repositorio de paquetes de Omarchy.
Vive aquí para que el empaquetado de Arch viaje con el producto y para que
cualquier cambio de rutas o de unidades rompa los tests en el mismo PR.

```
ilutex/
├── PKGBUILD              Receta; construye desde el tarball del tag
├── test.sh               Layout, udev, candado, unidades, .desktop, esquema
├── README.package.md     Lo que lee quien mantiene el paquete
└── .omarchy/
    └── package.json      `source: local`, upstream watch de GitHub Releases
```

Para publicar:

```bash
# 1. Tag en el repo del producto (vX.Y.Z). El release construye .deb, tarball y SBOM.
# 2. Rellenar el checksum del tarball del tag.
updpkgsums

# 3. Validar con las herramientas del repo destino.
python3 helpers/upstream-watch.py validate pkgbuilds/ilutex
bash tests/pinned-sources.sh
bash tests/package-metadata.sh
bash tests/pkgname-unique.sh
makepkg --verifysource          # en el directorio de la receta
./test.sh                       # layout y contratos

# 4. PR a omacom/omarchy-pkgs desde un fork.
```

`sha256sums` lleva `SKIP` hasta que exista el tag: es el único hueco de la
receta y lo rellena `updpkgsums`. Un PR con `SKIP` no publica.

Sin guion de instalación a propósito: pacman ya dispara `systemd-sysusers`,
`systemd-tmpfiles` y la recarga de udev con lo que el paquete deja en
`/usr/lib`.
