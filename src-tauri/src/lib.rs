mod embed;
mod github;
mod live;
mod manifest;
mod native;
mod shortcuts;
mod system;
mod tools;
mod updates;
mod webui;

use manifest::Manifest;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Emitter, Manager};
use tools::{Entry, Installed, Source};

/// Repository the workshop index is loaded from.
const WORKSHOP_REPO: &str = "SirSnolte/tuxhub";
const WORKSHOP_PATH: &str = "registry/index.json";
/// Bundled index, used when GitHub is not reachable.
const WORKSHOP_BUNDLED: &str = include_str!("../../registry/index.json");
/// JavaScript bridge that tool interfaces load with `<script src="/__tuxhub/bridge.js">`.
const BRIDGE_JS: &str = include_str!("bridge.js");
/// Time a live application has to quit before it is ended with SIGTERM.
const QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

pub struct Hub {
    tools_dir: PathBuf,
    settings_dir: PathBuf,
    hub_settings_path: PathBuf,
    runtime: native::Runtime,
    shortcuts: std::sync::Mutex<shortcuts::Status>,
    settings: std::sync::Mutex<system::HubSettings>,
    /// Tool id -> newer version
    updates: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
    /// Running sessions of applications shown inside tuxhub
    embeds: embed::Sessions,
    /// Connections to applications with a live interface
    live: live::Live,
}

impl Hub {
    #[cfg(test)]
    pub(crate) fn for_test(tools_dir: PathBuf, settings_dir: PathBuf) -> Self {
        Self {
            hub_settings_path: settings_dir.join("hub.json"),
            tools_dir,
            settings_dir,
            runtime: native::Runtime::default(),
            shortcuts: Default::default(),
            settings: Default::default(),
            updates: Default::default(),
            embeds: Default::default(),
            live: live::Live::new(None, std::sync::Arc::new(|_| {})),
        }
    }

    fn settings_path(&self, id: &str) -> PathBuf {
        self.settings_dir.join(format!("{id}.json"))
    }

    /// Manifest defaults, overridden by the saved values.
    fn settings(&self, id: &str, m: &Manifest) -> Map<String, Value> {
        let mut values = m.default_settings();
        if let Ok(s) = std::fs::read_to_string(self.settings_path(id))
            && let Ok(Value::Object(saved)) = serde_json::from_str(&s)
        {
            for (k, v) in saved {
                if values.contains_key(&k) {
                    values.insert(k, v);
                }
            }
        }
        values
    }

    fn dyn_shortcuts_path(&self, id: &str) -> PathBuf {
        self.settings_dir.join(format!("{id}.shortcuts.json"))
    }

    /// Hotkeys a tool reported at runtime (for example one per macro).
    pub(crate) fn dyn_shortcuts(&self, id: &str) -> Vec<DynShortcut> {
        std::fs::read_to_string(self.dyn_shortcuts_path(id))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn load(&self, id: &str) -> Result<(Installed, Manifest), String> {
        let inst = tools::load(&self.tools_dir, id)?;
        let m = inst.manifest(&self.tools_dir);
        Ok((inst, m))
    }
}

fn parse_workshop(s: &str) -> Result<Vec<Entry>, String> {
    let entries: Vec<Entry> =
        serde_json::from_str(s).map_err(|e| format!("invalid workshop index: {e}"))?;
    Ok(entries
        .into_iter()
        .filter(|e| match e.validate() {
            Ok(()) => true,
            Err(err) => {
                eprintln!("skipping workshop entry: {err}");
                false
            }
        })
        .collect())
}

/// Loads the workshop index from the repository, otherwise the bundled copy.
async fn workshop_entries() -> (Vec<Entry>, bool) {
    if !cfg!(debug_assertions)
        && let Ok(s) = github::raw_file(WORKSHOP_REPO, WORKSHOP_PATH).await
        && let Ok(entries) = parse_workshop(&s)
    {
        return (entries, true);
    }
    (parse_workshop(WORKSHOP_BUNDLED).unwrap_or_default(), false)
}

/// Starts the RPC process of a tool if needed and sends it the settings on the
/// first start.
async fn ensure_rpc(app: &AppHandle, id: &str) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let (inst, m) = hub.load(id)?;
    let rpc = m
        .rpc
        .clone()
        .ok_or("this tool has no background process (`rpc`)")?;
    let root = inst
        .root_dir(&hub.tools_dir)
        .ok_or("the tool has no folder")?;
    let exe = root.join(&rpc.command);
    if !exe.is_file() {
        return Err(format!("{} is missing", exe.display()));
    }
    if hub.runtime.ensure(app, id, &exe, &rpc.args, &root).await? {
        hub.runtime
            .call(id, "configure", Value::Object(hub.settings(id, &m)))
            .await?;
    }
    Ok(())
}

/// Starts the RPC processes of all installed tools that request `autostart`.
async fn start_background_tools(app: AppHandle) {
    let ids: Vec<String> = {
        let hub = app.state::<Hub>();
        tools::load_all(&hub.tools_dir)
            .into_iter()
            .filter(|t| t.manifest(&hub.tools_dir).rpc.is_some_and(|r| r.autostart))
            .map(|t| t.entry.id)
            .collect()
    };
    for id in ids {
        if let Err(e) = ensure_rpc(&app, &id).await {
            eprintln!("{id}: could not start in the background: {e}");
        }
    }
}

#[derive(Clone, Serialize, serde::Deserialize, PartialEq)]
pub(crate) struct DynShortcut {
    pub id: String,
    pub label: String,
    pub shortcut: String,
}

/// A tool reports its current runtime hotkeys with
/// `{"method":"shortcuts","params":[{id,label,shortcut}]}`. Hotkeys are
/// registered again when the list changes.
pub(crate) fn tool_shortcuts_changed(app: &AppHandle, id: &str, params: &Value) {
    let Ok(list) = serde_json::from_value::<Vec<DynShortcut>>(params.clone()) else {
        eprintln!("{id}: invalid shortcuts notification");
        return;
    };
    let list: Vec<DynShortcut> = list
        .into_iter()
        .filter(|s| !s.id.is_empty() && !s.id.contains("::") && !s.shortcut.trim().is_empty())
        .take(64)
        .collect();
    let hub = app.state::<Hub>();
    if hub.dyn_shortcuts(id) == list {
        return;
    }
    let path = hub.dyn_shortcuts_path(id);
    let res = if list.is_empty() {
        std::fs::remove_file(&path).or(Ok(()))
    } else {
        std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap())
    };
    if let Err(e) = res {
        eprintln!("{id}: could not save hotkeys: {e}");
        return;
    }
    shortcuts::rebind(app.clone());
}

pub(crate) async fn run_action(app: &AppHandle, id: &str, action: &str) -> Result<Value, String> {
    {
        let hub = app.state::<Hub>();
        let (_, m) = hub.load(id)?;
        if !m.actions.iter().any(|a| a.id == action)
            && !hub.dyn_shortcuts(id).iter().any(|s| s.id == action)
        {
            return Err(format!("{id} has no action {action:?}"));
        }
    }
    ensure_rpc(app, id).await?;
    app.state::<Hub>()
        .runtime
        .call(id, "action", json!({ "id": action }))
        .await
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
            .map(|entry| WorkshopItem {
                installed: installed.contains(&entry.id),
                entry,
            })
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
    // A tool installed outside tuxhub is updated where it is.
    let existing = tools::load(&app.state::<Hub>().tools_dir, &id).ok();
    if let Some(inst) = existing.filter(|i| matches!(i.entry.source, Source::External { .. })) {
        let hub = app.state::<Hub>();
        hub.runtime.stop(&id).await;
        embed::stop(&app, &id).await;
        tools::update_external(&app, &inst).await?;
        updates::forget(&app, &id);
        shortcuts::rebind(app.clone());
        start_background_tools(app).await;
        return Ok(());
    }
    let (entries, _) = workshop_entries().await;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("{id} is not in the workshop"))?;
    let hub = app.state::<Hub>();
    // Stop the running process before an update.
    hub.runtime.stop(&id).await;
    embed::stop(&app, &id).await;
    tools::install(&app, &hub.tools_dir, &entry).await?;
    updates::forget(&app, &id);
    shortcuts::rebind(app.clone());
    start_background_tools(app).await;
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
    /// The application of a tool that tuxhub only starts and stops
    /// (`embed = false`) is running.
    app_running: bool,
    can_launch: bool,
    /// Folder of the tool on disk (not for Flatpaks)
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
            app_running: app_is_running(&hub, &inst).await,
            level: manifest.level(),
            can_launch: manifest.run.is_some()
                || inst.detected_run.is_some()
                || matches!(
                    inst.entry.source,
                    Source::Flatpak { .. } | Source::SystemPackage { .. }
                ),
            path: inst
                .root_dir(&hub.tools_dir)
                .map(|p| p.to_string_lossy().into_owned()),
            version: inst.version.clone().or_else(|| manifest.version.clone()),
            entry: inst.entry,
            manifest,
        });
    }
    out
}

/// Whether the application of a tool that tuxhub only starts and stops, or
/// one with a live interface, is running, also when it was started outside
/// tuxhub.
async fn app_is_running(hub: &Hub, inst: &tools::Installed) -> bool {
    tools::app_running(&hub.tools_dir, inst)
        || (inst.manifest(&hub.tools_dir).live == Some(true) && hub.live.running(inst.id()).await)
}

/// Quits the application: a live application with `quit` first, and with
/// SIGTERM when it is still running after `wait`.
async fn quit_app(
    hub: &Hub,
    inst: &tools::Installed,
    wait: std::time::Duration,
) -> Result<(), String> {
    let id = inst.id();
    let program = tools::app_program(&hub.tools_dir, inst);
    let deadline = tokio::time::Instant::now() + wait;
    let asked = inst.manifest(&hub.tools_dir).live == Some(true) && hub.live.running(id).await;
    let quitting = asked && hub.live.quit(id, wait).await;
    let Some(program) = program else {
        return if asked && !quitting {
            Err(format!("{} did not quit", inst.entry.name))
        } else {
            Ok(())
        };
    };
    // The application stops listening first and may still save its settings:
    // give it the rest of the time before ending it.
    while quitting
        && !tools::processes(&program).is_empty()
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    tools::quit(&program).await
}

#[tauri::command]
async fn tool_launch(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    // An application that tuxhub only starts and stops runs once.
    if app_is_running(&hub, &inst).await {
        return Ok(());
    }
    tools::launch(&hub.tools_dir, &inst)
}

#[tauri::command]
async fn tool_app_running(app: AppHandle, id: String) -> Result<bool, String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    Ok(app_is_running(&hub, &inst).await)
}

/// Quits the application of a tool that tuxhub only starts and stops, or one
/// with a live interface.
#[tauri::command]
async fn tool_quit(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    if inst.manifest(&hub.tools_dir).live != Some(true)
        && tools::app_program(&hub.tools_dir, &inst).is_none()
    {
        return Err(format!("{id} is not started and stopped by tuxhub"));
    }
    quit_app(&hub, &inst, QUIT_WAIT).await
}

#[tauri::command]
async fn tool_uninstall(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    hub.runtime.stop(&id).await;
    embed::stop(&app, &id).await;
    quit_app(&hub, &inst, QUIT_WAIT).await?;
    tools::uninstall(&hub.tools_dir, &inst).await?;
    let _ = std::fs::remove_file(hub.settings_path(&id));
    let _ = std::fs::remove_file(hub.dyn_shortcuts_path(&id));
    updates::forget(&app, &id);
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
    tauri::async_runtime::spawn(start_background_tools(app));
    Ok(inst.entry.id)
}

#[tauri::command]
fn get_settings(app: AppHandle, id: String) -> Result<Map<String, Value>, String> {
    let hub = app.state::<Hub>();
    let (_, m) = hub.load(&id)?;
    Ok(hub.settings(&id, &m))
}

#[tauri::command]
async fn save_settings(
    app: AppHandle,
    id: String,
    values: Map<String, Value>,
) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let (_, m) = hub.load(&id)?;
    let mut merged = hub.settings(&id, &m);
    for (k, v) in values {
        if merged.contains_key(&k) {
            merged.insert(k, v);
        }
    }
    std::fs::write(
        hub.settings_path(&id),
        serde_json::to_string_pretty(&merged).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    if hub.runtime.is_running(&id).await {
        hub.runtime
            .call(&id, "configure", Value::Object(merged))
            .await?;
    }
    Ok(())
}

#[tauri::command]
async fn tool_action(app: AppHandle, id: String, action: String) -> Result<Value, String> {
    run_action(&app, &id, &action).await
}

/// Generic RPC call, used by tool interfaces through the JavaScript bridge.
#[tauri::command]
async fn tool_call(
    app: AppHandle,
    id: String,
    method: String,
    params: Value,
) -> Result<Value, String> {
    ensure_rpc(&app, &id).await?;
    app.state::<Hub>().runtime.call(&id, &method, params).await
}

#[tauri::command]
async fn tool_stop(app: AppHandle, id: String) {
    app.state::<Hub>().runtime.stop(&id).await;
}

// ---------------------------------------------------------- Live interface

/// The live interface is only used for installed tools with `live = true`.
fn live_tool(hub: &Hub, id: &str) -> Result<(), live::LiveError> {
    match hub.load(id) {
        Ok((_, m)) if m.live == Some(true) => Ok(()),
        _ => Err(live::LiveError::new(
            "not-live",
            format!("{id} has no live interface"),
        )),
    }
}

/// Languages the interface asks the application for.
fn live_locale(locale: &str) -> &'static str {
    if locale == "de" { "de" } else { "en" }
}

#[tauri::command]
async fn live_status(
    app: AppHandle,
    id: String,
    locale: String,
) -> Result<live::Status, live::LiveError> {
    let hub = app.state::<Hub>();
    live_tool(&hub, &id)?;
    Ok(hub.live.status(&id, live_locale(&locale)).await)
}

#[tauri::command]
async fn live_schema(
    app: AppHandle,
    id: String,
    locale: String,
) -> Result<live::Schema, live::LiveError> {
    let hub = app.state::<Hub>();
    live_tool(&hub, &id)?;
    hub.live.schema(&id, live_locale(&locale)).await
}

#[tauri::command]
async fn live_get(app: AppHandle, id: String) -> Result<Map<String, Value>, live::LiveError> {
    let hub = app.state::<Hub>();
    live_tool(&hub, &id)?;
    hub.live.values(&id).await
}

#[tauri::command]
async fn live_set(
    app: AppHandle,
    id: String,
    key: String,
    value: Value,
) -> Result<Value, live::LiveError> {
    let hub = app.state::<Hub>();
    live_tool(&hub, &id)?;
    hub.live.set(&id, &key, &value).await
}

#[tauri::command]
async fn live_action(app: AppHandle, id: String, action: String) -> Result<Value, live::LiveError> {
    let hub = app.state::<Hub>();
    live_tool(&hub, &id)?;
    hub.live.action(&id, &action).await
}

// ------------------------------------------------------- Hub & Updates

#[tauri::command]
fn hub_settings_get(app: AppHandle) -> system::HubSettings {
    app.state::<Hub>().settings.lock().unwrap().clone()
}

#[tauri::command]
fn hub_settings_set(app: AppHandle, settings: system::HubSettings) -> Result<(), String> {
    let hub = app.state::<Hub>();
    system::apply_autostart(&settings)?;
    settings
        .save(&hub.hub_settings_path)
        .map_err(|e| e.to_string())?;
    *hub.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn updates_list(app: AppHandle) -> std::collections::BTreeMap<String, String> {
    app.state::<Hub>().updates.lock().unwrap().clone()
}

#[tauri::command]
async fn updates_check(app: AppHandle) -> std::collections::BTreeMap<String, String> {
    updates::check_all(&app).await
}

#[tauri::command]
async fn tool_web_status(app: AppHandle, id: String) -> Result<bool, String> {
    let url = webui::web_url(&app, &id)?;
    Ok(webui::reachable(&url).await)
}

#[tauri::command]
fn tool_web_window(app: AppHandle, id: String) -> Result<(), String> {
    let name = tools::load(&app.state::<Hub>().tools_dir, &id)?.entry.name;
    webui::open_window(&app, &id, &name)
}

// --------------------------------------------------- Im Hub anzeigen

#[tauri::command]
fn embed_missing() -> Option<String> {
    embed::missing()
}

#[tauri::command]
async fn embed_start(app: AppHandle, id: String) -> Result<embed::EmbedInfo, String> {
    embed::start(&app, &id).await
}

#[tauri::command]
async fn embed_status(app: AppHandle, id: String) -> Option<embed::EmbedInfo> {
    embed::status(&app, &id).await
}

#[tauri::command]
async fn embed_stop(app: AppHandle, id: String) {
    embed::stop(&app, &id).await
}

#[tauri::command]
async fn shortcuts_configure() -> Result<(), String> {
    shortcuts::configure().await
}

#[tauri::command]
fn shortcut_status(app: AppHandle) -> shortcuts::Status {
    app.state::<Hub>().shortcuts.lock().unwrap().clone()
}

// ------------------------------------------------------- tool://-Protokoll

/// Serves files from the folder of a tool: `tool://<id>/<path>`.
fn serve_tool(app: &AppHandle, uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    let reply = |status: StatusCode, mime: &str, body: Vec<u8>| {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, mime)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            // Revalidate on every load, so an updated tool shows its new
            // interface without restarting tuxhub.
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body)
            .unwrap()
    };
    let not_found = || reply(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec());

    let id = uri.host().unwrap_or_default();
    let path = percent_decode(uri.path().trim_start_matches('/'));
    if path == "__tuxhub/bridge.js" {
        return reply(
            StatusCode::OK,
            "text/javascript",
            BRIDGE_JS.as_bytes().to_vec(),
        );
    }
    let hub = app.state::<Hub>();
    let Ok(inst) = tools::load(&hub.tools_dir, id) else {
        return not_found();
    };
    let Some(root) = inst
        .root_dir(&hub.tools_dir)
        .and_then(|r| r.canonicalize().ok())
    else {
        return not_found();
    };
    let Ok(file) = root.join(&path).canonicalize() else {
        return not_found();
    };
    // Never serve anything outside the tool folder.
    if !file.starts_with(&root) || !file.is_file() {
        return not_found();
    }
    match std::fs::read(&file) {
        Ok(body) => reply(StatusCode::OK, mime_for(&file), body),
        Err(_) => not_found(),
    }
}

fn mime_for(p: &Path) -> &'static str {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
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
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch only brings the window to the front.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            system::show_window(app)
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .register_uri_scheme_protocol("tool", |ctx, req| serve_tool(ctx.app_handle(), req.uri()))
        .register_asynchronous_uri_scheme_protocol("toolweb", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                responder.respond(webui::proxy(&app, req).await);
            });
        })
        .setup(|app| {
            let tools_dir = app.path().app_data_dir()?.join("tools");
            let config_dir = app.path().app_config_dir()?;
            let settings_dir = config_dir.join("settings");
            std::fs::create_dir_all(&tools_dir)?;
            std::fs::create_dir_all(&settings_dir)?;
            let hub_settings_path = config_dir.join("hub.json");
            let settings = system::HubSettings::load(&hub_settings_path);
            // Keep the autostart entry pointing to the running installation.
            if settings.autostart
                && let Err(e) = system::apply_autostart(&settings)
            {
                eprintln!("autostart: {e}");
            }
            // Notifications of live applications go to every window.
            let handle = app.handle().clone();
            let live = live::Live::new(
                std::env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute()),
                std::sync::Arc::new(move |event| {
                    let _ = handle.emit("live-event", event);
                }),
            );
            app.manage(Hub {
                tools_dir,
                settings_dir,
                hub_settings_path,
                runtime: native::Runtime::default(),
                shortcuts: Default::default(),
                settings: std::sync::Mutex::new(settings),
                updates: Default::default(),
                embeds: Default::default(),
                live,
            });

            if let Err(e) = system::setup_tray(app.handle()) {
                eprintln!("tray icon not available: {e}");
            }
            // The window starts hidden and stays hidden only when started with --minimized.
            if !std::env::args().any(|a| a == system::MINIMIZED_ARG) {
                system::show_window(app.handle());
            }
            shortcuts::rebind(app.handle().clone());
            updates::spawn_checker(app.handle().clone());
            tauri::async_runtime::spawn(start_background_tools(app.handle().clone()));
            let embed_dir = app
                .state::<Hub>()
                .tools_dir
                .parent()
                .map(|p| p.join("embed"));
            if let Some(dir) = embed_dir {
                tauri::async_runtime::spawn(async move { embed::cleanup_stale(&dir).await });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let to_tray = window.label() == "main"
                    && app.state::<Hub>().settings.lock().unwrap().close_to_tray
                    && app.try_state::<system::TrayItems>().is_some();
                if to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            workshop_list,
            workshop_readme,
            tool_install,
            tool_list,
            tool_launch,
            tool_app_running,
            tool_quit,
            tool_uninstall,
            tool_check_update,
            tool_link_local,
            get_settings,
            save_settings,
            tool_action,
            tool_call,
            tool_stop,
            shortcut_status,
            shortcuts_configure,
            hub_settings_get,
            hub_settings_set,
            updates_list,
            updates_check,
            tool_web_status,
            tool_web_window,
            embed_missing,
            embed_start,
            embed_status,
            embed_stop,
            live_status,
            live_schema,
            live_get,
            live_set,
            live_action,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                tauri::async_runtime::block_on(stop_children(app));
            }
        });
}

/// On exit: stops the tool processes and the sessions of applications shown
/// inside tuxhub.
async fn stop_children<R: tauri::Runtime>(app: &AppHandle<R>) {
    let hub = app.state::<Hub>();
    futures_util::join!(embed::stop_all(app), hub.runtime.stop_all());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes() {
        assert_eq!(percent_decode("a%20b.html"), "a b.html");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%2"), "%2");
    }

    /// Tool processes that ignore `shutdown` and the closed standard input are
    /// killed on exit, all within the same two seconds.
    #[tokio::test]
    async fn exit_stops_tool_processes() {
        let app = tauri::test::mock_app();
        let base = std::env::temp_dir().join(format!("tuxhub-exit-{}", std::process::id()));
        app.manage(Hub::for_test(base.join("tools"), base.join("settings")));
        let runtime = &app.state::<Hub>().runtime;
        // `sleep` never reads its standard input.
        let pids = [
            runtime.spawn_for_test("a", "sleep", &["30"]).await,
            runtime.spawn_for_test("b", "sleep", &["30"]).await,
        ];

        let started = std::time::Instant::now();
        stop_children(app.handle()).await;
        let alive: Vec<u32> = pids
            .into_iter()
            .filter(|pid| Path::new(&format!("/proc/{pid}")).exists())
            .collect();
        assert!(alive.is_empty(), "still running: {alive:?}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }

    /// A tool folder with a copy of `sleep` as its program, announced with
    /// `manifest`, and a hub whose live interface looks in `<base>/run`.
    fn hub_with_tool(name: &str, manifest: Value) -> (Hub, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("tuxhub-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let tool = base.join("tool");
        std::fs::create_dir_all(&tool).unwrap();
        std::fs::create_dir_all(base.join("tools/livedemo")).unwrap();
        std::fs::create_dir_all(base.join("run/tuxhub")).unwrap();
        std::fs::copy("/usr/bin/sleep", tool.join("app")).unwrap();
        let entry = json!({ "entry": {
            "id": "livedemo",
            "name": "Live Demo",
            "source": { "type": "local", "path": tool },
            "manifest": manifest
        }});
        std::fs::write(base.join("tools/livedemo/tool.json"), entry.to_string()).unwrap();
        let mut hub = Hub::for_test(base.join("tools"), base.join("settings"));
        hub.live = live::Live::new(Some(base.join("run")), std::sync::Arc::new(|_| {}));
        (hub, base, tool)
    }

    async fn exits_soon(child: &mut std::process::Child) -> bool {
        for _ in 0..200 {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let _ = child.kill();
        false
    }

    /// 7.15: a live application that answers `quit` but keeps running is
    /// ended as before, with SIGTERM.
    #[tokio::test]
    async fn quit_falls_back_to_ending_the_process() {
        use std::io::{BufRead, Write};
        let (hub, base, tool) = hub_with_tool(
            "quit",
            json!({ "embed": false, "live": true, "run": { "command": "app" } }),
        );
        let listener =
            std::os::unix::net::UnixListener::bind(base.join("run/tuxhub/livedemo.sock")).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut out = stream.try_clone().unwrap();
                    for line in std::io::BufReader::new(stream)
                        .lines()
                        .map_while(Result::ok)
                    {
                        let msg: Value = serde_json::from_str(&line).unwrap();
                        let result = match msg["method"].as_str() {
                            Some("hello") => json!({ "protocol": 1 }),
                            _ => json!(true),
                        };
                        let _ = writeln!(out, "{}", json!({ "id": msg["id"], "result": result }));
                    }
                });
            }
        });
        let mut child = tools::spawn_copied(&tool.join("app"), &["30"]).await;
        let inst = tools::load(&hub.tools_dir, "livedemo").unwrap();
        assert!(app_is_running(&hub, &inst).await);

        let started = std::time::Instant::now();
        quit_app(&hub, &inst, std::time::Duration::from_millis(300))
            .await
            .unwrap();
        assert!(exits_soon(&mut child).await, "the program still runs");
        assert!(started.elapsed() >= std::time::Duration::from_millis(300));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 7.15: an application that follows `quit` but needs a moment to exit
    /// (saving its settings) is not ended with SIGTERM.
    #[tokio::test]
    async fn quit_leaves_an_exiting_application_alone() {
        use std::io::{BufRead, Write};
        use std::os::unix::process::ExitStatusExt;
        let (hub, base, tool) = hub_with_tool(
            "quit-graceful",
            json!({ "embed": false, "live": true, "run": { "command": "app" } }),
        );
        let path = base.join("run/tuxhub/livedemo.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut out = stream.try_clone().unwrap();
                    for line in std::io::BufReader::new(stream)
                        .lines()
                        .map_while(Result::ok)
                    {
                        let msg: Value = serde_json::from_str(&line).unwrap();
                        let result = match msg["method"].as_str() {
                            Some("hello") => json!({ "protocol": 1 }),
                            _ => json!(true),
                        };
                        let _ = writeln!(out, "{}", json!({ "id": msg["id"], "result": result }));
                        if msg["method"] == "quit" {
                            // Stops listening and closes the connection; the
                            // process itself exits a second later.
                            let _ = std::fs::remove_file(&path);
                            return;
                        }
                    }
                });
            }
        });
        // `sleep 1` stands for the application that is still shutting down.
        let mut child = tools::spawn_copied(&tool.join("app"), &["1"]).await;
        let inst = tools::load(&hub.tools_dir, "livedemo").unwrap();
        assert!(app_is_running(&hub, &inst).await);

        quit_app(&hub, &inst, std::time::Duration::from_secs(3))
            .await
            .unwrap();
        assert!(exits_soon(&mut child).await, "the program still runs");
        let status = child.wait().unwrap();
        assert_eq!(status.signal(), None, "ended by a signal: {status:?}");
        assert!(status.success());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 7.1 and 7.15: without a socket, the program is ended directly.
    #[tokio::test]
    async fn quit_without_socket_ends_the_process() {
        let (hub, base, tool) = hub_with_tool(
            "quit-nosock",
            json!({ "embed": false, "live": true, "run": { "command": "app" } }),
        );
        let mut child = tools::spawn_copied(&tool.join("app"), &["30"]).await;
        let inst = tools::load(&hub.tools_dir, "livedemo").unwrap();
        assert!(app_is_running(&hub, &inst).await);
        quit_app(&hub, &inst, std::time::Duration::from_secs(3))
            .await
            .unwrap();
        assert!(exits_soon(&mut child).await, "the program still runs");
        assert!(!app_is_running(&hub, &inst).await);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The live commands only talk to tools that declare `live = true`.
    #[tokio::test]
    async fn live_commands_need_a_live_tool() {
        let (hub, base, _) = hub_with_tool("notlive", json!({ "embed": false }));
        assert_eq!(live_tool(&hub, "livedemo").unwrap_err().code, "not-live");
        assert_eq!(live_tool(&hub, "missing").unwrap_err().code, "not-live");
        let _ = std::fs::remove_dir_all(&base);
        let (hub, base, _) = hub_with_tool("islive", json!({ "live": true }));
        assert!(live_tool(&hub, "livedemo").is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }
}
