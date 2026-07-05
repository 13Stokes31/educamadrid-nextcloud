# Nube Educamadrid (`nextcloud-educamadrid`)

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
     usa `kwalletd6` o `kwalletd5`), con la misma clave (`usuario:servidor`) que busca el
     cliente Nextcloud.
   - Añade un marcador "Cloud - `<usuario>`" en Dolphin (`user-places.xbel`).
   - Lanza el cliente `nextcloud`.

> **Seguridad:** la contraseña de aplicación se pasa dentro de la llamada D-Bus a
> KWallet, nunca como argumento de un proceso — no aparece en `ps` / `/proc/<pid>/cmdline`.

## Compilar

```bash
cargo build --release
```

Salida: `target/release/nextcloud-educamadrid`.

## Requisitos del sistema

- Cliente **Nextcloud** instalado (binario `nextcloud` en el `PATH`).
- **KWallet** (`kwalletd5` o `kwalletd6`) corriendo en el bus de sesión D-Bus.
- **Dolphin** (o cualquier gestor de archivos KDE que lea `user-places.xbel`) si quieres
  que aparezca el marcador; si no existe el fichero, la app lo crea.
- `xdg-open` para abrir el navegador del flujo de login.

## Uso

```bash
./target/release/nextcloud-educamadrid
```

Pulsas **Conectar**, inicias sesión en el navegador con tu cuenta `@educa.madrid.org` y
la app deja todo listo y arranca el cliente Nextcloud.

## Archivos / inputs

No requiere ningún archivo externo. Toda la configuración va por la GUI y por lo que
devuelve el propio flujo de login de Nextcloud.

## Notas

- Si ya tienes una cuenta de este mismo servidor configurada manualmente, comprueba que
  el usuario y la URL coincidan exactamente con lo que ya hay en `nextcloud.cfg` antes de
  usar la app — la detección de duplicados compara cadenas literales (`dav_user=` y
  `url=`), así que una URL con barra final distinta o un usuario con mayúsculas distintas
  crearía una cuenta nueva en vez de reconocer la existente.
