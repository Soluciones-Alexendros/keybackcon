# Protocolo

USB HID Usage Tables v1.4, §26 "Lighting And Illumination Page" (Usage Page
`0x59`). Sin ingeniería inversa: el firmware expone un LampArray estándar.
Fuente única de valores en el código: `src/protocol.rs` (`src/lamp.rs` solo
los consume); mapa de uso en [`ARCHITECTURE.md`](../../ARCHITECTURE.md).

## Detección

El descriptor contiene la firma `05 59 09 01 A1 01` (Usage Page LampArray,
Usage LampArray, Collection Application), localizada vía la subruta sysfs
`device/report_descriptor`.

## Informes

| Informe | Dirección | Formato |
|---|---|---|
| 1 — atributos (`get_feature(1, 23)`) | Lectura | `LampCount` en bytes 1–2 (u16 LE, aquí: 1); `LampArrayKind` en bytes 15–18 (u32 LE, el firmware declara un valor engañoso: se muestra tal cual con aviso) |
| 5 — color (`set_feature`) | Escritura | `[5, 1, first_lo, first_hi, last_lo, last_hi, r, g, b, 0xff]` → color de la zona `first..=last` (aquí `0..=count-1`, una sola zona) |
| 6 — autónomo (`set_feature`) | Escritura | `[6, on]` → efectos autónomos del firmware on/off |

## Brillo

El dispositivo declara `IntensityLevelCount=1`: no hay canal de brillo
separable. `brightness N` guarda un porcentaje 0–100 y reemite
`scale(base, pct)` con redondeo (`(v*pct+50)/100`).

## Persistencia (fuera del cable)

El protocolo HID no define el fichero de estado. En disco, `src/state.rs`
persiste un esquema versionado (`schema_version=1`) con `base_color`,
`brightness`, `power`, `mode` y `restore_policy`. El formato plano
`"<hex> <pct>"` (p. ej. `ff7800 60`) solo se lee como legado. Detalle de
sesión y clientes: [`docs/product/contrato.md`](../../product/contrato.md)
y [`ARCHITECTURE.md`](../../ARCHITECTURE.md).
