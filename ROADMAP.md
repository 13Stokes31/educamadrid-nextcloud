# ROADMAP

Registro único de problemas conocidos y mejoras pendientes. Lo ya hecho que falta comprobar a
mano está en `TESTING.md`.

Origen: auditoría externa del 2026-08-29 (antiguo `CHATGPT.md`). Resueltos en 0.3.0 y después,
sin más pendientes: comprobación del resultado de `writePassword`, cuentas detectadas por
índice, validación de datos del servidor, escritura atómica, pruebas unitarias, CI, cancelación
y reintentos de red en el sondeo, y cierre del cliente Nextcloud antes de configurar.

## Publicación

v0.3.0 publicada el 2026-09-30: repo público, release con binario y paquetes AUR
`educamadrid-nextcloud` y `educamadrid-nextcloud-bin`. Queda la casilla de `yay -S` en `TESTING.md`.

## Robustez

### Revisar la rama `fix/nextcloud-34-compat`
- **Qué:** rama en GitHub con 16 commits del 2026-09-18 que nunca se fusionaron en `main`
  (más tiempo para el SSO de EducaMadrid, rechazar carpetas de sincronización no vacías en
  cuentas nuevas, activar el backend de KWallet configurado, workflows de binarios
  EndeavourOS/portable, entre otros). Parte ya está cubierta por 0.3.0 por otra vía.
- **Por qué importa:** puede traer arreglos que `main` no tiene; si no, conviene borrarla.
- **Estado:** PENDIENTE de revisar commit a commit. No se fusionó al publicar 0.3.0 para no
  cambiar el código ya probado.

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
