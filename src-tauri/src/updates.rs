//! Periodic check for new releases of installed tools.

use crate::{Hub, system, tools};
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

/// Checks all GitHub-based tools and returns `tool id -> new tag`.
pub async fn check_all(app: &AppHandle) -> BTreeMap<String, String> {
    let installed = tools::load_all(&app.state::<Hub>().tools_dir);
    let mut found = BTreeMap::new();
    for inst in installed {
        match tools::check_update(&inst).await {
            Ok(Some(tag)) => {
                found.insert(inst.entry.id.clone(), tag);
            }
            Ok(None) => {}
            Err(e) => eprintln!("update check for {} failed: {e}", inst.entry.id),
        }
    }
    set(app, found.clone());
    found
}

pub fn set(app: &AppHandle, map: BTreeMap<String, String>) {
    system::set_tray_updates(app, map.len());
    *app.state::<Hub>().updates.lock().unwrap() = map.clone();
    let _ = app.emit("updates-changed", map);
}

/// Removes the entry after the tool was installed or removed.
pub fn forget(app: &AppHandle, id: &str) {
    let mut map = app.state::<Hub>().updates.lock().unwrap().clone();
    if map.remove(id).is_some() {
        set(app, map);
    }
}
