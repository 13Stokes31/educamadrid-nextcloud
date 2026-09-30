use dbus::blocking::Connection;
use eframe::egui::{self, Color32, RichText};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

const SERVER_URL: &str = "https://cloud.educa.madrid.org/";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TIMEOUT: Duration = Duration::from_secs(300);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
// Errores de red seguidos tolerados en el sondeo antes de abortar.
const MAX_TRANSPORT_ERRORS: u32 = 5;
const CLIENT_COMM: &str = "nextcloud";
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);
const KILL_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_CHECK_INTERVAL: Duration = Duration::from_millis(250);
// QtKeychain usa Theme::appName() como carpeta e identificador de aplicación.
// En el cliente oficial sin personalizar ambos valores son "Nextcloud".
const NEXTCLOUD_KEYCHAIN_SERVICE: &str = "Nextcloud";

// ---------- Nextcloud Login Flow v2 ----------

#[derive(Deserialize)]
struct FlowResponse {
    poll: PollInfo,
    login: String,
}

#[derive(Deserialize)]
struct PollInfo {
    token: String,
    endpoint: String,
}

#[derive(Deserialize)]
struct LoginResult {
    server: String,
    #[serde(rename = "loginName")]
    login_name: String,
    #[serde(rename = "appPassword")]
    app_password: String,
}

// ---------- UI ----------

enum State {
    Ready,
    Waiting,
    Done(String),
    Error(String),
}

struct App {
    state: State,
    rx: Option<mpsc::Receiver<Result<String, String>>>,
    cancel: Arc<AtomicBool>,
    close_at: Option<Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: State::Ready,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            close_at: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let State::Waiting = &self.state {
            if let Some(rx) = &self.rx {
                match rx.try_recv() {
                    Ok(Ok(username)) => {
                        self.state = State::Done(username);
                        self.close_at = Some(Instant::now() + Duration::from_secs(3));
                        self.rx = None;
                    }
                    Ok(Err(e)) => {
                        self.state = State::Error(e);
                        self.rx = None;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint_after(Duration::from_millis(500));
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.state = State::Error("Hilo interrumpido".into());
                        self.rx = None;
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(RichText::new("Nube Colaborativa").size(22.0).strong());
                ui.label(RichText::new("Educamadrid").size(13.0).color(Color32::GRAY));
                ui.add_space(24.0);
                ui.separator();
                ui.add_space(18.0);

                match &self.state {
                    State::Ready => {
                        ui.label("Se abrirá el navegador para identificarte");
                        ui.label("con tu cuenta de Educamadrid.");
                        ui.add_space(16.0);
                        if ui
                            .button(RichText::new("  Conectar  ").size(15.0))
                            .clicked()
                        {
                            let (tx, rx) = mpsc::channel();
                            let cancel = Arc::new(AtomicBool::new(false));
                            self.rx = Some(rx);
                            self.cancel = cancel.clone();
                            self.state = State::Waiting;
                            std::thread::spawn(move || {
                                let result = run_login_flow(&cancel).and_then(|login| {
                                    // Si se canceló mientras terminaba el login, no se toca nada.
                                    if cancel.load(Ordering::Relaxed) {
                                        return Err("Cancelado".into());
                                    }
                                    let username = login.login_name.clone();
                                    close_nextcloud_client()?;
                                    // Cerrar el cliente puede tardar; se vuelve a mirar antes de escribir.
                                    if cancel.load(Ordering::Relaxed) {
                                        return Err("Cancelado".into());
                                    }
                                    setup_account(login)?;
                                    Ok(username)
                                });
                                let _ = tx.send(result);
                            });
                        }
                    }
                    State::Waiting => {
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label("Esperando confirmación y configurando…");
                        ui.add_space(10.0);
                        if ui.button("Cancelar").clicked() {
                            self.cancel.store(true, Ordering::Relaxed);
                            self.rx = None;
                            self.state = State::Ready;
                        }
                    }
                    State::Done(username) => {
                        ui.colored_label(
                            Color32::from_rgb(0, 180, 80),
                            format!("✓ Conectado como {username}"),
                        );
                        ui.add_space(8.0);
                        ui.label("El cliente Nextcloud se ha iniciado.");
                        if let Some(t) = self.close_at {
                            if Instant::now() >= t {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            } else {
                                ctx.request_repaint_after(Duration::from_millis(200));
                            }
                        }
                    }
                    State::Error(msg) => {
                        ui.colored_label(Color32::RED, format!("✗ {msg}"));
                        ui.add_space(10.0);
                        if ui.button("Reintentar").clicked() {
                            self.state = State::Ready;
                        }
                    }
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Nube Educamadrid")
            .with_app_id("educamadrid-nextcloud")
            .with_inner_size([380.0, 230.0])
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "Nube Educamadrid",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

// ---------- Login Flow v2 ----------

fn run_login_flow(cancel: &AtomicBool) -> Result<LoginResult, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(REQUEST_TIMEOUT)
        .timeout_read(REQUEST_TIMEOUT)
        .timeout_write(REQUEST_TIMEOUT)
        .build();

    let response = agent
        .post(&format!("{SERVER_URL}index.php/login/v2"))
        .call()
        .map_err(|e| format!("Error conectando con el servidor: {e}"))?;

    let flow: FlowResponse = response
        .into_json()
        .map_err(|e| format!("Respuesta inválida del servidor: {e}"))?;

    validate_flow_response(&flow)?;

    std::process::Command::new("xdg-open")
        .arg(&flow.login)
        .spawn()
        .map_err(|e| format!("No se pudo abrir el navegador: {e}"))?;

    let start = Instant::now();
    let mut transport_errors = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelado".into());
        }
        if start.elapsed() > POLL_TIMEOUT {
            return Err("Tiempo de espera agotado (5 minutos)".into());
        }
        std::thread::sleep(POLL_INTERVAL);
        match agent
            .post(&flow.poll.endpoint)
            .send_form(&[("token", flow.poll.token.as_str())])
        {
            Ok(resp) => {
                let result: LoginResult = resp
                    .into_json()
                    .map_err(|e| format!("Error leyendo credenciales: {e}"))?;
                validate_login_result(&result)?;
                return Ok(result);
            }
            Err(e) => match classify_poll_error(&e) {
                PollError::Pending => transport_errors = 0,
                PollError::Transport => {
                    transport_errors += 1;
                    if transport_errors >= MAX_TRANSPORT_ERRORS {
                        return Err(format!(
                            "Sin conexión con el servidor ({MAX_TRANSPORT_ERRORS} errores de red seguidos): {e}"
                        ));
                    }
                }
                PollError::Fatal => return Err(format!("Error de sondeo: {e}")),
            },
        }
    }
}

enum PollError {
    /// 404: el usuario aún no ha confirmado en el navegador.
    Pending,
    /// Fallo de red puntual: se reintenta.
    Transport,
    Fatal,
}

fn classify_poll_error(error: &ureq::Error) -> PollError {
    match error {
        ureq::Error::Status(404, _) => PollError::Pending,
        ureq::Error::Transport(_) => PollError::Transport,
        ureq::Error::Status(..) => PollError::Fatal,
    }
}

fn validate_flow_response(flow: &FlowResponse) -> Result<(), String> {
    if flow.poll.token.is_empty() {
        return Err("El servidor devolvió un token de acceso vacío".into());
    }
    validate_server_url(&flow.login, "URL de acceso")?;
    validate_server_url(&flow.poll.endpoint, "URL de sondeo")
}

fn validate_login_result(result: &LoginResult) -> Result<(), String> {
    validate_username(&result.login_name)?;
    if normalize_server(&result.server) != normalize_server(SERVER_URL) {
        return Err("El servidor devolvió una URL de cuenta inesperada".into());
    }
    if result.app_password.is_empty() {
        return Err("El servidor devolvió una contraseña de aplicación vacía".into());
    }
    Ok(())
}

fn validate_server_url(url: &str, description: &str) -> Result<(), String> {
    if url.starts_with(SERVER_URL) && !url.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(format!(
            "{description} inesperada: debe pertenecer a Educamadrid"
        ))
    }
}

fn validate_username(username: &str) -> Result<(), String> {
    if username.is_empty()
        || username.len() > 200
        || username
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err("El servidor devolvió un nombre de usuario no válido".into());
    }
    Ok(())
}

fn normalize_server(server: &str) -> &str {
    server.trim_end_matches('/')
}

// ---------- Cierre del cliente Nextcloud ----------

// `comm` termina en salto de línea al leerlo de /proc.
fn is_nextcloud_client(comm: &str) -> bool {
    comm.trim_end() == CLIENT_COMM
}

/// PIDs de procesos `nextcloud` del usuario actual.
fn running_client_pids() -> Vec<u32> {
    let Ok(own_uid) = std::fs::metadata("/proc/self").map(|m| m.uid()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            let comm = std::fs::read_to_string(entry.path().join("comm")).ok()?;
            let uid = entry.metadata().ok()?.uid();
            (is_nextcloud_client(&comm) && uid == own_uid).then_some(pid)
        })
        .collect()
}

/// Espera a que `done` sea cierto, comprobándolo cada `interval`. Devuelve si lo logró.
fn wait_until(timeout: Duration, interval: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if done() {
            return true;
        }
        if start.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(interval);
    }
}

// Cierra el cliente antes de tocar su configuración: si sigue abierto, al salir
// reescribiría nextcloud.cfg y pisaría la cuenta recién añadida.
fn close_nextcloud_client() -> Result<(), String> {
    let pids = running_client_pids();
    if pids.is_empty() {
        return Ok(());
    }
    let all_gone = || !running_client_pids().iter().any(|pid| pids.contains(pid));

    let _ = std::process::Command::new(CLIENT_COMM)
        .arg("--quit")
        .status();
    if wait_until(QUIT_TIMEOUT, CLOSE_CHECK_INTERVAL, all_gone) {
        return Ok(());
    }

    for pid in &pids {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .status();
    }
    if wait_until(KILL_TIMEOUT, CLOSE_CHECK_INTERVAL, all_gone) {
        return Ok(());
    }
    Err("No se pudo cerrar el cliente Nextcloud; ciérralo y pulsa Reintentar".into())
}

// ---------- Configuración de cuenta ----------

fn setup_account(result: LoginResult) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("No se puede obtener el directorio home")?;
    let sync_dir = home.join(format!("Cloud - {}", result.login_name));

    std::fs::create_dir_all(&sync_dir)
        .map_err(|e| format!("Error creando carpeta de sincronización: {e}"))?;

    let account_id = write_nextcloud_config(&result.login_name, &result.server, &sync_dir)?;
    store_in_kwallet(
        &result.login_name,
        &result.server,
        &account_id,
        &result.app_password,
    )?;
    add_dolphin_bookmark(&result.login_name, &sync_dir)?;

    std::process::Command::new("nextcloud")
        .spawn()
        .map_err(|e| format!("No se pudo iniciar nextcloud: {e}"))?;

    Ok(())
}

#[derive(Default)]
struct AccountConfig {
    username: Option<String>,
    server: Option<String>,
}

fn write_nextcloud_config(username: &str, server: &str, sync_dir: &Path) -> Result<String, String> {
    let config_dir = dirs::config_dir()
        .ok_or("No se puede obtener el directorio de configuración")?
        .join("Nextcloud");

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Error creando directorio de configuración: {e}"))?;

    let config_path = config_dir.join("nextcloud.cfg");
    let sync_path = sync_dir.to_string_lossy();
    let server = normalize_server(server);

    if !config_path.exists() {
        let content = format!(
            "[General]\n\
             launchOnSystemStartup=true\n\
             notifyExistingFoldersOverLimit=false\n\
             stopSyncingExistingFoldersOverLimit=false\n\
             useNewBigFolderSizeLimit=false\n\
             \n\
             [Accounts]\n\
             0\\authType=webflow\n\
             0\\dav_user={username}\n\
             0\\url={server}\n\
             0\\version=13\n\
             0\\webflow_user={username}\n\
             0\\Folders\\1\\ignoreHiddenFiles=false\n\
             0\\Folders\\1\\localPath={sync_path}/\n\
             0\\Folders\\1\\paused=false\n\
             0\\Folders\\1\\targetPath=/\n\
             0\\Folders\\1\\version=2\n\
             0\\Folders\\1\\virtualFilesMode=off\n\
             version=13\n"
        );
        atomic_write(&config_path, content.as_bytes())
            .map_err(|e| format!("Error escribiendo configuración: {e}"))?;
        return Ok("0".into());
    }

    let existing = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Error leyendo configuración existente: {e}"))?;

    let with_settings = apply_general_settings(&existing);
    let accounts = parse_accounts(&with_settings);
    if let Some((account_id, _)) = accounts.iter().find(|(_, account)| {
        account.username.as_deref() == Some(username)
            && account.server.as_deref().map(normalize_server) == Some(server)
    }) {
        if with_settings != existing {
            atomic_write(&config_path, with_settings.as_bytes())
                .map_err(|e| format!("Error actualizando configuración: {e}"))?;
        }
        return Ok(account_id.to_string());
    }

    let mut idx = 0;
    while accounts.contains_key(&idx) {
        idx += 1;
    }

    let new_lines = format!(
        "{idx}\\authType=webflow\n\
         {idx}\\dav_user={username}\n\
         {idx}\\url={server}\n\
         {idx}\\version=13\n\
         {idx}\\webflow_user={username}\n\
         {idx}\\Folders\\1\\ignoreHiddenFiles=false\n\
         {idx}\\Folders\\1\\localPath={sync_path}/\n\
         {idx}\\Folders\\1\\paused=false\n\
         {idx}\\Folders\\1\\targetPath=/\n\
         {idx}\\Folders\\1\\version=2\n\
         {idx}\\Folders\\1\\virtualFilesMode=off\n"
    );

    let updated = insert_account_lines(&with_settings, &new_lines);

    atomic_write(&config_path, updated.as_bytes())
        .map_err(|e| format!("Error actualizando configuración: {e}"))?;
    Ok(idx.to_string())
}

// Ajustes de [General] que se fuerzan siempre: sin ellos el cliente deja sin
// sincronizar (a la espera de confirmación) las carpetas de más de 500 MB.
const GENERAL_SETTINGS: [(&str, &str); 3] = [
    ("notifyExistingFoldersOverLimit", "false"),
    ("stopSyncingExistingFoldersOverLimit", "false"),
    ("useNewBigFolderSizeLimit", "false"),
];

// Fija GENERAL_SETTINGS en [General]: corrige el valor si la clave existe, la
// añade al final de la sección si falta y crea la sección si no hay.
fn apply_general_settings(existing: &str) -> String {
    let mut output = String::with_capacity(existing.len() + 128);
    let mut in_general = false;
    let mut found_general = false;
    let mut seen = [false; GENERAL_SETTINGS.len()];

    let push_missing = |output: &mut String, seen: &[bool]| {
        for ((key, value), done) in GENERAL_SETTINGS.iter().zip(seen) {
            if !done {
                output.push_str(&format!("{key}={value}\n"));
            }
        }
    };

    for line in existing.split_inclusive('\n') {
        let trimmed = line.trim().trim_end_matches('\r');
        let is_section = trimmed.starts_with('[') && trimmed.ends_with(']');

        if is_section {
            if in_general {
                // Las claves que falten van antes de la línea en blanco que separa secciones.
                let blank_tail = output.ends_with("\n\n");
                if blank_tail {
                    output.pop();
                }
                push_missing(&mut output, &seen);
                seen = [true; GENERAL_SETTINGS.len()];
                if blank_tail {
                    output.push('\n');
                }
            }
            in_general = trimmed == "[General]";
            found_general |= in_general;
        } else if in_general {
            let key = trimmed.split_once('=').map(|(key, _)| key);
            if let Some(i) = GENERAL_SETTINGS.iter().position(|(k, _)| Some(*k) == key) {
                let (k, v) = GENERAL_SETTINGS[i];
                output.push_str(&format!("{k}={v}\n"));
                seen[i] = true;
                continue;
            }
        }
        output.push_str(line);
    }

    if in_general {
        ensure_trailing_newline(&mut output);
        push_missing(&mut output, &seen);
    } else if !found_general {
        let mut header = String::from("[General]\n");
        push_missing(&mut header, &[false; GENERAL_SETTINGS.len()]);
        header.push('\n');
        output.insert_str(0, &header);
    }

    output
}

fn parse_accounts(content: &str) -> BTreeMap<usize, AccountConfig> {
    let mut accounts: BTreeMap<usize, AccountConfig> = BTreeMap::new();
    let mut in_accounts = false;

    for line in content.lines() {
        let trimmed = line.trim().trim_end_matches('\r');
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_accounts = trimmed == "[Accounts]";
            continue;
        }
        if !in_accounts {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let Some((account_id, field)) = key.split_once('\\') else {
            continue;
        };
        let Ok(account_id) = account_id.parse::<usize>() else {
            continue;
        };

        let account = accounts.entry(account_id).or_default();
        match field {
            "dav_user" => account.username = Some(value.to_owned()),
            "url" => account.server = Some(value.to_owned()),
            _ => {}
        }
    }

    accounts
}

fn insert_account_lines(existing: &str, new_lines: &str) -> String {
    let mut output = String::with_capacity(existing.len() + new_lines.len() + 32);
    let mut in_accounts = false;
    let mut found_accounts = false;
    let mut inserted = false;

    for line in existing.split_inclusive('\n') {
        let trimmed = line.trim().trim_end_matches('\r');
        let is_section = trimmed.starts_with('[') && trimmed.ends_with(']');

        if in_accounts
            && !inserted
            && (is_section || (trimmed.starts_with("version=") && !trimmed.contains('\\')))
        {
            ensure_trailing_newline(&mut output);
            output.push_str(new_lines);
            inserted = true;
        }

        if is_section {
            in_accounts = trimmed == "[Accounts]";
            found_accounts |= in_accounts;
        }
        output.push_str(line);
    }

    if found_accounts && !inserted {
        ensure_trailing_newline(&mut output);
        output.push_str(new_lines);
    } else if !found_accounts {
        ensure_trailing_newline(&mut output);
        output.push_str("\n[Accounts]\n");
        output.push_str(new_lines);
        output.push_str("version=13\n");
    }

    output
}

fn ensure_trailing_newline(content: &mut String) {
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
}

// Reproduce el backend KWallet de QtKeychain que usa Nextcloud: consulta el
// monedero de red configurado y escribe la clave con URL e id de cuenta.
fn store_in_kwallet(
    username: &str,
    server: &str,
    account_id: &str,
    password: &str,
) -> Result<(), String> {
    let conn =
        Connection::new_session().map_err(|e| format!("Error abriendo sesión D-Bus: {e}"))?;

    let candidates = [
        ("org.kde.kwalletd6", "/modules/kwalletd6"),
        ("org.kde.kwalletd5", "/modules/kwalletd5"),
        ("org.kde.kwalletd", "/modules/kwalletd"),
    ];
    let mut selected = None;
    let mut last_error = None;

    for (service, object) in candidates {
        let proxy = conn.with_proxy(service, object, Duration::from_secs(3));
        let reply: Result<(String,), _> = proxy.method_call("org.kde.KWallet", "networkWallet", ());
        match reply {
            Ok((wallet,)) if !wallet.is_empty() => {
                selected = Some((service, object, wallet));
                break;
            }
            Ok(_) => last_error = Some("KWallet devolvió un monedero de red vacío".into()),
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    let (service, object, wallet) = selected.ok_or_else(|| {
        format!(
            "No se encontró un servicio KWallet utilizable: {}",
            last_error.unwrap_or_else(|| "error desconocido".into())
        )
    })?;

    let proxy = conn.with_proxy(service, object, Duration::from_secs(10));

    let (handle,): (i32,) = proxy
        .method_call(
            "org.kde.KWallet",
            "open",
            (wallet.as_str(), 0i64, NEXTCLOUD_KEYCHAIN_SERVICE),
        )
        .map_err(|e| format!("KWallet open: {e}"))?;

    if handle < 0 {
        return Err("KWallet rechazó la apertura del monedero".into());
    }

    let key = nextcloud_keychain_key(username, server, account_id);
    let write_result: Result<(i32,), dbus::Error> = proxy.method_call(
        "org.kde.KWallet",
        "writePassword",
        (
            handle,
            NEXTCLOUD_KEYCHAIN_SERVICE,
            key.as_str(),
            password,
            NEXTCLOUD_KEYCHAIN_SERVICE,
        ),
    );

    let (status,) = write_result.map_err(|e| format!("KWallet writePassword: {e}"))?;
    if status != 0 {
        return Err(format!(
            "KWallet no pudo guardar la contraseña (código {status})"
        ));
    }

    // QtKeychain conserva el handle y deja que KWallet gestione su ciclo de vida.
    // Cerrar aquí falla si el monedero sigue en uso por otra aplicación.

    Ok(())
}

fn nextcloud_keychain_key(username: &str, server: &str, account_id: &str) -> String {
    format!("{username}:{}/:{account_id}", normalize_server(server))
}

// ---------- Marcador en Dolphin ----------

fn add_dolphin_bookmark(username: &str, sync_dir: &Path) -> Result<(), String> {
    let places_path = dirs::data_local_dir()
        .ok_or("No se puede obtener el directorio de datos locales")?
        .join("user-places.xbel");

    if let Some(parent) = places_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Error creando directorio de marcadores: {e}"))?;
    }

    let href = path_to_file_url(sync_dir);
    let href_xml = escape_xml(&href);
    let title = escape_xml(&format!("Cloud - {username}"));

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let entry = format!(
        " <bookmark href=\"{href_xml}\">\n  \
           <title>{title}</title>\n  \
           <info>\n   \
             <metadata owner=\"http://freedesktop.org\">\n    \
               <bookmark:icon name=\"folder-cloud\"/>\n   \
             </metadata>\n   \
             <metadata owner=\"http://www.kde.org\">\n    \
               <ID>{ts}/0</ID>\n    \
               <IsHidden>false</IsHidden>\n   \
             </metadata>\n  \
           </info>\n \
         </bookmark>\n"
    );

    let content = if places_path.exists() {
        let existing = std::fs::read_to_string(&places_path)
            .map_err(|e| format!("Error leyendo user-places.xbel: {e}"))?;
        if existing.contains(&format!("href=\"{href_xml}\"")) {
            return Ok(());
        }
        let Some(position) = existing.rfind("</xbel>") else {
            return Err("user-places.xbel no contiene una etiqueta </xbel> válida".into());
        };
        format!("{}{entry}{}", &existing[..position], &existing[position..])
    } else {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE xbel PUBLIC \"+//IDN python.org//DTD XML Bookmark Exchange Language 1.0//EN//XML\" \
             \"http://pyxml.sourceforge.net/topics/dtd/xbel-1.0.dtd\">\n\
             <xbel xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\" \
             xmlns:mime=\"http://www.freedesktop.org/standards/shared-mime-info\" version=\"1.0\">\n\
             {entry}</xbel>\n"
        )
    };

    atomic_write(&places_path, content.as_bytes())
        .map_err(|e| format!("Error escribiendo user-places.xbel: {e}"))?;
    Ok(())
}

fn path_to_file_url(path: &Path) -> String {
    let mut encoded = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "la ruta no tiene directorio padre",
        )
    })?;
    let original_permissions = std::fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let temp_path = unique_temp_path(path);

    let result = (|| {
        let mut temp = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        if let Some(permissions) = original_permissions {
            temp.set_permissions(permissions)?;
        }
        temp.write_all(content)?;
        temp.sync_all()?;
        std::fs::rename(&temp_path, path)?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(".{filename}.{}.{}.tmp", std::process::id(), nonce))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_accounts_without_combining_different_ids() {
        let content = "[Accounts]\n0\\dav_user=ana\n0\\url=https://other.example\n\
                       1\\dav_user=otro\n1\\url=https://cloud.educa.madrid.org\nversion=13\n";
        let accounts = parse_accounts(content);

        assert_eq!(accounts[&0].username.as_deref(), Some("ana"));
        assert_eq!(
            accounts[&0].server.as_deref(),
            Some("https://other.example")
        );
        assert_eq!(accounts[&1].username.as_deref(), Some("otro"));
        assert_eq!(
            accounts[&1].server.as_deref(),
            Some("https://cloud.educa.madrid.org")
        );
    }

    #[test]
    fn inserts_account_inside_accounts_section() {
        let existing =
            "[General]\nfoo=true\n\n[Accounts]\n0\\dav_user=ana\nversion=13\n\n[Other]\nx=1\n";
        let updated = insert_account_lines(existing, "1\\dav_user=bea\n");

        let account_position = updated.find("1\\dav_user=bea").unwrap();
        let version_position = updated.find("version=13").unwrap();
        let other_position = updated.find("[Other]").unwrap();
        assert!(account_position < version_position);
        assert!(version_position < other_position);
    }

    #[test]
    fn forces_big_folder_settings_in_general() {
        let existing = "[General]\nlaunchOnSystemStartup=true\nuseNewBigFolderSizeLimit=true\n\n\
                        [Accounts]\n0\\dav_user=ana\nversion=13\n";
        let updated = apply_general_settings(existing);
        assert_eq!(
            updated,
            "[General]\nlaunchOnSystemStartup=true\nuseNewBigFolderSizeLimit=false\n\
             notifyExistingFoldersOverLimit=false\nstopSyncingExistingFoldersOverLimit=false\n\n\
             [Accounts]\n0\\dav_user=ana\nversion=13\n"
        );
        assert_eq!(apply_general_settings(&updated), updated);
    }

    #[test]
    fn creates_general_section_when_missing() {
        let updated = apply_general_settings("[Accounts]\nversion=13\n");
        assert!(updated.starts_with("[General]\nnotifyExistingFoldersOverLimit=false\n"));
        assert!(updated.ends_with("useNewBigFolderSizeLimit=false\n\n[Accounts]\nversion=13\n"));
    }

    #[test]
    fn creates_accounts_section_when_missing() {
        let updated = insert_account_lines("[General]\nfoo=true\n", "0\\dav_user=ana\n");

        assert!(updated.contains("[Accounts]\n0\\dav_user=ana\nversion=13\n"));
    }

    #[test]
    fn builds_current_nextcloud_keychain_key() {
        assert_eq!(
            nextcloud_keychain_key(
                "ana@educa.madrid.org",
                "https://cloud.educa.madrid.org/",
                "2"
            ),
            "ana@educa.madrid.org:https://cloud.educa.madrid.org/:2"
        );
    }

    #[test]
    fn escapes_bookmark_values() {
        assert_eq!(escape_xml("a&<\"'"), "a&amp;&lt;&quot;&apos;");
        assert_eq!(
            path_to_file_url(Path::new("/home/a/Cloud - a+b@example.org")),
            "file:///home/a/Cloud%20-%20a%2Bb%40example.org"
        );
    }

    #[test]
    fn detects_nextcloud_client_by_comm() {
        assert!(is_nextcloud_client("nextcloud\n"));
        assert!(is_nextcloud_client("nextcloud"));
        assert!(!is_nextcloud_client("nextcloud-desktop\n"));
        assert!(!is_nextcloud_client("educamadrid-nextcloud\n"));
        assert!(!is_nextcloud_client("Nextcloud\n"));
    }

    #[test]
    fn classifies_poll_errors() {
        let pending = ureq::Error::Status(404, ureq::Response::new(404, "Not Found", "").unwrap());
        let server = ureq::Error::Status(500, ureq::Response::new(500, "Oops", "").unwrap());
        let network = ureq::Error::from(std::io::Error::other("sin red"));
        assert!(matches!(classify_poll_error(&pending), PollError::Pending));
        assert!(matches!(classify_poll_error(&server), PollError::Fatal));
        assert!(matches!(
            classify_poll_error(&network),
            PollError::Transport
        ));
    }

    #[test]
    fn wait_until_respects_condition_and_timeout() {
        let tick = Duration::from_millis(1);
        let mut calls = 0;
        assert!(wait_until(Duration::from_secs(1), tick, || {
            calls += 1;
            calls == 3
        }));
        assert!(!wait_until(Duration::from_millis(10), tick, || false));
    }

    #[test]
    fn rejects_unexpected_server_data() {
        assert!(
            validate_server_url("https://cloud.educa.madrid.org/index.php/login", "URL").is_ok()
        );
        assert!(
            validate_server_url("https://cloud.educa.madrid.org.evil.example/login", "URL")
                .is_err()
        );
        assert!(validate_username("../../escape").is_err());
    }
}
