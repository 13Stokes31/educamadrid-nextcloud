# Nube Educamadrid (`educamadrid-nextcloud`)

Pequeña GUI que automatiza el alta de la cuenta Nextcloud corporativa de Educamadrid
(`https://cloud.educa.madrid.org/`) en el cliente de escritorio, sin tener que rellenar
a mano el asistente de Nextcloud ni buscar dónde guarda la contraseña.

> **Solo para KDE Plasma** (uso personal). La contraseña se guarda en KWallet y el marcador
> en Dolphin; no hay soporte para GNOME/libsecret.

## Qué hace

1. Al pulsar **Conectar** abre el navegador contra el flujo de login **Nextcloud Login
   Flow v2** (`/index.php/login/v2`) y sondea el endpoint de confirmación hasta que
   inicias sesión (timeout de 5 minutos). Mientras esperas, el botón **Cancelar** vuelve
   a la pantalla inicial sin tocar nada. Un fallo de red puntual no aborta el sondeo: se
   reintenta, y solo se da por perdido tras 5 errores de red seguidos.
2. Con las credenciales devueltas, primero **se cierra el cliente Nextcloud si está abierto**
   (`nextcloud --quit`, espera hasta 15 s; si no cierra, SIGTERM y 5 s más; si aun así sigue,
   da error sin haber tocado nada). Así el cliente no pisa la configuración al salir. Después:
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

## Instalación

### Arch Linux (AUR)

```bash
yay -S educamadrid-nextcloud-bin   # binario precompilado
yay -S educamadrid-nextcloud       # compila desde el código fuente
```

Son incompatibles entre sí: instala solo uno. El lanzador «Nube Educamadrid» aparece en el
menú de aplicaciones.

## Compilar

```bash
cargo build --release
```

Salida: `target/release/educamadrid-nextcloud`.

## Requisitos del sistema

- **KDE Plasma** (KWallet y Dolphin).
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

## Publicar una versión (mantenimiento)

1. Subir `version` en `Cargo.toml` y `pkgver` en `packaging/aur/PKGBUILD` y
   `packaging/aur-bin/PKGBUILD`; poner `pkgrel=1` en ambos paquetes.
2. Ejecutar `cargo test && cargo clippy --all-targets -- -D warnings`, hacer commit y subirlo a
   `main`.
3. Crear y subir la etiqueta: `git tag vX.Y.Z && git push origin main vX.Y.Z`. Crear después
   la *release* de GitHub para esa etiqueta y publicarla. Al publicarse, GitHub Actions ejecuta
   de nuevo los tests, compila el binario Linux x86_64 desde esa etiqueta y adjunta a la release
   `educamadrid-nextcloud-vX.Y.Z-x86_64-linux-gnu.tar.gz` y su fichero `.sha256`.
4. Para `educamadrid-nextcloud`, en `packaging/aur/` ejecutar
   `updpkgsums && makepkg -f && makepkg --printsrcinfo > .SRCINFO`. Se mantiene `check()` en el
   `PKGBUILD`, por lo que `makepkg` ejecuta también los tests.
5. Para `educamadrid-nextcloud-bin`, copiar el SHA-256 del artefacto publicado en la release al
   `sha256sums` de `packaging/aur-bin/PKGBUILD` y ejecutar
   `makepkg -f && makepkg --printsrcinfo > .SRCINFO`.
6. Copiar `PKGBUILD` y `.SRCINFO` de cada variante a sus respectivos repositorios AUR y hacer
   commit + push:
   - `ssh://aur@aur.archlinux.org/educamadrid-nextcloud.git`
   - `ssh://aur@aur.archlinux.org/educamadrid-nextcloud-bin.git`

El workflow también admite ejecución manual indicando una etiqueta ya existente. La reejecución
solo completa artefactos ausentes o acepta los que tengan exactamente el mismo SHA-256; nunca
sustituye un artefacto publicado por contenido diferente.

## Documentos del proyecto

- `ROADMAP.md`: problemas conocidos y mejoras pendientes.
- `TESTING.md`: comprobaciones manuales pendientes.

## Licencia

MIT. Ver `LICENSE`.
