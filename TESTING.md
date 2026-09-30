# TESTING — comprobaciones manuales

Lo que ya está hecho pero hay que verificar a mano (no lo cubre `cargo test`). Marcar `[x]` al
comprobarlo. Hacerlo en el PC de pruebas con KDE Plasma, nunca en una cuenta Nextcloud en uso
sin copia de `~/.config/Nextcloud/nextcloud.cfg`.

## Alta de la cuenta

- [x] **Usuario sin `nextcloud.cfg`:** mover `~/.config/Nextcloud` a un lado, abrir la app,
      pulsar Conectar, iniciar sesión en el navegador. Mensaje verde, se crea `~/Cloud - usuario`
      y se abre el cliente con la cuenta.
- [ ] **Con otra cuenta ya en el cfg:** con un `nextcloud.cfg` que tenga otra cuenta, hacer el alta.
      La cuenta anterior se conserva y la nueva aparece con el siguiente índice.
- [x] **Repetir el alta:** volver a pulsar Conectar con el mismo usuario. No se duplica la cuenta en
      el cfg (`grep -c dav_user ~/.config/Nextcloud/nextcloud.cfg`) ni el marcador de Dolphin.
- [x] **Cliente abierto:** con el cliente Nextcloud en marcha, hacer el alta. Se cierra solo y vuelve
      a abrirse con la cuenta nueva, sin pisar la configuración.
- [x] **Sin contraseña:** tras el alta, el cliente NO pide contraseña (KWallet correcto).
- [x] **Carpetas grandes:** en un alta nueva, las carpetas de más de 500 MB (p. ej. `INSTITUTO`)
      se sincronizan sin pedir confirmación; en `nextcloud.cfg`, `[General]` tiene
      `useNewBigFolderSizeLimit=false`.
- [x] **Marcador:** en Dolphin aparece «Cloud - usuario» en Lugares, con icono de nube.

## Espera y red

- [x] **Cancelar:** pulsar Conectar y, sin iniciar sesión, pulsar Cancelar. Vuelve a la pantalla
      inicial al instante. Iniciar sesión después en la pestaña abierta no cambia nada en
      `~/.config/Nextcloud` ni en Dolphin.
- [x] **Corte de red:** durante la espera, desconectar la red unos segundos (p. ej. 10 s) y volver
      a conectarla. La app sigue esperando y el alta termina bien.

## Paquete

- [x] **Lanzador:** tras instalar el paquete, «Nube Educamadrid» aparece en el menú con icono de
      nube y la ventana se agrupa bien en la barra de tareas.
- [ ] `yay -S educamadrid-nextcloud-bin` (tras publicar) instala y abre la app.
