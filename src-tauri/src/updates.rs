//! Periodic check for new releases of installed tools and of the other
//! apps of the start menu.

use crate::{Hub, local_apps, local_updates, system, tools};
use std::collections::BTreeMap;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// First check shortly after startup, then every six hours.
const FIRST_CHECK: Duration = Duration::from_secs(15);
const INTERVAL: Duration = Duration::from_secs(6 * 3600);

pub fn spawn_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let enabled = app.state::<Hub>().settings.lock().unwrap().check_updates;
            if enabled {
                check_all(&app).await;
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

/// Checks all GitHub-based tools and Flatpaks and returns `tool id -> new tag`.
pub async fn check_all(app: &AppHandle) -> BTreeMap<String, String> {
    let hub = app.state::<Hub>();
    // Runs shortly after startup, so installed tools pick up changes of their
    // workshop entry without opening the workshop.
    let (entries, _) = crate::workshop_entries().await;
    if tools::refresh_entries(&hub.tools_dir, &entries) > 0 {
        let _ = app.emit("tools-changed", ());
    }
    let installed = tools::load_all(&hub.tools_dir);
    let mut found = BTreeMap::new();
    for inst in installed {
        match tools::check_update(&inst, &hub.programs).await {
            Ok(Some(tag)) => {
                found.insert(inst.entry.id.clone(), tag);
            }
            Ok(None) => {}
            Err(e) => eprintln!("update check for {} failed: {e}", inst.entry.id),
        }
    }
    set(app, found.clone());
    check_local(app).await;
    found
}

/// Checks every Flatpak and package of the start menu at once and returns
/// `desktop id -> new version`.
pub async fn check_local(app: &AppHandle) -> BTreeMap<String, String> {
    let hub = app.state::<Hub>();
    let catalog = crate::local_catalog();
    let apps = local_apps::list_cached(&hub.local_apps, &hub.programs, &catalog).await;
    let versions = local_updates::check(&hub.programs, &apps).await;
    let records = tools::load_all(&hub.tools_dir);
    let pending = local_updates::Pending::new(versions.clone(), &apps, |a| {
        records
            .iter()
            .find(|r| local_apps::same_app(&r.entry, a))
            .map(|r| r.entry.id.clone())
    });
    *hub.local_updates.lock().unwrap() = pending;
    refresh_tray(app);
    let _ = app.emit("local-updates-changed", &versions);
    versions
}

/// Drops the pending updates of apps that were just updated.
pub fn forget_local(app: &AppHandle, desktop_ids: &[String]) {
    let hub = app.state::<Hub>();
    let versions = {
        let mut pending = hub.local_updates.lock().unwrap();
        for d in desktop_ids {
            pending.versions.remove(d);
            pending.what.remove(d);
        }
        pending.versions.clone()
    };
    refresh_tray(app);
    let _ = app.emit("local-updates-changed", &versions);
}

fn refresh_tray(app: &AppHandle) {
    let hub = app.state::<Hub>();
    let tools = hub.updates.lock().unwrap().clone();
    let count = hub.local_updates.lock().unwrap().count_with(&tools);
    system::set_tray_updates(app, count);
}

pub fn set(app: &AppHandle, map: BTreeMap<String, String>) {
    *app.state::<Hub>().updates.lock().unwrap() = map.clone();
    refresh_tray(app);
    let _ = app.emit("updates-changed", map);
}

/// Removes the entry after the tool was installed or removed.
pub fn forget(app: &AppHandle, id: &str) {
    let mut map = app.state::<Hub>().updates.lock().unwrap().clone();
    if map.remove(id).is_some() {
        set(app, map);
    }
}
