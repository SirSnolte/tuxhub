mod appstream;
mod embed;
mod github;
mod live;
mod local_apps;
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
/// Time a live service started by tuxhub has to open its socket.
const LIVE_START_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
/// Time a service started elsewhere gets after tuxhub's own start ended.
const STARTED_ELSEWHERE_WAIT: std::time::Duration = std::time::Duration::from_millis(1000);
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
    /// Version that ran before an update, until the interface has shown it.
    updated_from: std::sync::Mutex<Option<String>>,
    /// Connections to applications with a live interface
    live: live::Live,
    /// Tool id -> lock held while tuxhub starts its live service
    live_starts:
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
    /// flatpak, rpm and dpkg-query, which tell where apps are installed
    programs: tools::Programs,
    /// Installed apps found on this PC, with the newest application folder
    /// time and the `show_all` setting they were built for.
    local_apps: std::sync::Mutex<local_apps::Cache>,
    /// The folder of the entries the desktop starts at login.
    autostart_dir: PathBuf,
}

impl Hub {
    #[cfg(test)]
    pub(crate) fn for_test(tools_dir: PathBuf, settings_dir: PathBuf) -> Self {
        Self {
            hub_settings_path: settings_dir.join("hub.json"),
            autostart_dir: settings_dir.join("autostart"),
            tools_dir,
            settings_dir,
            runtime: native::Runtime::default(),
            shortcuts: Default::default(),
            settings: Default::default(),
            updates: Default::default(),
            embeds: Default::default(),
            updated_from: Default::default(),
            live: live::Live::new(None, std::sync::Arc::new(|_| {})),
            live_starts: Default::default(),
            programs: Default::default(),
            local_apps: Default::default(),
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
    if save_dyn_shortcuts(&app.state::<Hub>(), id, params) {
        shortcuts::rebind(app.clone());
    }
}

/// Saves the runtime hotkeys of a tool. `true` when they changed.
fn save_dyn_shortcuts(hub: &Hub, id: &str, params: &Value) -> bool {
    let Ok(list) = serde_json::from_value::<Vec<DynShortcut>>(params.clone()) else {
        eprintln!("{id}: invalid shortcuts notification");
        return false;
    };
    let list: Vec<DynShortcut> = list
        .into_iter()
        .filter(|s| !s.id.is_empty() && !s.id.contains("::") && !s.shortcut.trim().is_empty())
        .take(64)
        .collect();
    if hub.dyn_shortcuts(id) == list {
        return false;
    }
    let path = hub.dyn_shortcuts_path(id);
    let res = if list.is_empty() {
        std::fs::remove_file(&path).or(Ok(()))
    } else {
        std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap())
    };
    if let Err(e) = res {
        eprintln!("{id}: could not save hotkeys: {e}");
        return false;
    }
    true
}

/// Checks that `action` is declared in the manifest or reported as a runtime
/// hotkey. `true` for a live tool.
fn known_action(hub: &Hub, id: &str, action: &str) -> Result<bool, String> {
    let (_, m) = hub.load(id)?;
    if !m.actions.iter().any(|a| a.id == action)
        && !hub.dyn_shortcuts(id).iter().any(|s| s.id == action)
    {
        return Err(format!("{id} has no action {action:?}"));
    }
    Ok(m.is_live())
}

/// Runs an action of a live tool, after starting its service if needed.
async fn action_on(hub: &Hub, id: &str, action: &str) -> Result<Value, String> {
    if !known_action(hub, id, action)? {
        return Err(format!("{id} has no live interface"));
    }
    ensure_live(hub, id).await?;
    hub.live
        .call(id, "action", json!({ "id": action }))
        .await
        .map_err(|e| e.message)
}

/// Request of a tool page to its live service.
async fn call_on(hub: &Hub, id: &str, method: &str, params: Value) -> Result<Value, String> {
    live::bridge_method(method).map_err(|e| e.message)?;
    // Actions follow the same rules as from `tuxhub.action`.
    if method == "action" {
        let action = params.get("id").and_then(Value::as_str).unwrap_or_default();
        return action_on(hub, id, action).await;
    }
    ensure_live(hub, id).await?;
    hub.live
        .call(id, method, params)
        .await
        .map_err(|e| e.message)
}

/// Makes sure the live service of tool `id` listens: starts it from the
/// `[live]` section when its socket is missing.
async fn ensure_live(hub: &Hub, id: &str) -> Result<(), String> {
    ensure_live_within(hub, id, LIVE_START_WAIT).await
}

async fn ensure_live_within(hub: &Hub, id: &str, wait: std::time::Duration) -> Result<(), String> {
    let (inst, m) = hub.load(id)?;
    if !m.is_live() {
        return Err(format!("{id} has no live interface"));
    }
    if hub.live.running(id).await {
        return Ok(());
    }
    let name = &inst.entry.name;
    let Some(start) = m.live_start() else {
        return Err(format!("{name} is not running"));
    };
    // One start at a time; whoever waited finds the service running.
    let lock = hub
        .live_starts
        .lock()
        .unwrap()
        .entry(id.to_string())
        .or_default()
        .clone();
    let _starting = lock.lock().await;
    if hub.live.running(id).await {
        return Ok(());
    }
    let root = inst
        .root_dir(&hub.tools_dir)
        .ok_or("the tool has no folder")?;
    let exe = root.join(&start.command);
    if !exe.is_file() {
        return Err(format!("{} is missing", exe.display()));
    }
    // Without a shell, in its own process group: the service keeps running
    // when tuxhub exits.
    let mut child = tokio::process::Command::new(&exe)
        .args(&start.args)
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("{name} does not start: {e}"))?;
    let deadline = tokio::time::Instant::now() + wait;
    let mut exited = None;
    let result = loop {
        if hub.live.running(id).await {
            break Ok(());
        }
        // A service that ended without listening will not come, unless
        // another instance (own window, login) is just starting: it held the
        // lock, so give it a moment to open the socket.
        if exited.is_none() && matches!(child.try_wait(), Ok(Some(_))) {
            exited = Some(tokio::time::Instant::now());
        }
        if exited.is_some_and(|t| t.elapsed() >= STARTED_ELSEWHERE_WAIT) {
            break Err(format!("{name} does not start"));
        }
        if tokio::time::Instant::now() >= deadline {
            break Err(format!("{name} does not start"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    // Collect the exit status whenever the service ends.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    result
}

pub(crate) async fn run_action(app: &AppHandle, id: &str, action: &str) -> Result<Value, String> {
    if known_action(&app.state::<Hub>(), id, action)? {
        return action_on(&app.state::<Hub>(), id, action).await;
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
    /// On this computer, but not in tuxhub: it can be added to the sidebar.
    present: bool,
    /// How deeply tuxhub integrates the app; none when the workshop does
    /// not say and the app is not installed.
    integration: Option<tools::Integration>,
}

/// The workshop entries with their state. Installations are looked for side
/// by side, so a hanging query delays the list by the timeout at most.
async fn workshop_items(hub: &Hub, entries: Vec<Entry>) -> Vec<WorkshopItem> {
    let records = tools::load_all(&hub.tools_dir);
    let installed: Vec<String> = records.iter().map(|t| t.entry.id.clone()).collect();
    let present = futures_util::future::join_all(entries.iter().map(|e| async {
        !installed.contains(&e.id) && tools::detect(e, &hub.programs).await.any()
    }))
    .await;
    entries
        .into_iter()
        .zip(present)
        .map(|(entry, present)| {
            let record = records.iter().find(|t| t.entry.id == entry.id);
            WorkshopItem {
                installed: record.is_some(),
                integration: workshop_integration(&hub.tools_dir, &entry, record),
                present,
                entry,
            }
        })
        .collect()
}

/// The integration a workshop card shows: the installed app's manifest
/// wins over what the workshop says.
fn workshop_integration(
    tools_dir: &Path,
    entry: &Entry,
    installed: Option<&Installed>,
) -> Option<tools::Integration> {
    let Some(inst) = installed else {
        return entry.integration;
    };
    let actual = tools::integration_of(&inst.manifest(tools_dir));
    if let Some(listed) = entry.integration
        && listed != actual
    {
        eprintln!(
            "{}: the workshop says {listed:?}, the installed app is {actual:?}",
            entry.id
        );
    }
    Some(actual)
}

#[tauri::command]
async fn workshop_list(app: AppHandle) -> WorkshopView {
    let (entries, live) = workshop_entries().await;
    tools::refresh_entries(&app.state::<Hub>().tools_dir, &entries);
    WorkshopView {
        entries: workshop_items(&app.state::<Hub>(), entries).await,
        live,
    }
}

#[tauri::command]
async fn workshop_readme(repo: String) -> Result<String, String> {
    github::readme(&repo).await
}

/// The result of an install or update: where the app ran before (the user
/// interface starts it there again) and why the update failed, if it did.
#[derive(Debug, Serialize)]
struct InstallOutcome {
    state: RunState,
    error: Option<String>,
}

/// Quits a running app, then updates it. Without `restart` (the user's yes
/// to quit and restart) a running app is left alone. When quitting fails,
/// nothing is updated; when the update fails, the old version stays and the
/// app is still to be started again.
async fn update_running<Q, QF, U, UF>(
    before: RunState,
    restart: bool,
    quit: Q,
    update: U,
) -> Result<InstallOutcome, String>
where
    Q: FnOnce() -> QF,
    QF: std::future::Future<Output = Result<(), String>>,
    U: FnOnce() -> UF,
    UF: std::future::Future<Output = Result<(), String>>,
{
    if before != RunState::Stopped {
        if !restart {
            return Err("the app is running; quit it before the update".into());
        }
        quit().await?;
    }
    Ok(InstallOutcome {
        state: before,
        error: update().await.err(),
    })
}

/// Installs or updates a tool. A running app is quit first when `restart`
/// says the user agreed; the returned state tells where it ran.
#[tauri::command]
async fn tool_install(app: AppHandle, id: String, restart: bool) -> Result<InstallOutcome, String> {
    let hub = app.state::<Hub>();
    let existing = tools::load(&hub.tools_dir, &id).ok();
    // A tool installed outside tuxhub is updated where it is.
    let external = existing
        .clone()
        .filter(|i| matches!(i.entry.source, Source::External { .. }));
    // An app adopted from the package manager is updated by it, never from
    // the workshop.
    let distro = existing
        .clone()
        .filter(|i| matches!(i.entry.source, Source::DistroPackage { .. }));
    let entry = match external.as_ref().or(distro.as_ref()) {
        Some(_) => None,
        None => {
            let (entries, _) = workshop_entries().await;
            Some(
                entries
                    .into_iter()
                    .find(|e| e.id == id)
                    .ok_or_else(|| format!("{id} is not in the workshop"))?,
            )
        }
    };
    let before = match &existing {
        Some(inst) => run_state(&hub, embed::status(&app, &id).await.is_some(), inst).await,
        None => RunState::Stopped,
    };
    let quit = || async {
        match (before, &existing) {
            (RunState::Hub, _) => embed::stop(&app, &id).await,
            (_, Some(inst)) => quit_running(&hub, inst, QUIT_WAIT).await?,
            _ => {}
        }
        Ok(())
    };
    let update = || async {
        // Background processes stop for the update and start again below.
        hub.runtime.stop(&id).await;
        embed::stop(&app, &id).await;
        match (&external, &entry) {
            (Some(inst), _) => tools::update_external(&app, inst).await?,
            (None, None) if distro.is_some() => {
                let inst = distro.as_ref().unwrap();
                tools::update_distro_package(&hub.tools_dir, &hub.programs, inst).await?;
            }
            (None, Some(entry)) => {
                install_entry(&app, &hub, entry).await?;
            }
            (None, None) => return Err(format!("{id} is not in the workshop")),
        }
        updates::forget(&app, &id);
        shortcuts::rebind(app.clone());
        Ok(())
    };
    let outcome = update_running(before, restart, quit, update).await;
    start_background_tools(app.clone()).await;
    outcome
}

/// Installs or updates a workshop entry. An app that is already on this
/// computer but not in tuxhub is never installed a second time.
async fn install_entry<R: tauri::Runtime>(
    app: &AppHandle<R>,
    hub: &Hub,
    entry: &Entry,
) -> Result<Installed, String> {
    let previous = tools::load(&hub.tools_dir, &entry.id).ok();
    let found = tools::detect(previous.as_ref().map_or(entry, |p| &p.entry), &hub.programs).await;
    match &previous {
        None if found.any() => {
            return Err(format!(
                "{} is already on this computer; add it to the sidebar",
                entry.name
            ));
        }
        // A Flatpak in tuxhub is updated where it is installed, never
        // installed a second time.
        Some(inst)
            if matches!(inst.entry.source, Source::Flatpak { .. }) && !found.flatpak.is_empty() =>
        {
            let mut inst = inst.clone();
            if !found.flatpak.contains(&inst.installation) {
                inst.installation = found.flatpak[0];
                inst.adopted = true;
            }
            return tools::update_flatpak(app, &hub.tools_dir, &hub.programs, &inst, entry).await;
        }
        _ => {}
    }
    let inst = tools::install(app, &hub.tools_dir, &hub.programs, entry).await?;
    record_after_install(hub, previous.as_ref(), found.package, inst)
}

/// Stores the record of a new installation with what it keeps from the
/// previous one (see [`carry_over`]).
fn record_after_install(
    hub: &Hub,
    previous: Option<&Installed>,
    present: bool,
    inst: Installed,
) -> Result<Installed, String> {
    let inst = carry_over(previous, present, inst);
    tools::save(&hub.tools_dir, &inst)?;
    Ok(inst)
}

/// The record after installing over `previous`: the update of a system
/// package that was on the computer before tuxhub stays adopted.
fn carry_over(previous: Option<&Installed>, present: bool, mut new: Installed) -> Installed {
    if let Some(p) = previous
        && p.adopted
        && present
        && matches!(new.entry.source, Source::SystemPackage { .. })
    {
        new.adopted = true;
    }
    if let Some(p) = previous {
        new.window = p.window;
    }
    new
}

/// Adds a workshop app that is already on this computer to the sidebar.
async fn adopt_entry(hub: &Hub, entries: Vec<Entry>, id: &str) -> Result<Installed, String> {
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("{id} is not in the workshop"))?;
    if tools::load(&hub.tools_dir, id).is_ok() {
        return Err(format!("{} is already in tuxhub", entry.name));
    }
    tools::adopt(&hub.tools_dir, &hub.programs, &entry).await
}

#[tauri::command]
async fn tool_adopt(app: AppHandle, id: String) -> Result<(), String> {
    let (entries, _) = workshop_entries().await;
    adopt_entry(&app.state::<Hub>(), entries, &id).await?;
    shortcuts::rebind(app.clone());
    start_background_tools(app).await;
    Ok(())
}

// ---------------------------------------------------------- Installed apps

/// An app of the start menu with what tuxhub knows about it.
#[derive(Serialize)]
struct LocalAppView {
    app: local_apps::LocalApp,
    /// The id it has in tuxhub: the catalog's id, or its own `local.…` id.
    tool_id: String,
    in_sidebar: bool,
    /// A newer version, found by the update check.
    update: Option<String>,
    /// Where it runs; `stopped` for apps that are not in the sidebar.
    run_state: RunState,
    /// How deeply tuxhub integrates it; none when the catalog does not say.
    integration: Option<tools::Integration>,
}

/// The catalog the installed apps are matched with. It is the bundled
/// index: opening the workshop's installed apps needs no network.
fn local_catalog() -> Vec<Entry> {
    parse_workshop(WORKSHOP_BUNDLED).unwrap_or_default()
}

/// The id an app of this computer has in tuxhub.
fn local_tool_id(app: &local_apps::LocalApp, catalog: &[Entry]) -> String {
    match local_apps::catalog_entry(catalog, &app.source) {
        Some(i) => catalog[i].id.clone(),
        None => local_apps::local_id(&app.desktop_id),
    }
}

/// Whether the start menu entry starts a tool tuxhub installed: such tools
/// write their own entries, but they already are tuxhub's tools. A path with
/// `.` or `..` in it never counts as inside the tools folder.
fn is_own_tool(hub: &Hub, app: &local_apps::LocalApp) -> bool {
    use std::path::Component;
    app.command.first().is_some_and(|p| {
        let path = Path::new(p);
        path.components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
            && path.starts_with(&hub.tools_dir)
    })
}

/// The apps with their state in tuxhub; `in_hub`: tools shown in an xpra
/// session.
async fn local_app_views(
    hub: &Hub,
    apps: Vec<local_apps::LocalApp>,
    catalog: &[Entry],
    in_hub: &std::collections::HashSet<String>,
) -> Vec<LocalAppView> {
    let records = tools::load_all(&hub.tools_dir);
    let updates = hub.updates.lock().unwrap().clone();
    let apps = apps.into_iter().filter(|a| !is_own_tool(hub, a));
    let views = apps.map(|app| {
        let tool_id = local_tool_id(&app, catalog);
        let record = records.iter().find(|r| r.entry.id == tool_id);
        let update = updates.get(&tool_id).cloned();
        let integration = match catalog.iter().find(|e| e.id == tool_id) {
            Some(entry) => workshop_integration(&hub.tools_dir, entry, record),
            // Without a manifest an app runs inside tuxhub or in its own window.
            None => Some(record.map_or(tools::Integration::Embedded, |inst| {
                tools::integration_of(&inst.manifest(&hub.tools_dir))
            })),
        };
        async move {
            let run_state = match record {
                Some(inst) => run_state(hub, in_hub.contains(&tool_id), inst).await,
                None => RunState::Stopped,
            };
            LocalAppView {
                app,
                in_sidebar: record.is_some(),
                tool_id,
                update,
                run_state,
                integration,
            }
        }
    });
    futures_util::future::join_all(views).await
}

#[tauri::command]
async fn local_apps_list(app: AppHandle, show_all: bool) -> Vec<LocalAppView> {
    let hub = app.state::<Hub>();
    let catalog = local_catalog();
    let apps = local_apps::list_cached(&hub.local_apps, &hub.programs, &catalog, show_all).await;
    let mut in_hub = std::collections::HashSet::new();
    for inst in tools::load_all(&hub.tools_dir) {
        if embed::status(&app, inst.id()).await.is_some() {
            in_hub.insert(inst.entry.id);
        }
    }
    local_app_views(&hub, apps, &catalog, &in_hub).await
}

/// Longest desktop file id accepted from the user interface.
const MAX_DESKTOP_ID: usize = 255;

/// Adds an app of `apps` to the sidebar, found by its desktop file id: an
/// app the catalog knows the way the workshop adds it, any other with the
/// command of its desktop entry. An app that starts at login opens in its
/// own window, as it already runs.
async fn add_local_app(
    hub: &Hub,
    apps: &[local_apps::LocalApp],
    catalog: Vec<Entry>,
    desktop_id: &str,
) -> Result<Installed, String> {
    let app = (desktop_id.len() <= MAX_DESKTOP_ID)
        .then(|| apps.iter().find(|a| a.desktop_id == desktop_id))
        .flatten()
        .filter(|a| !is_own_tool(hub, a))
        .ok_or("this is not an app on this computer")?;
    let mut inst = match local_apps::catalog_entry(&catalog, &app.source) {
        Some(i) => {
            let id = catalog[i].id.clone();
            adopt_entry(hub, catalog, &id).await?
        }
        None => {
            let entry = local_apps::local_entry(app);
            if tools::load(&hub.tools_dir, &entry.id).is_ok() {
                return Err(format!("{} is already in tuxhub", entry.name));
            }
            tools::adopt(&hub.tools_dir, &hub.programs, &entry).await?
        }
    };
    if app.autostart {
        inst.window = tools::WindowMode::Own;
        tools::save(&hub.tools_dir, &inst)?;
    }
    Ok(inst)
}

#[tauri::command]
async fn local_app_add(app: AppHandle, desktop_id: String) -> Result<String, String> {
    let hub = app.state::<Hub>();
    let catalog = local_catalog();
    // The list the user chose from; usually cached, so nothing is scanned.
    let show_all = hub
        .local_apps
        .lock()
        .ok()
        .and_then(|c| c.as_ref().map(|(_, all, _)| *all))
        .unwrap_or(true);
    let apps = local_apps::list_cached(&hub.local_apps, &hub.programs, &catalog, show_all).await;
    let inst = add_local_app(&hub, &apps, catalog, &desktop_id).await?;
    shortcuts::rebind(app.clone());
    start_background_tools(app).await;
    Ok(inst.entry.id)
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
    /// The app was on this computer before; tuxhub only added it.
    adopted: bool,
    /// The Flatpak installation tuxhub uses.
    installation: tools::Installation,
    /// The app is no longer installed on this computer.
    missing: bool,
    /// The Flatpak is installed for the user and system-wide, and the user
    /// has not chosen to keep both.
    duplicate: bool,
    /// How deeply tuxhub integrates the app.
    integration: tools::Integration,
    /// Where an embedded app runs when it is started.
    window: tools::WindowMode,
    /// The app was on this computer before and starts at login.
    autostart: bool,
}

#[tauri::command]
async fn tool_list(app: AppHandle) -> Vec<ToolView> {
    tool_views(&app.state::<Hub>()).await
}

async fn tool_views(hub: &Hub) -> Vec<ToolView> {
    let mut installed = tools::load_all(&hub.tools_dir);
    let states = futures_util::future::join_all(
        installed
            .iter_mut()
            .map(|inst| tools::check_installation(&hub.tools_dir, &hub.programs, inst)),
    )
    .await;
    let mut out = Vec::new();
    for (inst, state) in installed.into_iter().zip(states) {
        let manifest = inst.manifest(&hub.tools_dir);
        out.push(ToolView {
            adopted: inst.adopted,
            installation: inst.installation,
            missing: state.missing,
            duplicate: state.duplicate,
            running: hub.runtime.is_running(inst.id()).await,
            app_running: app_is_running(hub, &inst).await,
            level: manifest.level(),
            can_launch: can_launch(&inst, &manifest),
            integration: tools::integration_of(&manifest),
            window: inst.window,
            autostart: inst.adopted && starts_at_login(&inst, &hub.autostart_dir),
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

/// Whether an autostart entry starts the app: found by its Flatpak ID, or
/// by the program of its start command.
fn starts_at_login(inst: &Installed, autostart_dir: &Path) -> bool {
    match &inst.entry.source {
        Source::Flatpak { flatpak_id, .. } => {
            local_apps::autostart_starts(Some(flatpak_id), None, autostart_dir)
        }
        Source::DistroPackage { command, .. } | Source::Launcher { command, .. } => {
            local_apps::autostart_starts(None, command.first().map(String::as_str), autostart_dir)
        }
        _ => false,
    }
}

/// Whether tuxhub can start the tool as an application.
fn can_launch(inst: &Installed, manifest: &Manifest) -> bool {
    manifest.run.is_some()
        || inst.detected_run.is_some()
        || matches!(
            inst.entry.source,
            Source::Flatpak { .. }
                | Source::SystemPackage { .. }
                | Source::DistroPackage { .. }
                | Source::Launcher { .. }
        )
}

/// Whether the app has the "In tuxhub / Own window" toggle: an app that runs
/// per xpra, recorded by tuxhub (an announced tool has no record to keep it).
fn has_window_toggle(inst: &Installed, manifest: &Manifest) -> bool {
    tools::integration_of(manifest) == tools::Integration::Embedded
        && manifest.level() == 0
        && can_launch(inst, manifest)
        && !matches!(inst.entry.source, Source::External { .. })
}

/// Stores where an embedded app runs. Starts and stops nothing.
fn set_window(hub: &Hub, id: &str, mode: &str) -> Result<(), String> {
    let mode: tools::WindowMode = serde_json::from_value(Value::String(mode.into()))
        .map_err(|_| format!("unknown window mode {mode:?}"))?;
    let mut inst = tools::load(&hub.tools_dir, id)?;
    if !has_window_toggle(&inst, &inst.manifest(&hub.tools_dir)) {
        return Err(format!("{} always runs the same way", inst.entry.name));
    }
    inst.window = mode;
    tools::save(&hub.tools_dir, &inst)
}

#[tauri::command]
fn tool_set_window(app: AppHandle, id: String, mode: String) -> Result<(), String> {
    set_window(&app.state::<Hub>(), &id, &mode)
}

/// Whether and where an app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum RunState {
    Stopped,
    /// Inside tuxhub (xpra session).
    Hub,
    /// In its own window, or a socket app.
    Own,
    /// It could not be found out (flatpak did not answer).
    Unknown,
}

/// Where the app runs; `in_hub`: tuxhub shows it in an xpra session.
async fn run_state(hub: &Hub, in_hub: bool, inst: &Installed) -> RunState {
    if in_hub {
        return RunState::Hub;
    }
    // Its processes cannot be told from others that run the same launcher.
    if tools::shared_launcher(inst) {
        return RunState::Unknown;
    }
    if let Source::Flatpak { flatpak_id, .. } = &inst.entry.source
        && !inst.manifest(&hub.tools_dir).is_live()
    {
        return match tools::flatpak_running(&hub.programs, flatpak_id).await {
            Some(true) => RunState::Own,
            Some(false) => RunState::Stopped,
            None => RunState::Unknown,
        };
    }
    // A service from the `[live]` section runs without a window; it is not
    // the app the user would lose work in.
    if window_is_open(hub, inst).await {
        RunState::Own
    } else {
        RunState::Stopped
    }
}

#[tauri::command]
async fn tool_run_state(app: AppHandle, id: String) -> Result<RunState, String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    let in_hub = embed::status(&app, &id).await.is_some();
    Ok(run_state(&hub, in_hub, &inst).await)
}

/// Whether the application of a tool that tuxhub only starts and stops, or
/// one with a live interface, is running, also when it was started outside
/// tuxhub.
async fn app_is_running(hub: &Hub, inst: &tools::Installed) -> bool {
    tools::app_running(&hub.tools_dir, inst)
        || (inst.manifest(&hub.tools_dir).is_live() && hub.live.running(inst.id()).await)
}

/// Whether the tool's own window already runs, so "Open own window" has
/// nothing to start. A service from the `[live]` section runs without a
/// window, so only `live = true` counts its socket as the open app.
async fn window_is_open(hub: &Hub, inst: &tools::Installed) -> bool {
    let manifest = inst.manifest(&hub.tools_dir);
    tools::app_running(&hub.tools_dir, inst)
        || (manifest.is_live()
            && manifest.live_start().is_none()
            && hub.live.running(inst.id()).await)
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
    let asked = inst.manifest(&hub.tools_dir).is_live() && hub.live.running(id).await;
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

/// Quits the running app the user asked to quit (or to update): like
/// [`quit_app`], and a Flatpak with `flatpak kill`.
async fn quit_running(
    hub: &Hub,
    inst: &tools::Installed,
    wait: std::time::Duration,
) -> Result<(), String> {
    if let Source::Flatpak { flatpak_id, .. } = &inst.entry.source
        && !inst.manifest(&hub.tools_dir).is_live()
    {
        return tools::flatpak_kill(&hub.programs, flatpak_id, wait).await;
    }
    quit_app(hub, inst, wait).await
}

#[tauri::command]
async fn tool_launch(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    // An application that tuxhub only starts and stops runs once.
    if window_is_open(&hub, &inst).await {
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
    quit_tool(&hub, &inst, QUIT_WAIT).await
}

/// Quits the app when tuxhub knows how ([`can_quit`]), waiting up to `wait`.
async fn quit_tool(hub: &Hub, inst: &Installed, wait: std::time::Duration) -> Result<(), String> {
    can_quit(hub, inst)?;
    quit_running(hub, inst, wait).await
}

/// Whether tuxhub knows how to quit the app: by its live interface, its own
/// program or as a Flatpak.
fn can_quit(hub: &Hub, inst: &Installed) -> Result<(), String> {
    if !inst.manifest(&hub.tools_dir).is_live()
        && tools::app_program(&hub.tools_dir, inst).is_none()
        && !matches!(inst.entry.source, Source::Flatpak { .. })
    {
        return Err(format!(
            "{} is not started and stopped by tuxhub",
            inst.id()
        ));
    }
    Ok(())
}

#[tauri::command]
async fn tool_uninstall(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    // Before anything is stopped: an app tuxhub may not remove keeps running.
    tools::check_removable(&inst)?;
    hub.runtime.stop(&id).await;
    embed::stop(&app, &id).await;
    quit_app(&hub, &inst, QUIT_WAIT).await?;
    tools::uninstall(&hub.tools_dir, &hub.programs, &inst).await?;
    remove_tool_data(&hub, &id);
    updates::forget(&app, &id);
    shortcuts::rebind(app.clone());
    Ok(())
}

/// Removes the settings and hotkeys tuxhub keeps for a tool.
fn remove_tool_data(hub: &Hub, id: &str) {
    let _ = std::fs::remove_file(hub.settings_path(id));
    let _ = std::fs::remove_file(hub.dyn_shortcuts_path(id));
}

/// Removes a Flatpak or system package tool from tuxhub only: its app stays
/// on this computer and nothing is run.
fn forget(hub: &Hub, id: &str) -> Result<(), String> {
    let inst = tools::load(&hub.tools_dir, id)?;
    let dir = hub.tools_dir.join(id);
    if !matches!(
        inst.entry.source,
        Source::Flatpak { .. }
            | Source::SystemPackage { .. }
            | Source::DistroPackage { .. }
            | Source::Launcher { .. }
    ) || !dir.join("tool.json").is_file()
    {
        return Err(format!(
            "{} can only be uninstalled, not just removed from tuxhub",
            inst.entry.name
        ));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    remove_tool_data(hub, id);
    Ok(())
}

#[tauri::command]
async fn tool_forget(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    hub.runtime.stop(&id).await;
    embed::stop(&app, &id).await;
    forget(&hub, &id)?;
    updates::forget(&app, &id);
    shortcuts::rebind(app.clone());
    Ok(())
}

/// Removes the user copy of a Flatpak that is also installed system-wide.
/// tuxhub then uses the system installation; settings and hotkeys stay.
async fn remove_copy(hub: &Hub, id: &str) -> Result<(), String> {
    let mut inst = tools::load(&hub.tools_dir, id)?;
    let Source::Flatpak { flatpak_id, .. } = &inst.entry.source else {
        return Err(format!("{} is not a Flatpak", inst.entry.name));
    };
    let flatpak_id = flatpak_id.clone();
    if tools::detect(&inst.entry, &hub.programs)
        .await
        .flatpak
        .len()
        < 2
    {
        return Err(format!("{} is installed only once", inst.entry.name));
    }
    tools::flatpak_uninstall(&hub.programs, &flatpak_id, tools::Installation::User).await?;
    inst.installation = tools::Installation::System;
    inst.adopted = true;
    tools::save(&hub.tools_dir, &inst)
}

#[tauri::command]
async fn tool_remove_copy(app: AppHandle, id: String) -> Result<(), String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    embed::stop(&app, &id).await;
    quit_app(&hub, &inst, QUIT_WAIT).await?;
    remove_copy(&hub, &id).await
}

/// Keeps both Flatpak installations: the hint about the copy stays hidden.
fn dismiss_duplicate(hub: &Hub, id: &str) -> Result<(), String> {
    let mut inst = tools::load(&hub.tools_dir, id)?;
    if !matches!(inst.entry.source, Source::Flatpak { .. }) {
        return Err(format!("{} is not a Flatpak", inst.entry.name));
    }
    inst.duplicate_dismissed = true;
    tools::save(&hub.tools_dir, &inst)
}

#[tauri::command]
fn tool_dismiss_duplicate(app: AppHandle, id: String) -> Result<(), String> {
    dismiss_duplicate(&app.state::<Hub>(), &id)
}

#[tauri::command]
async fn tool_check_update(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    tools::check_update(&inst, &hub.programs).await
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
    let hub = app.state::<Hub>();
    if hub.load(&id)?.1.is_live() {
        return call_on(&hub, &id, &method, params).await;
    }
    ensure_rpc(&app, &id).await?;
    hub.runtime.call(&id, &method, params).await
}

#[tauri::command]
async fn tool_stop(app: AppHandle, id: String) {
    app.state::<Hub>().runtime.stop(&id).await;
}

// ---------------------------------------------------------- Live interface

/// The live interface is only used for installed tools with `live = true`.
fn live_tool(hub: &Hub, id: &str) -> Result<(), live::LiveError> {
    match hub.load(id) {
        Ok((_, m)) if m.is_live() => Ok(()),
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

/// A request of tuxhub's own menu for an application; only the methods of
/// `live::menu_call` pass.
#[tauri::command]
async fn live_call(
    app: AppHandle,
    id: String,
    method: String,
    params: Value,
) -> Result<Value, live::LiveError> {
    let hub = app.state::<Hub>();
    live_tool(&hub, &id)?;
    live::menu_call(&method, &params)?;
    hub.live.call(&id, &method, params).await
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

/// The version tuxhub was updated from, once per update.
#[tauri::command]
fn self_updated_from(app: AppHandle) -> Option<String> {
    app.state::<Hub>().updated_from.lock().unwrap().take()
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

/// Shows the app inside tuxhub. An app that runs outside tuxhub fails with
/// [`embed::RUNNING_OUTSIDE`]; with `restart` (the user's yes) it is quit
/// first and started here.
#[tauri::command]
async fn embed_start(
    app: AppHandle,
    id: String,
    restart: bool,
) -> Result<embed::EmbedInfo, String> {
    let hub = app.state::<Hub>();
    let inst = tools::load(&hub.tools_dir, &id)?;
    embed::start(
        &app,
        &id,
        &inst.entry.name,
        restart,
        || run_state(&hub, false, &inst),
        // An app like a daemon may need longer than `QUIT_WAIT` to quit.
        || quit_tool(&hub, &inst, embed::STOP_WAIT),
    )
    .await
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

/// Icons of installed apps (`appicon://localhost/<path>`): only PNG and SVG
/// files inside the icon folders of the XDG data folders (see
/// [`local_apps::read_icon`]). They are served as inert images: no script of
/// an SVG runs, and the type is never guessed.
fn serve_app_icon(uri: &tauri::http::Uri) -> Response<Vec<u8>> {
    serve_app_icon_in(uri, &local_apps::data_dirs())
}

fn serve_app_icon_in(uri: &tauri::http::Uri, dirs: &[PathBuf]) -> Response<Vec<u8>> {
    let path = PathBuf::from(percent_decode(uri.path()));
    let builder = Response::builder()
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; style-src 'unsafe-inline'; sandbox",
        )
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    match local_apps::read_icon(&path, dirs) {
        Some(body) => builder
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_for(&path))
            .body(body)
            .unwrap(),
        None => builder
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(b"not found".to_vec())
            .unwrap(),
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
        .register_uri_scheme_protocol("appicon", |_ctx, req| serve_app_icon(req.uri()))
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
            let updated_from = system::record_version(
                &config_dir.join("version"),
                &app.package_info().version.to_string(),
            );
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
                std::sync::Arc::new(move |event: live::LiveEvent| {
                    if event.method == "shortcuts" {
                        tool_shortcuts_changed(&handle, &event.tool, &event.params);
                    }
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
                updated_from: std::sync::Mutex::new(updated_from.clone()),
                live,
                live_starts: Default::default(),
                programs: Default::default(),
                local_apps: Default::default(),
                autostart_dir: local_apps::autostart_dir(),
            });

            if let Err(e) = system::setup_tray(app.handle()) {
                eprintln!("tray icon not available: {e}");
            }
            // The window starts hidden and stays hidden only when started with --minimized.
            // After an update it is shown anyway: the restart keeps the arguments of the
            // autostart, and a window that disappears looks like a crash.
            if updated_from.is_some() || !std::env::args().any(|a| a == system::MINIMIZED_ARG) {
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
            tool_adopt,
            local_apps_list,
            local_app_add,
            tool_remove_copy,
            tool_dismiss_duplicate,
            tool_forget,
            tool_list,
            tool_launch,
            tool_app_running,
            tool_set_window,
            tool_run_state,
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
            self_updated_from,
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
            live_call,
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

    /// App icons are served as inert images: no other origin may read them,
    /// and an SVG can run nothing.
    #[test]
    fn app_icons_are_served_as_inert_images() {
        let base = std::env::temp_dir().join(format!("tuxhub-appicon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let svg = base.join("share/icons/hicolor/scalable/apps/a.svg");
        std::fs::create_dir_all(svg.parent().unwrap()).unwrap();
        std::fs::write(&svg, "<svg/>").unwrap();
        let dirs = [base.join("share")];
        let uri = |p: &Path| local_apps::icon_url(p).parse::<tauri::http::Uri>().unwrap();
        for (path, status) in [
            (svg.clone(), StatusCode::OK),
            (base.join("share/icons/missing.png"), StatusCode::NOT_FOUND),
        ] {
            let res = serve_app_icon_in(&uri(&path), &dirs);
            assert_eq!(res.status(), status);
            let h = res.headers();
            assert!(h.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
            assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
            assert_eq!(
                h[header::CONTENT_SECURITY_POLICY],
                "default-src 'none'; style-src 'unsafe-inline'; sandbox"
            );
        }
        let res = serve_app_icon_in(&uri(&svg), &dirs);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/svg+xml");
        assert_eq!(res.body(), b"<svg/>");
        std::fs::remove_dir_all(&base).unwrap();
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

    /// A running `[live]` service is not the tool's window: "Open own window"
    /// still starts `[run]`. With `live = true` the live socket is the app.
    #[tokio::test]
    async fn running_service_does_not_block_the_window() {
        use std::io::{BufRead, Write};
        for (name, live, blocked) in [
            (
                "win-service",
                json!({ "command": "app", "args": ["daemon"] }),
                false,
            ),
            ("win-app", json!(true), true),
        ] {
            let (hub, base, _) =
                hub_with_tool(name, json!({ "live": live, "run": { "command": "app" } }));
            let listener =
                std::os::unix::net::UnixListener::bind(base.join("run/tuxhub/livedemo.sock"))
                    .unwrap();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    std::thread::spawn(move || {
                        let mut out = stream.try_clone().unwrap();
                        for line in std::io::BufReader::new(stream)
                            .lines()
                            .map_while(Result::ok)
                        {
                            let msg: Value = serde_json::from_str(&line).unwrap();
                            let result = json!({ "protocol": 1 });
                            let _ =
                                writeln!(out, "{}", json!({ "id": msg["id"], "result": result }));
                        }
                    });
                }
            });
            let inst = tools::load(&hub.tools_dir, "livedemo").unwrap();
            assert!(app_is_running(&hub, &inst).await, "{name}");
            assert_eq!(window_is_open(&hub, &inst).await, blocked, "{name}");
            // Only the app itself is quit and started again for an update,
            // not a service that runs without a window.
            let expected = if blocked {
                RunState::Own
            } else {
                RunState::Stopped
            };
            assert_eq!(run_state(&hub, false, &inst).await, expected, "{name}");
            let _ = std::fs::remove_dir_all(&base);
        }
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

    /// Live service for the tests: answers every request with `true` (and
    /// `hello`), reports a runtime hotkey after `hello`, and logs `start <pid>`
    /// and each method to `argv[2]`.
    const TEST_SERVICE: &str = r#"#!/usr/bin/env python3
import json, os, socket, sys, threading
sock, log = sys.argv[1], sys.argv[2]
def note(line):
    with open(log, "a") as f:
        f.write(line + "\n")
note("start %d" % os.getpid())
s = socket.socket(socket.AF_UNIX)
s.bind(sock)
s.listen()
def serve(c):
    f = c.makefile("rw")
    for line in f:
        m = json.loads(line)
        note(m["method"] + " " + json.dumps(m["params"], sort_keys=True))
        res = {"protocol": 1, "name": "Demo"} if m["method"] == "hello" else True
        f.write(json.dumps({"id": m["id"], "result": res}) + "\n")
        if m["method"] == "hello":
            f.write(json.dumps({"method": "shortcuts", "params": [{"id": "macro:a", "label": "A", "shortcut": "F7"}]}) + "\n")
        f.flush()
while True:
    c, _ = s.accept()
    threading.Thread(target=serve, args=(c,), daemon=True).start()
"#;

    /// A live tool whose `[live]` section starts `script` with the socket
    /// path and a log file.
    fn hub_with_service(name: &str, script: &str) -> (Hub, PathBuf) {
        let base = std::env::temp_dir().join(format!("tuxhub-{name}-{}", std::process::id()));
        let sock = base.join("run/tuxhub/livedemo.sock");
        let log = base.join("service.log");
        let (hub, base, tool) = hub_with_tool(
            name,
            json!({
                "embed": false,
                "actions": [{ "id": "toggle", "label": "Toggle" }],
                "live": { "command": "service", "args": [sock, log] }
            }),
        );
        let exe = tool.join("service");
        std::fs::write(&exe, script).unwrap();
        std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        (hub, base)
    }

    fn service_log(base: &Path) -> Vec<String> {
        std::fs::read_to_string(base.join("service.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Ends the services the test started.
    fn stop_services(base: &Path) {
        for line in service_log(base) {
            if let Some(pid) = line.strip_prefix("start ") {
                let _ = std::process::Command::new("kill").arg(pid).status();
            }
        }
        let _ = std::fs::remove_dir_all(base);
    }

    /// Edge case 12: parallel requests start the service exactly once.
    #[tokio::test]
    async fn ensure_live_starts_the_service_once() {
        let (hub, base) = hub_with_service("ensure-once", TEST_SERVICE);
        let (a, b) = tokio::join!(ensure_live(&hub, "livedemo"), ensure_live(&hub, "livedemo"));
        let starts = service_log(&base)
            .iter()
            .filter(|l| l.starts_with("start "))
            .count();
        stop_services(&base);
        a.unwrap();
        b.unwrap();
        assert_eq!(starts, 1);
    }

    #[tokio::test]
    async fn ensure_live_leaves_a_running_service_alone() {
        let (hub, base) = hub_with_service("ensure-running", TEST_SERVICE);
        ensure_live(&hub, "livedemo").await.unwrap();
        ensure_live(&hub, "livedemo").await.unwrap();
        let starts = service_log(&base)
            .iter()
            .filter(|l| l.starts_with("start "))
            .count();
        stop_services(&base);
        assert_eq!(starts, 1);
    }

    /// Edge case 13: a service that ends at once, or never listens, gives an
    /// error instead of hanging.
    #[tokio::test]
    async fn ensure_live_reports_a_service_that_does_not_start() {
        let wait = std::time::Duration::from_millis(500);
        for (name, script) in [
            ("ensure-exit", "#!/bin/sh\nexit 1\n"),
            ("ensure-silent", "#!/bin/sh\nexec sleep 3\n"),
        ] {
            let (hub, base) = hub_with_service(name, script);
            let started = std::time::Instant::now();
            let e = ensure_live_within(&hub, "livedemo", wait)
                .await
                .unwrap_err();
            let took = started.elapsed();
            let _ = std::fs::remove_dir_all(&base);
            assert_eq!(e, "Live Demo does not start", "{name}");
            assert!(
                took < std::time::Duration::from_secs(2),
                "{name} took {took:?}"
            );
        }
        // A missing program is reported as well.
        let (hub, base) = hub_with_service("ensure-missing", TEST_SERVICE);
        std::fs::remove_file(base.join("tool/service")).unwrap();
        let e = ensure_live(&hub, "livedemo").await.unwrap_err();
        let _ = std::fs::remove_dir_all(&base);
        assert!(e.contains("is missing"), "{e}");
    }

    /// Without `[live]` tuxhub cannot start the application.
    #[tokio::test]
    async fn ensure_live_without_start_command_is_not_running() {
        let (hub, base, _) =
            hub_with_tool("ensure-nostart", json!({ "embed": false, "live": true }));
        let e = ensure_live(&hub, "livedemo").await.unwrap_err();
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(e, "Live Demo is not running");
    }

    /// Edge case 15: a hotkey of a live tool starts the service, and the
    /// action arrives, also a runtime hotkey that is not in the schema.
    #[tokio::test]
    async fn hotkey_action_starts_the_live_service() {
        let (hub, base) = hub_with_service("hotkey", TEST_SERVICE);
        std::fs::create_dir_all(&hub.settings_dir).unwrap();
        std::fs::write(
            hub.dyn_shortcuts_path("livedemo"),
            json!([{ "id": "macro:a", "label": "A", "shortcut": "F7" }]).to_string(),
        )
        .unwrap();
        let toggle = action_on(&hub, "livedemo", "toggle").await;
        let dynamic = action_on(&hub, "livedemo", "macro:a").await;
        let unknown = action_on(&hub, "livedemo", "nope").await;
        let log = service_log(&base);
        stop_services(&base);
        assert_eq!(toggle.unwrap(), json!(true));
        assert_eq!(dynamic.unwrap(), json!(true));
        assert!(unknown.is_err());
        assert!(
            log.contains(&r#"action {"id": "toggle"}"#.to_string()),
            "{log:?}"
        );
        assert!(
            log.contains(&r#"action {"id": "macro:a"}"#.to_string()),
            "{log:?}"
        );
        assert!(!log.iter().any(|l| l.contains("nope")));
    }

    /// Edge case 11 (backend): a tool page's `call` goes over the socket,
    /// reserved methods do not.
    #[tokio::test]
    async fn bridge_call_goes_over_the_live_socket() {
        let (hub, base) = hub_with_service("bridge", TEST_SERVICE);
        let ok = call_on(&hub, "livedemo", "macros.list", json!({ "n": 1 })).await;
        let refused = call_on(&hub, "livedemo", "quit", json!({})).await;
        let log = service_log(&base);
        stop_services(&base);
        assert_eq!(ok.unwrap(), json!(true));
        assert!(refused.unwrap_err().contains("not allowed"));
        assert!(
            log.contains(&r#"macros.list {"n": 1}"#.to_string()),
            "{log:?}"
        );
        assert!(!log.iter().any(|l| l.starts_with("quit")));
    }

    /// Edge case 16: `shortcuts` from a live application sets the runtime
    /// hotkeys.
    #[tokio::test]
    async fn shortcuts_over_live_set_the_runtime_hotkeys() {
        let (mut hub, base) = hub_with_service("shortcuts", TEST_SERVICE);
        std::fs::create_dir_all(&hub.settings_dir).unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<live::LiveEvent>::new()));
        let sink = events.clone();
        hub.live = live::Live::new(
            Some(base.join("run")),
            std::sync::Arc::new(move |e| sink.lock().unwrap().push(e)),
        );
        ensure_live(&hub, "livedemo").await.unwrap();
        hub.live
            .call("livedemo", "status", json!({}))
            .await
            .unwrap();
        let mut changed = false;
        for _ in 0..100 {
            let evs = events.lock().unwrap().clone();
            if let Some(e) = evs.iter().find(|e| e.method == "shortcuts") {
                changed = save_dyn_shortcuts(&hub, &e.tool, &e.params);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let saved = hub.dyn_shortcuts("livedemo");
        stop_services(&base);
        assert!(changed);
        assert_eq!(saved.len(), 1);
        assert_eq!(
            (saved[0].id.as_str(), saved[0].shortcut.as_str()),
            ("macro:a", "F7")
        );
        // The same list again changes nothing.
        assert!(!save_dyn_shortcuts(
            &hub,
            "livedemo",
            &json!([{ "id": "macro:a", "label": "A", "shortcut": "F7" }])
        ));
    }

    /// A start that ends at once because another instance is starting is not
    /// an error when that instance opens the socket shortly after.
    #[tokio::test]
    async fn ensure_live_waits_for_an_instance_started_elsewhere() {
        let script = format!(
            "#!/bin/sh\n(sleep 0.4; exec python3 -c '{}' \"$@\") >/dev/null 2>&1 &\nexit 0\n",
            "import sys,os;exec(open(os.path.join(os.path.dirname(sys.argv[0]),\"svc.py\")).read())"
        );
        let (hub, base) = hub_with_service("elsewhere", &script);
        std::fs::write(base.join("tool/svc.py"), TEST_SERVICE).unwrap();
        let r = ensure_live(&hub, "livedemo").await;
        stop_services(&base);
        r.unwrap();
    }

    /// `call("action")` from a tool page only runs declared actions.
    #[tokio::test]
    async fn bridge_action_call_is_checked() {
        let (hub, base) = hub_with_service("bridge-action", TEST_SERVICE);
        let ok = call_on(&hub, "livedemo", "action", json!({ "id": "toggle" })).await;
        let refused = call_on(&hub, "livedemo", "action", json!({ "id": "setup" })).await;
        let log = service_log(&base);
        stop_services(&base);
        assert_eq!(ok.unwrap(), json!(true));
        assert!(refused.is_err());
        assert!(!log.iter().any(|l| l.contains("setup")), "{log:?}");
    }

    /// A hub that asks fake flatpak, rpm and dpkg-query where apps are.
    fn hub_with_fakes(name: &str) -> (Hub, tools::Fakes) {
        let f = tools::Fakes::new(name);
        let mut hub = Hub::for_test(f.dir.join("tools"), f.dir.join("settings"));
        hub.programs = f.programs.clone();
        (hub, f)
    }

    fn workshop_entry(id: &str, source: Value) -> Entry {
        serde_json::from_value(json!({
            "id": id,
            "name": format!("App {id}"),
            "repo": "owner/app",
            "source": source,
        }))
        .unwrap()
    }

    fn flatpak_app(id: &str) -> Entry {
        workshop_entry(
            id,
            json!({ "type": "flatpak", "flatpak_id": format!("net.example.{id}") }),
        )
    }

    fn package_app(id: &str) -> Entry {
        workshop_entry(
            id,
            json!({ "type": "system-package", "package": id, "rpm": "*.rpm", "command": "/usr/bin/true" }),
        )
    }

    /// Writes the tool.json of a tool in tuxhub.
    fn record(hub: &Hub, inst: &Installed) {
        let dir = hub.tools_dir.join(inst.id());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.json"), serde_json::to_string(inst).unwrap()).unwrap();
    }

    fn workshop_state(items: &[WorkshopItem]) -> Vec<(&str, bool, bool)> {
        items
            .iter()
            .map(|i| (i.entry.id.as_str(), i.installed, i.present))
            .collect()
    }

    #[tokio::test]
    async fn workshop_marks_apps_on_this_computer() {
        let (hub, f) = hub_with_fakes("workshop-present");
        f.flatpak_with(&[tools::Installation::System]);
        f.set("rpm", "exit 0");
        let own = flatpak_app("own");
        record(&hub, &Installed::new(own.clone()));
        let release = workshop_entry("release", json!({ "type": "github-release", "asset": "*" }));
        let items = workshop_items(
            &hub,
            vec![flatpak_app("sys"), own, package_app("clock"), release],
        )
        .await;
        assert_eq!(
            workshop_state(&items),
            [
                ("sys", false, true),
                ("own", true, false),
                ("clock", false, true),
                ("release", false, false)
            ]
        );
    }

    #[tokio::test]
    async fn workshop_without_flatpak_marks_nothing() {
        let (hub, _f) = hub_with_fakes("workshop-no-flatpak");
        let items = workshop_items(&hub, vec![flatpak_app("a"), package_app("b")]).await;
        assert_eq!(
            workshop_state(&items),
            [("a", false, false), ("b", false, false)]
        );
    }

    /// Hanging queries run side by side and end at the timeout.
    #[tokio::test]
    async fn workshop_loads_while_flatpak_hangs() {
        let (mut hub, f) = hub_with_fakes("workshop-hang");
        hub.programs.timeout = std::time::Duration::from_millis(500);
        f.set("flatpak", "exec sleep 30");
        let entries: Vec<Entry> = (0..6).map(|i| flatpak_app(&format!("app{i}"))).collect();
        let start = std::time::Instant::now();
        let items = workshop_items(&hub, entries).await;
        assert_eq!(items.len(), 6);
        assert!(items.iter().all(|i| !i.present));
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn install_refuses_an_app_already_on_this_computer() {
        let (hub, f) = hub_with_fakes("install-present");
        f.flatpak_with(&[tools::Installation::System]);
        let app = tauri::test::mock_app();
        let err = install_entry(app.handle(), &hub, &flatpak_app("demo"))
            .await
            .unwrap_err();
        assert!(err.contains("already on this computer"), "{err}");
        assert!(
            f.calls().iter().all(|c| c.starts_with("flatpak info ")),
            "{:?}",
            f.calls()
        );
        assert!(!hub.tools_dir.join("demo").exists());
    }

    #[tokio::test]
    async fn install_puts_a_new_flatpak_into_the_user_installation() {
        let (hub, f) = hub_with_fakes("install-new");
        f.flatpak_with(&[]);
        let app = tauri::test::mock_app();
        let inst = install_entry(app.handle(), &hub, &flatpak_app("demo"))
            .await
            .unwrap();
        assert!(!inst.adopted);
        assert_eq!(inst.installation, tools::Installation::User);
        assert!(f.calls().contains(
            &"flatpak install --user -y --noninteractive flathub net.example.demo".to_string()
        ));
        assert!(hub.tools_dir.join("demo/tool.json").is_file());
    }

    fn recorded(hub: &Hub, id: &str) -> Option<Installed> {
        let s = std::fs::read_to_string(hub.tools_dir.join(id).join("tool.json")).ok()?;
        Some(serde_json::from_str(&s).unwrap())
    }

    #[tokio::test]
    async fn adopts_a_user_flatpak() {
        let (hub, f) = hub_with_fakes("adopt-user");
        f.flatpak_with(&[tools::Installation::User]);
        let inst = adopt_entry(&hub, vec![flatpak_app("demo")], "demo")
            .await
            .unwrap();
        assert!(inst.adopted);
        assert_eq!(inst.installation, tools::Installation::User);
        let saved = recorded(&hub, "demo").unwrap();
        assert!(saved.adopted);
        assert_eq!(saved.installation, tools::Installation::User);
        // Adding never installs or removes anything.
        assert!(
            f.calls().iter().all(|c| c.starts_with("flatpak info ")),
            "{:?}",
            f.calls()
        );
    }

    #[tokio::test]
    async fn adopting_both_flatpaks_takes_the_system_one() {
        let (hub, f) = hub_with_fakes("adopt-both");
        f.flatpak_with(&[tools::Installation::User, tools::Installation::System]);
        let inst = adopt_entry(&hub, vec![flatpak_app("demo")], "demo")
            .await
            .unwrap();
        assert_eq!(inst.installation, tools::Installation::System);
        assert!(recorded(&hub, "demo").unwrap().adopted);
    }

    #[tokio::test]
    async fn adopts_a_system_package_with_its_version() {
        let (hub, f) = hub_with_fakes("adopt-package");
        f.set("rpm", "[ \"$2\" = --quiet ] && exit 0\nprintf 1.4.2");
        let inst = adopt_entry(&hub, vec![package_app("clock")], "clock")
            .await
            .unwrap();
        assert!(inst.adopted);
        assert_eq!(inst.version.as_deref(), Some("1.4.2"));
        assert!(
            f.calls()
                .contains(&"rpm -q --qf %{VERSION} clock".to_string()),
            "{:?}",
            f.calls()
        );
    }

    #[tokio::test]
    async fn adopt_needs_an_installation() {
        let (hub, f) = hub_with_fakes("adopt-missing");
        f.flatpak_with(&[]);
        let err = adopt_entry(&hub, vec![flatpak_app("demo")], "demo")
            .await
            .unwrap_err();
        assert!(err.contains("not installed on this computer"), "{err}");
        assert!(!hub.tools_dir.join("demo").exists());
    }

    #[tokio::test]
    async fn adopt_needs_a_workshop_entry() {
        let (hub, f) = hub_with_fakes("adopt-unknown");
        f.flatpak_with(&[tools::Installation::System]);
        let err = adopt_entry(&hub, vec![flatpak_app("demo")], "other")
            .await
            .unwrap_err();
        assert!(err.contains("not in the workshop"), "{err}");
        assert!(f.calls().is_empty());
        assert!(!hub.tools_dir.join("other").exists());
    }

    #[tokio::test]
    async fn adopt_leaves_a_tool_in_tuxhub_alone() {
        let (hub, f) = hub_with_fakes("adopt-twice");
        f.flatpak_with(&[tools::Installation::System]);
        let own = Installed::new(flatpak_app("demo"));
        record(&hub, &own);
        let err = adopt_entry(&hub, vec![flatpak_app("demo")], "demo")
            .await
            .unwrap_err();
        assert!(err.contains("already in tuxhub"), "{err}");
        assert!(!recorded(&hub, "demo").unwrap().adopted);
    }

    fn installed_app(
        desktop_id: &str,
        source: local_apps::LocalSource,
        command: &[&str],
    ) -> local_apps::LocalApp {
        local_apps::LocalApp {
            desktop_id: desktop_id.into(),
            name: format!("App {desktop_id}"),
            summary: None,
            description: None,
            icon: None,
            homepage: None,
            repo: None,
            categories: vec![],
            source,
            command: command.iter().map(|s| s.to_string()).collect(),
            autostart: false,
        }
    }

    fn installed_flatpak(desktop_id: &str, flatpak_id: &str) -> local_apps::LocalApp {
        installed_app(
            desktop_id,
            local_apps::LocalSource::Flatpak {
                flatpak_id: flatpak_id.into(),
                installations: vec![tools::Installation::User],
                origin: Some("flathub".into()),
            },
            &["/usr/bin/flatpak", "run", flatpak_id],
        )
    }

    fn installed_program(desktop_id: &str, program: &str) -> local_apps::LocalApp {
        installed_app(
            desktop_id,
            local_apps::LocalSource::Unknown {
                program: program.into(),
            },
            &[program],
        )
    }

    /// Tools tuxhub installed put their own start menu entries next to the
    /// apps; they are tuxhub's tools already, never a second, adopted copy.
    #[tokio::test]
    async fn own_tools_are_not_installed_apps() {
        let (hub, f) = hub_with_fakes("local-own-tools");
        f.set("run-a", "exit 0");
        let run_a = f.dir.join("run-a");
        let own = hub.tools_dir.join("autoclicker/files/tuxhub-autoclicker");
        // Only lexically: a path that leaves the folder again is no tool's.
        let out = hub.tools_dir.join("../elsewhere/app");
        let apps = vec![
            installed_program("a.desktop", run_a.to_str().unwrap()),
            installed_program("own.desktop", own.to_str().unwrap()),
            installed_program("out.desktop", out.to_str().unwrap()),
        ];
        let views = local_app_views(&hub, apps.clone(), &[], &Default::default()).await;
        let ids: Vec<_> = views.iter().map(|v| v.app.desktop_id.as_str()).collect();
        assert_eq!(ids, ["a.desktop", "out.desktop"]);
        assert!(
            add_local_app(&hub, &apps, vec![], "own.desktop")
                .await
                .is_err()
        );
    }

    /// Only an app of the current list can be added; the string from the
    /// user interface never names a file or a command.
    #[tokio::test]
    async fn local_app_add_needs_an_app_of_the_list() {
        let (hub, f) = hub_with_fakes("local-add-unknown");
        f.flatpak_with(&[tools::Installation::User]);
        let apps = [installed_program("a.desktop", "/bin/sh")];
        for bad in [
            "b.desktop",
            "/bin/sh",
            "../a.desktop",
            "",
            &"x".repeat(5000),
        ] {
            let err = add_local_app(&hub, &apps, vec![], bad).await.unwrap_err();
            assert!(err.contains("not an app on this computer"), "{err}");
        }
        assert!(!hub.tools_dir.exists());
        assert!(f.calls().is_empty());
    }

    /// An app the catalog knows is added like from the workshop, under the
    /// catalog id.
    #[tokio::test]
    async fn local_app_add_takes_the_catalog_way() {
        let (hub, f) = hub_with_fakes("local-add-catalog");
        f.flatpak_with(&[tools::Installation::System]);
        let apps = [installed_flatpak("x.desktop", "net.example.demo")];
        let inst = add_local_app(&hub, &apps, vec![flatpak_app("demo")], "x.desktop")
            .await
            .unwrap();
        assert_eq!(inst.id(), "demo");
        let saved = recorded(&hub, "demo").unwrap();
        assert!(saved.adopted);
        assert_eq!(saved.installation, tools::Installation::System);
        assert_eq!(saved.window, tools::WindowMode::Hub);
        assert!(f.calls().iter().all(|c| c.starts_with("flatpak info ")));
        // Twice is refused.
        let err = add_local_app(&hub, &apps, vec![flatpak_app("demo")], "x.desktop")
            .await
            .unwrap_err();
        assert!(err.contains("already in tuxhub"), "{err}");
    }

    /// An app that starts at login already runs: it opens in its own window.
    #[tokio::test]
    async fn local_app_add_stores_the_command() {
        let (hub, _f) = hub_with_fakes("local-add-launcher");
        let mut app = installed_program("Some App.desktop", "/bin/sh");
        app.autostart = true;
        let inst = add_local_app(&hub, &[app.clone()], vec![], "Some App.desktop")
            .await
            .unwrap();
        assert_eq!(inst.id(), "local.some-app");
        let saved = recorded(&hub, "local.some-app").unwrap();
        assert!(saved.adopted);
        assert_eq!(saved.window, tools::WindowMode::Own);
        assert!(matches!(
            &saved.entry.source,
            Source::Launcher { desktop_id, command }
                if desktop_id == "Some App.desktop" && command == &["/bin/sh"]
        ));
        assert!(can_launch(&saved, &saved.manifest(&hub.tools_dir)));
        let err = add_local_app(&hub, &[app], vec![], "Some App.desktop")
            .await
            .unwrap_err();
        assert!(err.contains("already in tuxhub"), "{err}");

        // Leaving the sidebar keeps the app on the computer.
        forget(&hub, "local.some-app").unwrap();
        assert!(recorded(&hub, "local.some-app").is_none());
        assert!(Path::new("/bin/sh").is_file());
    }

    #[tokio::test]
    async fn local_app_views_tell_the_sidebar_state() {
        let (hub, f) = hub_with_fakes("local-views");
        f.flatpak_with(&[tools::Installation::User]);
        f.set(
            "flatpak",
            "[ \"$1\" = ps ] && echo net.example.demo\nexit 0",
        );
        // Programs of their own: a shared one like /bin/sh always runs somewhere.
        f.set("run-a", "exit 0");
        f.set("run-b", "exit 0");
        let (run_a, run_b) = (f.dir.join("run-a"), f.dir.join("run-b"));
        let apps = vec![
            installed_flatpak("x.desktop", "net.example.demo"),
            installed_program("a.desktop", run_a.to_str().unwrap()),
            installed_program("b.desktop", run_b.to_str().unwrap()),
        ];
        let catalog = vec![flatpak_app("demo")];
        record(
            &hub,
            &Installed {
                adopted: true,
                ..Installed::new(flatpak_app("demo"))
            },
        );
        add_local_app(&hub, &apps, catalog.clone(), "a.desktop")
            .await
            .unwrap();
        hub.updates
            .lock()
            .unwrap()
            .insert("demo".into(), "2.0".into());
        let views = local_app_views(&hub, apps, &catalog, &Default::default()).await;
        let state: Vec<_> = views
            .iter()
            .map(|v| {
                (
                    v.app.desktop_id.as_str(),
                    v.tool_id.as_str(),
                    v.in_sidebar,
                    v.update.as_deref(),
                    v.run_state,
                )
            })
            .collect();
        assert_eq!(
            state,
            [
                ("x.desktop", "demo", true, Some("2.0"), RunState::Own),
                ("a.desktop", "local.a", true, None, RunState::Stopped),
                ("b.desktop", "local.b", false, None, RunState::Stopped),
            ]
        );
        let in_hub = ["demo".to_string()].into_iter().collect();
        let views = local_app_views(
            &hub,
            vec![installed_flatpak("x.desktop", "net.example.demo")],
            &catalog,
            &in_hub,
        )
        .await;
        assert_eq!(views[0].run_state, RunState::Hub);
    }

    /// The level tag of the installed apps: what the catalog says, the
    /// installed manifest once it is in tuxhub, and "embedded" for an app
    /// tuxhub only starts, as it has no manifest.
    #[tokio::test]
    async fn local_app_views_tell_the_integration() {
        let (hub, f) = hub_with_fakes("local-integration");
        f.set("run-a", "exit 0");
        let run_a = f.dir.join("run-a");
        let mut listed = flatpak_app("demo");
        listed.integration = Some(tools::Integration::Standalone);
        let unlisted = flatpak_app("other");
        let apps = vec![
            installed_flatpak("x.desktop", "net.example.demo"),
            installed_flatpak("y.desktop", "net.example.other"),
            installed_program("a.desktop", run_a.to_str().unwrap()),
        ];
        let catalog = vec![listed, unlisted];
        let views = local_app_views(&hub, apps.clone(), &catalog, &Default::default()).await;
        let levels: Vec<_> = views.iter().map(|v| v.integration).collect();
        assert_eq!(
            levels,
            [
                Some(tools::Integration::Standalone),
                None,
                Some(tools::Integration::Embedded),
            ]
        );
        add_local_app(&hub, &apps, catalog.clone(), "a.desktop")
            .await
            .unwrap();
        let views = local_app_views(&hub, apps, &catalog, &Default::default()).await;
        assert_eq!(views[2].integration, Some(tools::Integration::Embedded));
    }

    /// An app started through a shell runs as bash, like the user's terminals:
    /// tuxhub cannot tell whether it runs and never quits it.
    #[tokio::test]
    async fn shared_launchers_are_never_quit() {
        let (hub, _f) = hub_with_fakes("local-shared-quit");
        let mut app = installed_program("s.desktop", "/bin/sh");
        app.command = vec!["/bin/sh".into(), "-c".into(), "x".into()];
        let inst = add_local_app(&hub, &[app], vec![], "s.desktop")
            .await
            .unwrap();
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Unknown);
        assert_eq!(run_state(&hub, true, &inst).await, RunState::Hub);
        assert!(!app_is_running(&hub, &inst).await);
        let err = can_quit(&hub, &inst).unwrap_err();
        assert!(err.contains("not started and stopped by tuxhub"), "{err}");
        // `quit_app` is not called here: a regression would end the user's shells.
        assert!(tools::app_program(&hub.tools_dir, &inst).is_none());
    }

    /// tuxhub never removes an app it only added from this computer.
    #[tokio::test]
    async fn added_apps_are_not_uninstalled() {
        let (hub, _f) = hub_with_fakes("local-uninstall");
        let apps = [installed_program("a.desktop", "/bin/sh")];
        let inst = add_local_app(&hub, &apps, vec![], "a.desktop")
            .await
            .unwrap();
        assert!(tools::check_removable(&inst).is_err());
        assert!(tools::check_removable(&Installed::new(flatpak_app("demo"))).is_ok());
    }

    /// (missing, duplicate) of a tool in the sidebar.
    async fn sidebar_state(hub: &Hub, id: &str) -> (bool, bool) {
        let views = tool_views(hub).await;
        let v = views.iter().find(|v| v.entry.id == id).unwrap();
        (v.missing, v.duplicate)
    }

    #[tokio::test]
    async fn adopting_both_flatpaks_shows_the_duplicate_hint() {
        let (hub, f) = hub_with_fakes("hint-adopted");
        f.flatpak_with(&[tools::Installation::User, tools::Installation::System]);
        adopt_entry(&hub, vec![flatpak_app("demo")], "demo")
            .await
            .unwrap();
        assert_eq!(sidebar_state(&hub, "demo").await, (false, true));
    }

    /// tuxhub installed a user copy of an app the
    /// system already had. Removing the copy keeps the settings.
    #[tokio::test]
    async fn removes_the_own_user_copy_of_a_system_flatpak() {
        let (hub, f) = hub_with_fakes("remove-copy");
        f.flatpak_with(&[tools::Installation::User, tools::Installation::System]);
        record(&hub, &Installed::new(flatpak_app("demo")));
        std::fs::create_dir_all(&hub.settings_dir).unwrap();
        std::fs::write(hub.settings_path("demo"), "{}").unwrap();
        assert_eq!(sidebar_state(&hub, "demo").await, (false, true));

        remove_copy(&hub, "demo").await.unwrap();
        let uninstalls: Vec<String> = f
            .calls()
            .into_iter()
            .filter(|c| c.contains("uninstall"))
            .collect();
        assert_eq!(
            uninstalls,
            ["flatpak uninstall --user -y --noninteractive net.example.demo"]
        );
        let saved = recorded(&hub, "demo").unwrap();
        assert_eq!(saved.installation, tools::Installation::System);
        assert!(saved.adopted);
        assert!(hub.settings_path("demo").is_file());

        f.flatpak_with(&[tools::Installation::System]);
        assert_eq!(sidebar_state(&hub, "demo").await, (false, false));
    }

    #[tokio::test]
    async fn remove_copy_needs_two_installations() {
        let (hub, f) = hub_with_fakes("remove-copy-once");
        f.flatpak_with(&[tools::Installation::User]);
        record(&hub, &Installed::new(flatpak_app("demo")));
        assert!(remove_copy(&hub, "demo").await.is_err());
        assert!(!f.calls().iter().any(|c| c.contains("uninstall")));
        assert_eq!(
            recorded(&hub, "demo").unwrap().installation,
            tools::Installation::User
        );
    }

    #[tokio::test]
    async fn keeping_both_installations_hides_the_hint_for_good() {
        let (hub, f) = hub_with_fakes("keep-both");
        f.flatpak_with(&[tools::Installation::User, tools::Installation::System]);
        record(&hub, &Installed::new(flatpak_app("demo")));
        dismiss_duplicate(&hub, "demo").unwrap();
        assert!(recorded(&hub, "demo").unwrap().duplicate_dismissed);
        // A new hub reads the same tool.json, as after a restart.
        let mut again = Hub::for_test(hub.tools_dir.clone(), hub.settings_dir.clone());
        again.programs = f.programs.clone();
        assert_eq!(sidebar_state(&again, "demo").await, (false, false));
        assert!(!f.calls().iter().any(|c| c.contains("uninstall")));
    }

    #[tokio::test]
    async fn a_moved_flatpak_follows_the_remaining_installation() {
        let (hub, f) = hub_with_fakes("moved");
        f.flatpak_with(&[tools::Installation::System]);
        record(&hub, &Installed::new(flatpak_app("demo")));
        assert_eq!(sidebar_state(&hub, "demo").await, (false, false));
        let saved = recorded(&hub, "demo").unwrap();
        assert_eq!(saved.installation, tools::Installation::System);
        assert!(saved.adopted);
    }

    #[tokio::test]
    async fn an_app_removed_elsewhere_is_missing() {
        let (hub, f) = hub_with_fakes("missing");
        f.flatpak_with(&[]);
        f.set("rpm", "exit 1");
        record(&hub, &Installed::new(flatpak_app("demo")));
        record(&hub, &Installed::new(package_app("clock")));
        assert_eq!(sidebar_state(&hub, "demo").await, (true, false));
        assert_eq!(sidebar_state(&hub, "clock").await, (true, false));
        // The entry stays until the user decides.
        assert!(recorded(&hub, "demo").is_some());
    }

    fn adopted_system_flatpak(hub: &Hub) -> Installed {
        let inst = Installed {
            adopted: true,
            installation: tools::Installation::System,
            ..Installed::new(flatpak_app("demo"))
        };
        record(hub, &inst);
        std::fs::create_dir_all(&hub.settings_dir).unwrap();
        std::fs::write(hub.settings_path("demo"), "{}").unwrap();
        std::fs::write(hub.dyn_shortcuts_path("demo"), "[]").unwrap();
        inst
    }

    #[tokio::test]
    async fn forgetting_an_adopted_app_runs_nothing() {
        let (hub, f) = hub_with_fakes("forget");
        f.flatpak_with(&[tools::Installation::System]);
        adopted_system_flatpak(&hub);
        forget(&hub, "demo").unwrap();
        assert!(f.calls().is_empty(), "{:?}", f.calls());
        assert!(recorded(&hub, "demo").is_none());
        assert!(!hub.tools_dir.join("demo").exists());
        assert!(!hub.settings_path("demo").exists());
        assert!(!hub.dyn_shortcuts_path("demo").exists());
    }

    /// Only tools without files of their own can leave tuxhub without being
    /// uninstalled.
    #[test]
    fn forget_leaves_tools_with_files_alone() {
        let (hub, _f) = hub_with_fakes("forget-release");
        let release = workshop_entry("release", json!({ "type": "github-release", "asset": "*" }));
        record(&hub, &Installed::new(release));
        assert!(forget(&hub, "release").is_err());
        assert!(recorded(&hub, "release").is_some());
    }

    #[tokio::test]
    async fn uninstalls_an_adopted_flatpak_from_its_installation() {
        let (hub, f) = hub_with_fakes("uninstall-system");
        f.flatpak_with(&[tools::Installation::System]);
        let inst = adopted_system_flatpak(&hub);
        tools::uninstall(&hub.tools_dir, &hub.programs, &inst)
            .await
            .unwrap();
        assert_eq!(
            f.calls(),
            ["flatpak uninstall --system -y --noninteractive net.example.demo"]
        );
        assert!(recorded(&hub, "demo").is_none());
    }

    #[tokio::test]
    async fn uninstalling_an_app_that_is_gone_succeeds() {
        let (hub, f) = hub_with_fakes("uninstall-gone");
        f.set(
            "flatpak",
            "echo \"error: No installed refs found for \u{2018}net.example.demo\u{2019}\" >&2\nexit 1",
        );
        let inst = adopted_system_flatpak(&hub);
        tools::uninstall(&hub.tools_dir, &hub.programs, &inst)
            .await
            .unwrap();
        assert!(recorded(&hub, "demo").is_none());
    }

    #[tokio::test]
    async fn installing_a_missing_app_makes_an_own_user_installation() {
        let (hub, f) = hub_with_fakes("reinstall");
        f.flatpak_with(&[]);
        adopted_system_flatpak(&hub);
        let app = tauri::test::mock_app();
        install_entry(app.handle(), &hub, &flatpak_app("demo"))
            .await
            .unwrap();
        assert!(f.calls().contains(
            &"flatpak install --user -y --noninteractive flathub net.example.demo".to_string()
        ));
        let saved = recorded(&hub, "demo").unwrap();
        assert!(!saved.adopted);
        assert_eq!(saved.installation, tools::Installation::User);
    }

    /// A flatpak that has the app in the given installations and lists
    /// `updates` for `remote-ls --updates`.
    fn flatpak_with_updates(f: &tools::Fakes, installed: &[tools::Installation], updates: &str) {
        flatpak_with_versions(f, installed, "", updates);
    }

    /// Like [`flatpak_with_updates`], and `list` prints `versions`.
    fn flatpak_with_versions(
        f: &tools::Fakes,
        installed: &[tools::Installation],
        versions: &str,
        updates: &str,
    ) {
        let flags: Vec<&str> = installed.iter().map(|i| i.flag()).collect();
        f.set(
            "flatpak",
            &format!(
                "case \"$1\" in\n info) case \" {} \" in *\" $2 \"*) exit 0;; esac; exit 1;;\n list) printf '{versions}';;\n remote-ls) printf '{updates}';;\nesac\nexit 0",
                flags.join(" ")
            ),
        );
    }

    #[tokio::test]
    async fn finds_flatpak_updates_in_the_used_installation() {
        let (hub, f) = hub_with_fakes("flatpak-update-check");
        let inst = adopted_system_flatpak(&hub);
        flatpak_with_updates(
            &f,
            &[tools::Installation::System],
            "net.example.other\\t1.0\\tabc\\nnet.example.demo\\t2.3.0\\t0123456789ab\\n",
        );
        assert_eq!(
            tools::check_update(&inst, &hub.programs)
                .await
                .unwrap()
                .as_deref(),
            Some("2.3.0")
        );
        assert!(
            f.calls().contains(
                &"flatpak remote-ls --updates --app --system --columns=application,version,commit"
                    .to_string()
            ),
            "{:?}",
            f.calls()
        );

        // Without a version the commit tells the update apart.
        flatpak_with_updates(
            &f,
            &[tools::Installation::System],
            "net.example.demo\\t\\t0123456789ab\\n",
        );
        assert_eq!(
            tools::check_update(&inst, &hub.programs)
                .await
                .unwrap()
                .as_deref(),
            Some("0123456789ab")
        );

        flatpak_with_updates(
            &f,
            &[tools::Installation::System],
            "net.example.other\\t1.0\\tabc\\n",
        );
        assert_eq!(
            tools::check_update(&inst, &hub.programs).await.unwrap(),
            None
        );

        // A new commit of the installed version is named by its commit, so
        // the button never offers the version that is already there.
        let update = "net.example.demo\\t2.3.0\\t0123456789abcdef\\n";
        flatpak_with_versions(
            &f,
            &[tools::Installation::System],
            "net.example.other\\t2.2.0\\nnet.example.demo\\t2.3.0\\n",
            update,
        );
        assert_eq!(
            tools::check_update(&inst, &hub.programs)
                .await
                .unwrap()
                .as_deref(),
            Some("2.3.0 (0123456789ab)")
        );
        assert!(
            f.calls()
                .contains(&"flatpak list --app --system --columns=application,version".to_string()),
            "{:?}",
            f.calls()
        );
        flatpak_with_versions(
            &f,
            &[tools::Installation::System],
            "net.example.demo\\t2.2.0\\n",
            update,
        );
        assert_eq!(
            tools::check_update(&inst, &hub.programs)
                .await
                .unwrap()
                .as_deref(),
            Some("2.3.0")
        );
    }

    #[tokio::test]
    async fn updates_a_flatpak_in_its_installation() {
        let (hub, f) = hub_with_fakes("flatpak-update");
        flatpak_with_updates(&f, &[tools::Installation::System], "");
        adopted_system_flatpak(&hub);
        let app = tauri::test::mock_app();
        let inst = install_entry(app.handle(), &hub, &flatpak_app("demo"))
            .await
            .unwrap();
        let changes: Vec<String> = f
            .calls()
            .into_iter()
            .filter(|c| !c.starts_with("flatpak info "))
            .collect();
        assert_eq!(
            changes,
            ["flatpak update --system -y --noninteractive net.example.demo"]
        );
        assert!(inst.adopted);
        let saved = recorded(&hub, "demo").unwrap();
        assert!(saved.adopted);
        assert_eq!(saved.installation, tools::Installation::System);
    }

    /// The recorded installation is gone, the other one has the app: an
    /// update goes there instead of installing a second copy.
    #[tokio::test]
    async fn updating_a_moved_flatpak_never_installs_a_second_copy() {
        let (hub, f) = hub_with_fakes("update-moved");
        flatpak_with_updates(&f, &[tools::Installation::System], "");
        record(&hub, &Installed::new(flatpak_app("demo")));
        let app = tauri::test::mock_app();
        install_entry(app.handle(), &hub, &flatpak_app("demo"))
            .await
            .unwrap();
        let changes: Vec<String> = f
            .calls()
            .into_iter()
            .filter(|c| !c.starts_with("flatpak info "))
            .collect();
        assert_eq!(
            changes,
            ["flatpak update --system -y --noninteractive net.example.demo"]
        );
        let saved = recorded(&hub, "demo").unwrap();
        assert_eq!(saved.installation, tools::Installation::System);
        assert!(saved.adopted);
    }

    /// A query that hangs says nothing about the installation: the record
    /// keeps pointing where it did.
    #[tokio::test]
    async fn a_hanging_query_does_not_move_a_flatpak() {
        let (mut hub, f) = hub_with_fakes("hang-no-move");
        hub.programs.timeout = std::time::Duration::from_millis(300);
        f.set(
            "flatpak",
            "case \"$2\" in --user) exec sleep 30;; --system) exit 0;; esac\nexit 1",
        );
        record(&hub, &Installed::new(flatpak_app("demo")));
        assert_eq!(sidebar_state(&hub, "demo").await, (false, false));
        let saved = recorded(&hub, "demo").unwrap();
        assert_eq!(saved.installation, tools::Installation::User);
        assert!(!saved.adopted);
    }

    /// The update of an adopted system package keeps it adopted; installing
    /// a missing one makes it tuxhub's own.
    #[test]
    fn updates_keep_an_adopted_package_adopted() {
        let previous = Installed {
            adopted: true,
            ..Installed::new(package_app("clock"))
        };
        let new = Installed {
            version: Some("v2".into()),
            ..Installed::new(package_app("clock"))
        };
        assert!(carry_over(Some(&previous), true, new.clone()).adopted);
        assert!(!carry_over(Some(&previous), false, new.clone()).adopted);
        assert!(!carry_over(None, true, new).adopted);
    }

    /// A workshop entry that no longer is the same Flatpak does not replace
    /// the record of the installed one.
    #[tokio::test]
    async fn a_flatpak_update_keeps_the_entry_of_the_installed_app() {
        let (hub, f) = hub_with_fakes("update-other-entry");
        flatpak_with_updates(&f, &[tools::Installation::User], "");
        record(&hub, &Installed::new(flatpak_app("demo")));
        let release = workshop_entry("demo", json!({ "type": "github-release", "asset": "*" }));
        let app = tauri::test::mock_app();
        install_entry(app.handle(), &hub, &release).await.unwrap();
        let saved = recorded(&hub, "demo").unwrap();
        assert!(
            matches!(saved.entry.source, Source::Flatpak { .. }),
            "{:?}",
            saved.entry.source
        );
    }

    #[test]
    fn an_unknown_integration_keeps_the_workshop() {
        let entries = parse_workshop(
            r#"[
                {"id": "a", "name": "A", "integration": "foo",
                 "source": {"type": "flatpak", "flatpak_id": "net.example.A"}},
                {"id": "b", "name": "B", "integration": "embedded",
                 "source": {"type": "flatpak", "flatpak_id": "net.example.B"}}
            ]"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].integration, None);
        assert_eq!(entries[1].integration, Some(tools::Integration::Embedded));
    }

    #[tokio::test]
    async fn workshop_integration_prefers_the_installed_manifest() {
        let (hub, _f) = hub_with_fakes("workshop-integration");
        let mut live = flatpak_app("live");
        live.manifest = Some(toml::from_str("live = true").unwrap());
        record(&hub, &Installed::new(live.clone()));
        let mut listed = flatpak_app("live");
        listed.integration = Some(tools::Integration::Embedded);
        let mut other = flatpak_app("other");
        other.integration = Some(tools::Integration::Standalone);
        let items = workshop_items(&hub, vec![listed, other, flatpak_app("plain")]).await;
        let levels: Vec<_> = items.iter().map(|i| i.integration).collect();
        assert_eq!(
            levels,
            [
                Some(tools::Integration::Socket),
                Some(tools::Integration::Standalone),
                None
            ]
        );
    }

    #[test]
    fn a_record_without_window_runs_in_the_hub() {
        let inst: Installed =
            serde_json::from_value(json!({ "entry": flatpak_app("old") })).unwrap();
        assert_eq!(inst.window, tools::WindowMode::Hub);
    }

    #[test]
    fn an_update_keeps_the_window() {
        let mut previous = Installed::new(flatpak_app("demo"));
        previous.window = tools::WindowMode::Own;
        let new = Installed::new(flatpak_app("demo"));
        assert_eq!(
            carry_over(Some(&previous), false, new.clone()).window,
            tools::WindowMode::Own
        );
        assert_eq!(carry_over(None, false, new).window, tools::WindowMode::Hub);
    }

    #[tokio::test]
    async fn a_flatpak_update_keeps_the_window() {
        let (hub, f) = hub_with_fakes("flatpak-update-window");
        flatpak_with_updates(&f, &[tools::Installation::User], "");
        let mut inst = Installed::new(flatpak_app("demo"));
        inst.window = tools::WindowMode::Own;
        record(&hub, &inst);
        let app = tauri::test::mock_app();
        install_entry(app.handle(), &hub, &flatpak_app("demo"))
            .await
            .unwrap();
        assert_eq!(
            recorded(&hub, "demo").unwrap().window,
            tools::WindowMode::Own
        );
    }

    #[test]
    fn sets_the_window_of_an_app_in_the_hub() {
        let (hub, _f) = hub_with_fakes("set-window");
        record(&hub, &Installed::new(flatpak_app("demo")));
        set_window(&hub, "demo", "own").unwrap();
        assert_eq!(
            recorded(&hub, "demo").unwrap().window,
            tools::WindowMode::Own
        );
        set_window(&hub, "demo", "hub").unwrap();
        assert_eq!(
            recorded(&hub, "demo").unwrap().window,
            tools::WindowMode::Hub
        );
    }

    #[test]
    fn only_apps_in_the_hub_get_a_window_setting() {
        let (hub, _f) = hub_with_fakes("set-window-refused");
        let with = |id: &str, manifest: &str| {
            let mut e = flatpak_app(id);
            e.manifest = Some(toml::from_str(manifest).unwrap());
            let inst = Installed::new(e);
            record(&hub, &inst);
        };
        with("socket", "live = true");
        with("standalone", "embed = false");
        with("page", "ui = \"index.html\"");
        with("web", "web_url = \"http://localhost:1/\"");
        record(&hub, &Installed::new(flatpak_app("plain")));
        let before = |id: &str| std::fs::read(hub.tools_dir.join(id).join("tool.json")).unwrap();
        for (id, mode) in [
            ("socket", "own"),
            ("standalone", "own"),
            ("page", "own"),
            ("web", "own"),
            ("plain", "x"),
            ("plain", "Own"),
        ] {
            let saved = before(id);
            assert!(set_window(&hub, id, mode).is_err(), "{id} {mode}");
            assert_eq!(before(id), saved, "{id} {mode}");
        }
        assert!(set_window(&hub, "missing", "own").is_err());
        assert!(set_window(&hub, "../plain", "own").is_err());
        assert!(!hub.tools_dir.join("missing").exists());
    }

    #[tokio::test]
    async fn tool_view_tells_integration_and_window() {
        let (hub, f) = hub_with_fakes("tool-view-window");
        f.flatpak_with(&[tools::Installation::User]);
        let mut inst = Installed::new(flatpak_app("demo"));
        inst.window = tools::WindowMode::Own;
        record(&hub, &inst);
        let views = tool_views(&hub).await;
        let view = views.iter().find(|v| v.entry.id == "demo").unwrap();
        assert_eq!(view.integration, tools::Integration::Embedded);
        assert_eq!(view.window, tools::WindowMode::Own);
    }

    /// An adopted app that starts at login is marked, found by its Flatpak
    /// ID or its program; an app tuxhub installed is not.
    #[tokio::test]
    async fn tool_view_tells_autostart() {
        let (hub, f) = hub_with_fakes("tool-view-autostart");
        f.flatpak_with(&[tools::Installation::User]);
        std::fs::create_dir_all(&hub.autostart_dir).unwrap();
        std::fs::write(
            hub.autostart_dir.join("a.desktop"),
            "[Desktop Entry]\nType=Application\nName=A\nExec=flatpak run net.example.demo\nX-Flatpak=net.example.demo\n",
        )
        .unwrap();
        std::fs::write(
            hub.autostart_dir.join("b.desktop"),
            "[Desktop Entry]\nType=Application\nName=B\nExec=/usr/bin/launched --background\n",
        )
        .unwrap();
        let mut demo = Installed::new(flatpak_app("demo"));
        demo.adopted = true;
        record(&hub, &demo);
        let mut own = Installed::new(flatpak_app("own"));
        own.adopted = true;
        record(&hub, &own);
        let installed = Installed::new(workshop_entry(
            "installed",
            json!({ "type": "flatpak", "flatpak_id": "net.example.demo" }),
        ));
        record(&hub, &installed);
        for (id, program) in [
            ("launched", "/usr/bin/launched"),
            ("other", "/usr/bin/other"),
        ] {
            let mut inst = Installed::new(workshop_entry(
                id,
                json!({ "type": "launcher", "desktop_id": format!("{id}.desktop"), "command": [program] }),
            ));
            inst.adopted = true;
            record(&hub, &inst);
        }
        let views = tool_views(&hub).await;
        let autostart = |id: &str| views.iter().find(|v| v.entry.id == id).unwrap().autostart;
        assert!(autostart("demo"));
        assert!(!autostart("own"));
        assert!(!autostart("installed"));
        assert!(autostart("launched"));
        assert!(!autostart("other"));
    }

    /// Edge case 9: an app in the hub that the user runs in its own window
    /// is noticed, not only apps with `embed = false`.
    #[tokio::test]
    async fn notices_an_app_in_its_own_window() {
        let (hub, base, tool) = hub_with_tool("own-window", json!({ "run": { "command": "app" } }));
        let inst = tools::load(&hub.tools_dir, "livedemo").unwrap();
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Stopped);
        let mut child = tools::spawn_copied(&tool.join("app"), &["30"]).await;
        assert!(app_is_running(&hub, &inst).await);
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Own);
        assert_eq!(run_state(&hub, true, &inst).await, RunState::Hub);
        quit_app(&hub, &inst, std::time::Duration::from_secs(3))
            .await
            .unwrap();
        assert!(exits_soon(&mut child).await, "the program still runs");
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Stopped);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Edge case 10: a Flatpak in its own window is found with `flatpak ps`;
    /// without an answer the state is unknown.
    #[tokio::test]
    async fn run_state_of_a_flatpak() {
        let (hub, f) = hub_with_fakes("run-state-flatpak");
        let inst = Installed::new(flatpak_app("demo"));
        record(&hub, &inst);
        f.set("flatpak", "echo net.example.demo");
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Own);
        f.set("flatpak", "echo net.example.other");
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Stopped);
        f.set("flatpak", "exit 1");
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Unknown);
        assert_eq!(run_state(&hub, true, &inst).await, RunState::Hub);
    }

    #[tokio::test]
    async fn quitting_a_flatpak_uses_flatpak_kill() {
        let (hub, f) = hub_with_fakes("quit-flatpak");
        let state = f.dir.join("running");
        std::fs::write(&state, "net.example.demo\n").unwrap();
        f.set(
            "flatpak",
            &format!(
                "case \"$1\" in ps) cat '{0}';; kill) : > '{0}';; esac",
                state.display()
            ),
        );
        let inst = Installed::new(flatpak_app("demo"));
        record(&hub, &inst);
        // Removing a copy or uninstalling leaves a running Flatpak alone, as
        // before; only quitting it and updates end it.
        quit_app(&hub, &inst, std::time::Duration::from_millis(300))
            .await
            .unwrap();
        assert!(!f.calls().iter().any(|c| c.starts_with("flatpak kill")));
        quit_running(&hub, &inst, std::time::Duration::from_millis(300))
            .await
            .unwrap();
        assert!(
            f.calls()
                .contains(&"flatpak kill net.example.demo".to_string())
        );
        assert_eq!(run_state(&hub, false, &inst).await, RunState::Stopped);
    }

    /// Edge case 11: the update of a running app with fake steps.
    #[tokio::test]
    async fn updates_a_running_app_in_order() {
        use std::sync::{Arc, Mutex};
        let steps = Arc::new(Mutex::new(Vec::<&str>::new()));
        let step = |name: &'static str, ok: bool| {
            let steps = steps.clone();
            move || async move {
                steps.lock().unwrap().push(name);
                if ok {
                    Ok(())
                } else {
                    Err(format!("{name} failed"))
                }
            }
        };

        let out = update_running(
            RunState::Own,
            true,
            step("quit", true),
            step("update", true),
        )
        .await
        .unwrap();
        assert_eq!(out.state, RunState::Own);
        assert_eq!(out.error, None);
        assert_eq!(*steps.lock().unwrap(), ["quit", "update"]);

        // Quitting fails: nothing is updated, nothing to restart.
        steps.lock().unwrap().clear();
        let err = update_running(
            RunState::Own,
            true,
            step("quit", false),
            step("update", true),
        )
        .await
        .unwrap_err();
        assert_eq!(err, "quit failed");
        assert_eq!(*steps.lock().unwrap(), ["quit"]);

        // The update fails: the app is started again where it ran.
        steps.lock().unwrap().clear();
        let out = update_running(
            RunState::Own,
            true,
            step("quit", true),
            step("update", false),
        )
        .await
        .unwrap();
        assert_eq!(out.state, RunState::Own);
        assert_eq!(out.error.as_deref(), Some("update failed"));
        assert_eq!(*steps.lock().unwrap(), ["quit", "update"]);

        // A stopped app is not quit.
        steps.lock().unwrap().clear();
        let out = update_running(
            RunState::Stopped,
            false,
            step("quit", true),
            step("update", true),
        )
        .await
        .unwrap();
        assert_eq!(out.state, RunState::Stopped);
        assert_eq!(*steps.lock().unwrap(), ["update"]);

        // A running app is never quit without the user's yes.
        for before in [RunState::Hub, RunState::Own, RunState::Unknown] {
            steps.lock().unwrap().clear();
            assert!(
                update_running(before, false, step("quit", true), step("update", true))
                    .await
                    .is_err()
            );
            assert!(steps.lock().unwrap().is_empty(), "{before:?}");
        }
    }

    /// An update of a downloaded app keeps "Own window" in its record.
    #[test]
    fn an_update_stores_the_kept_window() {
        let (hub, _f) = hub_with_fakes("update-stores-window");
        let release = workshop_entry("demo", json!({ "type": "github-release", "asset": "*" }));
        let mut previous = Installed::new(release.clone());
        previous.window = tools::WindowMode::Own;
        previous.version = Some("v1".into());
        // What tools::install leaves behind: a fresh record of the new version.
        let mut fresh = Installed::new(release);
        fresh.version = Some("v2".into());
        record(&hub, &fresh);
        let inst = record_after_install(&hub, Some(&previous), false, fresh).unwrap();
        assert_eq!(inst.window, tools::WindowMode::Own);
        let saved = recorded(&hub, "demo").unwrap();
        assert_eq!(saved.window, tools::WindowMode::Own);
        assert_eq!(saved.version.as_deref(), Some("v2"));
    }
}
