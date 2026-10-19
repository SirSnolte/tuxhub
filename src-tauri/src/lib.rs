mod github;
mod manifest;
mod native;
mod shortcuts;
mod tools;

use manifest::Manifest;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Manager};
use tools::{Entry, Installed, Source};

/// Repo, aus dem der Workshop-Index live geladen wird.
const WORKSHOP_REPO: &str = "SirSnolte/tuxhub";
const WORKSHOP_PATH: &str = "registry/index.json";
/// Mitgelieferter Index, falls GitHub nicht erreichbar ist.
const WORKSHOP_BUNDLED: &str = include_str!("../../registry/index.json");
/// JS-Bridge, die Tool-Oberflächen per `<script src="/__tuxhub/bridge.js">` laden.
const BRIDGE_JS: &str = include_str!("bridge.js");

pub struct Hub {
    tools_dir: PathBuf,
    settings_dir: PathBuf,
    runtime: native::Runtime,
    shortcuts: std::sync::Mutex<shortcuts::Status>,
}

impl Hub {
    fn settings_path(&self, id: &str) -> PathBuf {
        self.settings_dir.join(format!("{id}.json"))
    }

    /// Defaults aus dem Manifest, überschrieben mit den gespeicherten Werten.
    fn settings(&self, id: &str, m: &Manifest) -> Map<String, Value> {
        let mut values = m.default_settings();
        if let Ok(s) = std::fs::read_to_string(self.settings_path(id)) {
            if let Ok(Value::Object(saved)) = serde_json::from_str(&s) {
                for (k, v) in saved {
                    if values.contains_key(&k) {
                        values.insert(k, v);
                    }
                }
            }
        }
        values
    }

    fn load(&self, id: &str) -> Result<(Installed, Manifest), String> {
        let inst = tools::load(&self.tools_dir, id)?;
        let m = inst.manifest(&self.tools_dir);
        Ok((inst, m))
    }
}

fn parse_workshop(s: &str) -> Result<Vec<Entry>, String> {
    let entries: Vec<Entry> = serde_json::from_str(s).map_err(|e| format!("Workshop-Index ungültig: {e}"))?;
    Ok(entries
        .into_iter()
        .filter(|e| match e.validate() {
            Ok(()) => true,
            Err(err) => {
                eprintln!("Workshop-Eintrag übersprungen: {err}");
                false
            }
        })
        .collect())
}

/// Lädt den Workshop-Index aus dem Hub-Repo, sonst den mitgelieferten.
async fn workshop_entries() -> (Vec<Entry>, bool) {
    if !cfg!(debug_assertions) {
        if let Ok(s) = github::raw_file(WORKSHOP_REPO, WORKSHOP_PATH).await {
            if let Ok(entries) = parse_workshop(&s) {
                return (entries, true);
            }
        }
    }
    (parse_workshop(WORKSHOP_BUNDLED).unwrap_or_default(), false)
}

/// Startet den RPC-Prozess eines Tools, falls nötig, und schickt ihm beim
/// ersten Start die Einstellungen.
async fn ensure_rpc(app: &AppHandle, id: &str) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let (inst, m) = hub.load(id)?;
    let rpc = m.rpc.clone().ok_or("Dieses Tool hat keinen Hintergrundprozess (`rpc`)")?;
    let root = inst.root_dir(&hub.tools_dir).ok_or("Kein Tool-Ordner")?;
    let exe = root.join(&rpc.command);
    if !exe.is_file() {
        return Err(format!("{} fehlt", exe.display()));
    }
    if hub.runtime.ensure(app, id, &exe, &rpc.args, &root).await? {
        hub.runtime
            .call(id, "configure", Value::Object(hub.settings(id, &m)))
            .await?;
    }
    Ok(())
}

pub(crate) async fn run_action(app: &AppHandle, id: &str, action: &str) -> Result<Value, String> {
    {
        let hub = app.state::<Hub>();
        let (_, m) = hub.load(id)?;
        if !m.actions.iter().any(|a| a.id == action) {
            return Err(format!("{id} kennt die Aktion {action:?} nicht"));
        }
    }
    ensure_rpc(app, id).await?;
    app.state::<Hub>().runtime.call(id, "action", json!({ "id": action })).await
}

// ---------------------------------------------------------------- Workshop

#[derive(Serialize)]
struct WorkshopView {
    entries: Vec<WorkshopItem>,
    live: bool,
}

#[derive(Serialize)]
struct WorkshopItem {
    entry: Entry,
    installed: bool,
}

#[tauri::command]
async fn workshop_list(app: AppHandle) -> WorkshopView {
    let (entries, live) = workshop_entries().await;
    let hub = app.state::<Hub>();
    let installed: Vec<String> = tools::load_all(&hub.tools_dir)
        .into_iter()
        .map(|t| t.entry.id)
        .collect();
    WorkshopView {
        entries: entries
            .into_iter()
            .map(|entry| WorkshopItem { installed: installed.contains(&entry.id), entry })
            .collect(),
        live,
    }
}

#[tauri::command]
async fn workshop_readme(repo: String) -> Result<String, String> {
    github::readme(&repo).await
}

#[tauri::command]
async fn tool_install(app: AppHandle, id: String) -> Result<(), String> {
    let (entries, _) = workshop_entries().await;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("{id} ist nicht im Workshop"))?;
    let hub = app.state::<Hub>();
    // Bei Updates den alten Prozess vorher beenden.
    hub.runtime.stop(&id).await;
    tools::install(&app, &hub.tools_dir, &entry).await?;
    shortcuts::rebind(app.clone());
    Ok(())
}

// ------------------------------------------------------------------- Tools

#[derive(Serialize)]
struct ToolView {
    entry: Entry,
    version: Option<String>,
    manifest: Manifest,
    level: u8,
    running: bool,
    can_launch: bool,
    /// Ordner des Tools auf der Platte (nicht bei Flatpaks)
    path: Option<String>,
}

#[tauri::command]
async fn tool_list(app: AppHandle) -> Vec<ToolView> {
    let hub = app.state::<Hub>();
    let mut out = Vec::new();
    for inst in tools::load_all(&hub.tools_dir) {
        let manifest = inst.manifest(&hub.tools_dir);
        out.push(ToolView {
            running: hub.runtime.is_running(inst.id()).await,
            level: manifest.level(),
            can_launch: manifest.run.is_some()
                || inst.detected_run.is_some()
                || matches!(inst.entry.source, Source::Flatpak { .. }),
            path: inst.root_dir(&hub.tools_dir).map(|p| p.to_string_lossy().into_owned()),
            version: inst.version.clone().or_else(|| manifest.version.clone()),
            entry: inst.entry,
            manifest,
        });
    }
    out
}

#[tauri::command]
fn tool_launch(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    tools::launch(&hub.tools_dir, &tools::load(&hub.tools_dir, &id)?)
}

#[tauri::command]
async fn tool_uninstall(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    hub.runtime.stop(&id).await;
    tools::uninstall(&hub.tools_dir, &inst).await?;
    let _ = std::fs::remove_file(hub.settings_path(&id));
    shortcuts::rebind(app.clone());
    Ok(())
}

#[tauri::command]
async fn tool_check_update(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let inst = tools::load(&app.state::<Hub>().tools_dir, &id)?;
    tools::check_update(&inst).await
}

#[tauri::command]
fn tool_link_local(app: AppHandle, path: String) -> Result<String, String> {
    let hub = app.state::<Hub>();
    let inst = tools::link_local(&hub.tools_dir, Path::new(&path))?;
    shortcuts::rebind(app.clone());
    Ok(inst.entry.id)
}

#[tauri::command]
fn get_settings(app: AppHandle, id: String) -> Result<Map<String, Value>, String> {
    let hub = app.state::<Hub>();
    let (_, m) = hub.load(&id)?;
    Ok(hub.settings(&id, &m))
}

#[tauri::command]
async fn save_settings(app: AppHandle, id: String, values: Map<String, Value>) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let (_, m) = hub.load(&id)?;
    let mut merged = hub.settings(&id, &m);
    for (k, v) in values {
        if merged.contains_key(&k) {
            merged.insert(k, v);
        }
    }
    std::fs::write(hub.settings_path(&id), serde_json::to_string_pretty(&merged).unwrap())
        .map_err(|e| e.to_string())?;
    if hub.runtime.is_running(&id).await {
        hub.runtime.call(&id, "configure", Value::Object(merged)).await?;
    }
    Ok(())
}

#[tauri::command]
async fn tool_action(app: AppHandle, id: String, action: String) -> Result<Value, String> {
    run_action(&app, &id, &action).await
}

/// Freier RPC-Aufruf – für eigene Tool-Oberflächen über die JS-Bridge.
#[tauri::command]
async fn tool_call(app: AppHandle, id: String, method: String, params: Value) -> Result<Value, String> {
    ensure_rpc(&app, &id).await?;
    app.state::<Hub>().runtime.call(&id, &method, params).await
}

#[tauri::command]
async fn tool_stop(app: AppHandle, id: String) {
    app.state::<Hub>().runtime.stop(&id).await;
}

#[tauri::command]
fn shortcut_status(app: AppHandle) -> shortcuts::Status {
    app.state::<Hub>().shortcuts.lock().unwrap().clone()
}

// ------------------------------------------------------- tool://-Protokoll

/// Liefert Dateien aus dem Ordner eines Tools aus: `tool://<id>/<pfad>`.
fn serve_tool(app: &AppHandle, uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    let reply = |status: StatusCode, mime: &str, body: Vec<u8>| {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, mime)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(body)
            .unwrap()
    };
    let not_found = || reply(StatusCode::NOT_FOUND, "text/plain", b"nicht gefunden".to_vec());

    let id = uri.host().unwrap_or_default();
    let path = percent_decode(uri.path().trim_start_matches('/'));
    if path == "__tuxhub/bridge.js" {
        return reply(StatusCode::OK, "text/javascript", BRIDGE_JS.as_bytes().to_vec());
    }
    let hub = app.state::<Hub>();
    let Ok(inst) = tools::load(&hub.tools_dir, id) else { return not_found() };
    let Some(root) = inst.root_dir(&hub.tools_dir).and_then(|r| r.canonicalize().ok()) else {
        return not_found();
    };
    let Ok(file) = root.join(&path).canonicalize() else { return not_found() };
    // Nichts außerhalb des Tool-Ordners ausliefern.
    if !file.starts_with(&root) || !file.is_file() {
        return not_found();
    }
    match std::fs::read(&file) {
        Ok(body) => reply(StatusCode::OK, mime_for(&file), body),
        Err(_) => not_found(),
    }
}

fn mime_for(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("tool", |ctx, req| serve_tool(ctx.app_handle(), req.uri()))
        .setup(|app| {
            let tools_dir = app.path().app_data_dir()?.join("tools");
            let settings_dir = app.path().app_config_dir()?.join("settings");
            std::fs::create_dir_all(&tools_dir)?;
            std::fs::create_dir_all(&settings_dir)?;
            app.manage(Hub {
                tools_dir,
                settings_dir,
                runtime: native::Runtime::default(),
                shortcuts: Default::default(),
            });
            shortcuts::rebind(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workshop_list,
            workshop_readme,
            tool_install,
            tool_list,
            tool_launch,
            tool_uninstall,
            tool_check_update,
            tool_link_local,
            get_settings,
            save_settings,
            tool_action,
            tool_call,
            tool_stop,
            shortcut_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes() {
        assert_eq!(percent_decode("a%20b.html"), "a b.html");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%2"), "%2");
    }
}
