# Contrato funcional de estado y sesión

### Propósito de este documento

- **Objetivos:** Fijar el comportamiento esperado de brillo, apagado,
  restauración, consultas y clientes (CLI, ventana, bandeja).
- **Estructura:** Preferencias vs aplicación → comandos → sesión → JSON.
- **Contenido a integrar según contexto:** Cualquier cambio de semántica
  exige actualizar este archivo y las pruebas asociadas.

## Distinciones

| Concepto | Significado |
|---|---|
| Preferencia | Color base y brillo que el usuario quiere conservar. |
| Último aplicado | Cambio confirmado por el backend tras escritura HID. |
| Pendiente | Cambio solicitado en la GUI aún no confirmado. |
| Modo | `fijar`, animación (`breathe`/`rainbow`), `off`, efectos de firmware. |
| Lectura física | El hardware no ofrece una medición de color fiable vía este producto; no se presenta la preferencia como medición. |

## Comandos y efectos

| Acción | Efecto sobre el teclado | Efecto sobre preferencias / modo |
|---|---|---|
| `set` | Escribe color escalado; detiene animación si había. | Actualiza color base persistido. |
| `brightness` | Escala el RGB del color base. **No detiene** la animación (ADR 4: la animación relee el porcentaje). | Actualiza el porcentaje persistido. |
| `off` | Apaga la luz. | Conserva color y brillo; el modo pasa a apagado y debe ser visible a otros clientes. |
| `stop` | Detiene la animación y restaura el color base. | Modo vuelve a fijar. |
| `animation` | Un solo escritor; reclama exclusión antes de abrir el HID. | Modo de animación activo. |
| `firmware-effects` | Activa o desactiva autónomos del firmware. | Publica el modo correspondiente. |
| `restore` | Reaplica preferencia solo si la política de restauración está activa. | No reescribe la preferencia. |
| `info` / `info --json` | Consulta de solo lectura. | Los campos de color/brillo son **preferencia persistida**, no medición física. |

## Interfaz

- Abrir, presentar o consultar la ventana **no** escribe en el HID.
- Un cambio visual pendiente no se anuncia como aplicado.
- Si la aplicación falla, se revierte o se marca el error; no se finge éxito.
- La bandeja no inventa el modo `breathe` ante una animación desconocida.

## Sesión y preferencias

- Restauración al login solo según preferencia explícita del usuario.
- Actualizar o reinstalar **no** reactiva una restauración que el usuario
  desactivó.
- Preferencias de interfaz y sesión no dependen del canal de instalación
  (`.deb`, script local, npm CLI).

## Exclusión de escritura

Contrato que cierra la enmienda del ADR 13. El árbol anterior solo tomaba
el `flock` dentro de la animación; hasta que el código lo cumpla, esta
sección es el requisito, no una descripción del binario ya publicado.

Una sola escritura HID a la vez entre los procesos del producto que toman
el candado (`/run/keybackcon/writer.lock`). `info` no escribe y no necesita
el candado. Si el candado está ocupado o la ruta no es segura, el comando
falla y no abre el dispositivo. Cerrar la interfaz no arranca una animación
que aún estuviera en cola.

La preferencia de restauración que lee el CLI es `restore_policy` del
estado versionado. Un interruptor de la GUI que no escriba esa política
no cuenta como desactivación.

## Regresiones que acreditan este contrato

| Hallazgo | Prueba | Qué debe observarse |
|---|---|---|
| KC-01, KC-02 | T-01, T-04 | Dos escrituras no se solapan. T-04 es manual (cambio de usuario). |
| KC-01 | T-02, T-03 | No hay hueco entre «no hay animación» y el nuevo escritor. Una animación viva rechaza la escritura nueva. |
| KC-03, KC-04 | T-05, T-06 | Directorio ajeno o demasiado abierto, y un enlace en el candado, se rechazan. |
| KC-05 | T-07 | Sin entorno XDG válido no aparece un fallback compartido. |
| KC-08, KC-09 | T-08, T-09 | Dos actualizaciones de estado no se pierden. Un esquema futuro no se reescribe. |
| KC-06, KC-07 | T-10, T-11, T-12 | `restore --on-login` obedece `restore_policy=never`. Una animación que no arranca no queda anunciada. La salida vieja no pisa el modo nuevo. |
| KC-11–KC-16 | T-13–T-16 | La UI no se bloquea con el backend ocupado. El color fallido se revierte. `close` no ejecuta la animación pendiente. |

El detalle de cada caso vive en el plan de estabilización. Esta tabla solo
impide dar por cerrado un comportamiento sin la prueba nombrada.

## Códigos de salida (CLI)

| Código | Uso |
|---|---|
| 0 | Éxito, incluida la ayuda solicitada (`--help` / `help`). |
| 1 | Error de ejecución o E/S. |
| 2 | Uso incorrecto (argumentos inválidos). |
