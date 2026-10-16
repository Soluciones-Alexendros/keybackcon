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

## Códigos de salida (CLI)

| Código | Uso |
|---|---|
| 0 | Éxito, incluida la ayuda solicitada (`--help` / `help`). |
| 1 | Error de ejecución o E/S. |
| 2 | Uso incorrecto (argumentos inválidos). |
