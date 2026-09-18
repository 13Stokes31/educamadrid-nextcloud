use dbus::blocking::Connection;
use eframe::egui::{self, Color32, RichText};
use serde::Deserialize;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SERVER_URL: &str = "https://cloud.educa.madrid.org/";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TIMEOUT: Duration = Duration::from_secs(600);

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
    rx: Option<mpsc::Receiver<Result<LoginResult, String>>>,
    close_at: Option<Instant>,
}

impl Default for App {
    fn default() -> Self {
        Self { state: State::Ready, rx: None, close_at: None }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let State::Waiting = &self.state {
            if let Some(rx) = &self.rx {
                match rx.try_recv() {
                    Ok(Ok(result)) => {
                        let username = result.login_name.clone();
                        match setup_account(result) {
                            Ok(_) => {
                                self.state = State::Done(username);
                                self.close_at =
                                    Some(Instant::now() + Duration::from_secs(3));
                            }
                            Err(e) => self.state = State::Error(e),
                        }
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
                            self.rx = Some(rx);
                            self.state = State::Waiting;
                            std::thread::spawn(move || {
                                let _ = tx.send(run_login_flow());
                            });
                        }
                    }
                    State::Waiting => {
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label("Esperando confirmación en el navegador…");
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

fn run_login_flow() -> Result<LoginResult, String> {
    let response = ureq::post(&format!("{SERVER_URL}index.php/login/v2"))
        .call()
        .map_err(|e| format!("Error conectando con el servidor: {e}"))?;

    let flow: FlowResponse = response
        .into_json()
        .map_err(|e| format!("Respuesta inválida del servidor: {e}"))?;

    std::process::Command::new("xdg-open")
        .arg(&flow.login)
        .spawn()
        .map_err(|e| format!("No se pudo abrir el navegador: {e}"))?;

    let start = Instant::now();
    loop {
        if start.elapsed() > POLL_TIMEOUT {
            return Err("Tiempo de espera agotado (10 minutos)".into());
        }
        std::thread::sleep(POLL_INTERVAL);
        match ureq::post(&flow.poll.endpoint)
            .send_form(&[("token", flow.poll.token.as_str())])
        {
            Ok(resp) => {
                return resp
                    .into_json()
                    .map_err(|e| format!("Error leyendo credenciales: {e}"));
            }
            Err(ureq::Error::Status(404, _)) => continue,
            Err(e) => return Err(format!("Error de sondeo: {e}")),
        }
    }
}

// ---------- Configuración de cuenta ----------

fn setup_account(result: LoginResult) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("No se puede obtener el directorio home")?;
    let sync_dir = home.join(format!("Cloud - {}", result.login_name));

    std::fs::create_dir_all(&sync_dir)
        .map_err(|e| format!("Error creando carpeta de sincronización: {e}"))?;

    stop_nextcloud_if_running()?;

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

fn effective_uid(status: &str) -> Option<u32> {
    let line = status.lines().find(|line| line.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

fn is_nextcloud_running() -> bool {
    let self_uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| effective_uid(&status));

    let Some(self_uid) = self_uid else {
        return false;
    };

    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };

    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let Some(pid) = name.to_str() else {
            return false;
        };
        if !pid.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        let comm_matches = std::fs::read_to_string(entry.path().join("comm"))
            .map(|comm| comm.trim() == "nextcloud")
            .unwrap_or(false);
        if !comm_matches {
            return false;
        }

        std::fs::read_to_string(entry.path().join("status"))
            .ok()
            .and_then(|status| effective_uid(&status))
            == Some(self_uid)
    })
}

fn stop_nextcloud_if_running() -> Result<(), String> {
    if !is_nextcloud_running() {
        return Ok(());
    }

    let status = std::process::Command::new("nextcloud")
        .arg("--quit")
        .status()
        .map_err(|e| format!("No se pudo solicitar el cierre de Nextcloud: {e}"))?;

    if !status.success() {
        return Err(format!(
            "Nextcloud no aceptó la orden de cierre (código {:?})",
            status.code()
        ));
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    while is_nextcloud_running() {
        if Instant::now() >= deadline {
            return Err(
                "Nextcloud sigue ejecutándose; no se modificará su configuración".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    Ok(())
}

fn write_nextcloud_config(
    username: &str,
    server: &str,
    sync_dir: &Path,
) -> Result<String, String> {
    let config_dir = dirs::config_dir()
        .ok_or("No se puede obtener el directorio de configuración")?
        .join("Nextcloud");

    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Error creando directorio de configuración: {e}"))?;

    let config_path = config_dir.join("nextcloud.cfg");
    let sync_path = sync_dir.to_string_lossy();
    let server = server.trim_end_matches('/');

    if !config_path.exists() {
        let content = format!(
            "[General]\n\
             launchOnSystemStartup=true\n\
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
        std::fs::write(&config_path, content)
            .map_err(|e| format!("Error escribiendo configuración: {e}"))?;
        return Ok("0".into());
    }

    let existing = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Error leyendo configuración existente: {e}"))?;

    if let Some(account_id) = find_account_id(&existing, username, server) {
        return Ok(account_id);
    }

    // Siguiente índice libre de cuenta.
    let mut idx = 0;
    while existing
        .lines()
        .any(|line| line.starts_with(&format!("{idx}\\")))
    {
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

    // Insertar antes del marcador "version=13" de sección (sin prefijo numérico)
    let updated = if let Some(pos) = existing.rfind("\nversion=13") {
        format!("{}\n{}{}", &existing[..pos], new_lines, &existing[pos + 1..])
    } else {
        format!("{existing}{new_lines}")
    };

    std::fs::write(&config_path, updated)
        .map_err(|e| format!("Error actualizando configuración: {e}"))?;
    Ok(idx.to_string())
}

fn find_account_id(existing: &str, username: &str, server: &str) -> Option<String> {
    let user_suffix = format!("\\dav_user={username}");
    for line in existing.lines() {
        let Some(account_id) = line.strip_suffix(&user_suffix) else {
            continue;
        };
        if account_id.is_empty() || account_id.contains('\\') {
            continue;
        }

        let expected_url = format!("{account_id}\\url={server}");
        if existing.lines().any(|candidate| candidate == expected_url.as_str()) {
            return Some(account_id.to_string());
        }
    }
    None
}

// nextcloud-client en KDE usa KWallet directamente vía D-Bus (QtKeychain),
// no la Secret Service API. Escribimos en KWallet con la misma clave que
// busca nextcloud: "loginName:serverUrl/:accountId" en la carpeta "Nextcloud".
fn nextcloud_keychain_key(username: &str, server: &str, account_id: &str) -> String {
    let server = format!("{}/", server.trim_end_matches('/'));
    format!("{username}:{server}:{account_id}")
}

fn store_in_kwallet(
    username: &str,
    server: &str,
    account_id: &str,
    password: &str,
) -> Result<(), String> {
    let conn = Connection::new_session()
        .map_err(|e| format!("Error abriendo sesión D-Bus: {e}"))?;

    let key = nextcloud_keychain_key(username, server, account_id);
    let backends = [
        ("org.kde.kwalletd6", "/modules/kwalletd6"),
        ("org.kde.kwalletd5", "/modules/kwalletd5"),
    ];
    let mut errors = Vec::new();

    for (service, obj) in backends {
        let proxy = conn.with_proxy(service, obj, Duration::from_secs(10));

        // La llamada activa kwalletd mediante D-Bus si todavía no estaba arrancado.
        let wallet_result: Result<(String,), _> =
            proxy.method_call("org.kde.KWallet", "networkWallet", ());
        let (wallet_name,) = match wallet_result {
            Ok(wallet) => wallet,
            Err(e) => {
                errors.push(format!("{service}: {e}"));
                continue;
            }
        };

        let (handle,): (i32,) = proxy
            .method_call(
                "org.kde.KWallet",
                "open",
                (wallet_name.as_str(), 0i64, "nextcloud-educamadrid"),
            )
            .map_err(|e| format!("KWallet open: {e}"))?;

        if handle < 0 {
            return Err("KWallet rechazó la apertura del monedero".into());
        }

        let (write_result,): (i32,) = proxy
            .method_call(
                "org.kde.KWallet",
                "writePassword",
                (handle, "Nextcloud", key.as_str(), password, "nextcloud-educamadrid"),
            )
            .map_err(|e| format!("KWallet writePassword: {e}"))?;

        if write_result != 0 {
            return Err(format!(
                "KWallet no pudo guardar la contraseña (código {write_result})"
            ));
        }

        return Ok(());
    }

    Err(format!(
        "No se pudo acceder a KWallet 6 ni KWallet 5: {}",
        errors.join(" | ")
    ))
}

// ---------- Marcador en Dolphin ----------

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for &byte in path.as_bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'/'
            | b'-'
            | b'_'
            | b'.'
            | b'~' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn add_dolphin_bookmark(username: &str, sync_dir: &Path) -> Result<(), String> {
    let places_path = dirs::data_local_dir()
        .ok_or("No se puede obtener el directorio de datos locales")?
        .join("user-places.xbel");

    let path_str = sync_dir.to_string_lossy();
    let href = format!("file://{}", percent_encode_path(&path_str));
    let href_xml = xml_escape(&href);
    let title = xml_escape(&format!("Cloud - {username}"));

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
        existing.replace("</xbel>", &format!("{entry}</xbel>"))
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

    std::fs::write(&places_path, content)
        .map_err(|e| format!("Error escribiendo user-places.xbel: {e}"))?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keychain_key_matches_nextcloud_34_format() {
        assert_eq!(
            nextcloud_keychain_key("alice", "https://cloud.educa.madrid.org", "0"),
            "alice:https://cloud.educa.madrid.org/:0"
        );
        assert_eq!(
            nextcloud_keychain_key("alice", "https://cloud.educa.madrid.org/", "3"),
            "alice:https://cloud.educa.madrid.org/:3"
        );
    }

    #[test]
    fn finds_only_the_matching_account() {
        let config = "[Accounts]\n\
                      0\\dav_user=alice\n\
                      0\\url=https://otro.example\n\
                      1\\dav_user=bob\n\
                      1\\url=https://cloud.educa.madrid.org\n\
                      2\\dav_user=alice\n\
                      2\\url=https://cloud.educa.madrid.org\n";

        assert_eq!(
            find_account_id(config, "alice", "https://cloud.educa.madrid.org"),
            Some("2".into())
        );
        assert_eq!(
            find_account_id(config, "carol", "https://cloud.educa.madrid.org"),
            None
        );
    }

    #[test]
    fn parses_effective_uid_from_proc_status() {
        let status = "Name:\ttest\nUid:\t1000\t1001\t1002\t1003\n";
        assert_eq!(effective_uid(status), Some(1001));
    }

    #[test]
    fn encodes_dolphin_paths_and_xml() {
        assert_eq!(
            percent_encode_path("/home/alumno/Cloud - josé"),
            "/home/alumno/Cloud%20-%20jos%C3%A9"
        );
        assert_eq!(xml_escape("A&B <test>"), "A&amp;B &lt;test&gt;");
    }
}
