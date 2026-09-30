# ROADMAP

Registro único de problemas conocidos y mejoras pendientes. Lo ya hecho que falta comprobar a
mano está en `TESTING.md`.

Origen: auditoría externa del 2026-08-29 (antiguo `CHATGPT.md`). Resueltos en 0.3.0 y después,
sin más pendientes: comprobación del resultado de `writePassword`, cuentas detectadas por
índice, validación de datos del servidor, escritura atómica, pruebas unitarias, CI, cancelación
y reintentos de red en el sondeo, y cierre del cliente Nextcloud antes de configurar.

## Publicación

### Publicar v0.3.0 en GitHub y en AUR
- **Qué:** hacer público el repo `13Stokes31/educamadrid-nextcloud`, crear la etiqueta y la
  release `v0.3.0` y subir los paquetes `educamadrid-nextcloud` y `-bin` al AUR.
- **Por qué:** los ficheros de empaquetado ya están; los `sha256sums` están en `SKIP` hasta
  que exista la release.
- **Estado:** PENDIENTE, tras la prueba real (ver `TESTING.md`). Pasos en el README, «Publicar
  una versión».

## Robustez

### El alta no es transaccional
- **Qué:** el orden es carpeta, `nextcloud.cfg`, KWallet, marcador de Dolphin y arranque. Si falla
  un paso intermedio, los anteriores quedan aplicados y la interfaz solo muestra un error general.
- **Por qué importa:** puede quedar una cuenta en el cfg sin contraseña en KWallet. Reintentar es
  idempotente (no duplica cuenta ni marcador), así que el daño práctico es pequeño.
- **Estado:** baja prioridad, por decidir (revertir lo creado o dejarlo como está).

### Formato frágil de `nextcloud.cfg`
- **Qué:** se edita a mano un formato interno del cliente (`version=13`, carpeta `version=2`) y
  el parseo es propio, sin cubrir comentarios ni escapes Qt INI.
- **Por qué importa:** una versión futura del cliente podría cambiar el esquema y dejar una
  configuración inválida. No se conoce una API o CLI soportada para dar de alta cuentas.
- **Estado:** vigilar al actualizar `nextcloud-client` (la prueba de `TESTING.md` lo cubre).

### Contraseña en memoria
- **Qué:** `app_password` vive en varios `String` durante el alta y no se sobrescribe al liberarse.
- **Cómo:** `zeroize`, que añade una dependencia por una ganancia pequeña.
- **Estado:** baja prioridad, por decidir.

## Descartado por ahora

- **Soporte de GNOME / libsecret:** descartado por ahora, uso solo en Plasma.
