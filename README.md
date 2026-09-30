# Nube Educamadrid (`educamadrid-nextcloud`)

Pequeña GUI que automatiza el alta de la cuenta Nextcloud corporativa de Educamadrid
(`https://cloud.educa.madrid.org/`) en el cliente de escritorio, sin tener que rellenar
a mano el asistente de Nextcloud ni buscar dónde guarda la contraseña.

## Qué hace

1. Al pulsar **Conectar** abre el navegador contra el flujo de login **Nextcloud Login
   Flow v2** (`/index.php/login/v2`) y sondea el endpoint de confirmación hasta que
   inicias sesión (timeout de 5 minutos).
2. Con las credenciales devueltas:
   - Crea la carpeta de sincronización `~/Cloud - <usuario>`.
   - Escribe/actualiza `~/.config/Nextcloud/nextcloud.cfg` añadiendo la cuenta (si el
     usuario+servidor ya existían, no toca nada; si no, añade un índice nuevo sin
     afectar a las cuentas que ya tuvieras configuradas).
   - Guarda la contraseña de aplicación en **KWallet vía D-Bus** (detecta si el sistema
     usa `kwalletd6` o `kwalletd5`), dentro del monedero de red configurado y con la
     misma clave que construye el cliente Nextcloud.
   - Añade un marcador "Cloud - `<usuario>`" en Dolphin (`user-places.xbel`).
   - Lanza el cliente `nextcloud`.

> **Seguridad:** la contraseña de aplicación se pasa dentro de la llamada D-Bus a
> KWallet, nunca como argumento de un proceso — no aparece en `ps` / `/proc/<pid>/cmdline`.
> En el despliegue previsto, el monedero KWallet tiene una contraseña maestra vacía:
> esto evita cualquier solicitud de desbloqueo, pero significa que la credencial no
> tiene protección criptográfica efectiva en disco. Se mantienen los permisos del
> archivo y los controles de acceso de KWallet durante la sesión.

## Compilar

```bash
cargo build --release
```

Salida: `target/release/educamadrid-nextcloud`.

## Requisitos del sistema

- Cliente **Nextcloud** instalado (binario `nextcloud` en el `PATH`).
- **KWallet** (`kwalletd5` o `kwalletd6`) disponible en el bus de sesión D-Bus, con
  un monedero de red configurado. Puede tener una contraseña maestra vacía.
- **Dolphin** (o cualquier gestor de archivos KDE que lea `user-places.xbel`) si quieres
  que aparezca el marcador; si no existe el fichero, la app lo crea.
- `xdg-open` para abrir el navegador del flujo de login.

## Uso

```bash
./target/release/educamadrid-nextcloud
```

Pulsas **Conectar**, inicias sesión en el navegador con tu cuenta `@educa.madrid.org` y
la app deja todo listo y arranca el cliente Nextcloud.

## Archivos / inputs

No requiere ningún archivo externo. Toda la configuración va por la GUI y por lo que
devuelve el propio flujo de login de Nextcloud.

## Notas

- Si ya tienes una cuenta de este mismo servidor configurada manualmente, comprueba que
  el usuario coincida exactamente con lo que ya hay en `nextcloud.cfg`. La URL se
  compara ignorando la barra final; las mayúsculas y minúsculas del usuario siguen
  siendo significativas.
- `nextcloud.cfg` es un formato interno del cliente. La aplicación conserva las cuentas
  existentes y reemplaza el archivo atómicamente, pero debe probarse con cada versión de
  Nextcloud Desktop que se vaya a desplegar.
