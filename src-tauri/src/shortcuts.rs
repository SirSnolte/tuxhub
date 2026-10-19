//! Globale Hotkeys über das XDG-GlobalShortcuts-Portal.
//!
//! Funktioniert unter Wayland (KDE, neuere GNOME-Versionen), ohne dass der Hub
//! Tastatureingaben mitlesen muss. Jede Aktion mit `shortcut` in einem
//! `tuxhub.toml` wird als `<tool>::<aktion>` registriert. Die endgültige
//! Taste legt der Desktop fest (der Nutzer kann sie dort ändern).

use crate::{tools, Hub};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::Session;
use futures_util::StreamExt;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub available: bool,
    pub message: String,
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Serialize)]
pub struct Binding {
    pub tool: String,
    pub action: String,
    pub label: String,
    pub trigger: String,
}

/// Generation der aktuellen Bindung; alte Listener beenden sich daran.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static SESSION: Mutex<Option<Session<GlobalShortcuts>>> = Mutex::const_new(None);

fn set_status(app: &AppHandle, status: Status) {
    *app.state::<Hub>().shortcuts.lock().unwrap() = status.clone();
    let _ = app.emit("shortcuts-changed", status);
}

/// (Neu) registriert alle Hotkeys der installierten Tools.
pub fn rebind(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = bind(&app).await {
            eprintln!("Hotkeys: {e}");
            set_status(&app, Status { available: false, message: e, bindings: vec![] });
        }
    });
}

async fn bind(app: &AppHandle) -> Result<(), String> {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let mut slot = SESSION.lock().await;
    if let Some(old) = slot.take() {
        let _ = old.close().await;
    }

    let mut wanted = Vec::new();
    {
        let hub = app.state::<Hub>();
        for inst in tools::load_all(&hub.tools_dir) {
            let m = inst.manifest(&hub.tools_dir);
            for a in m.actions {
                if let Some(trigger) = a.shortcut {
                    let desc = format!("{}: {}", inst.entry.name, a.label);
                    wanted.push((format!("{}::{}", inst.entry.id, a.id), desc, trigger));
                }
            }
        }
    }
    if wanted.is_empty() {
        set_status(app, Status {
            available: true,
            message: "Keine installierten Tools mit Hotkeys.".into(),
            bindings: vec![],
        });
        return Ok(());
    }

    // Ältere Portale kennen die Registry nicht – dann geht es trotzdem weiter.
    if let Ok(app_id) = "dev.tuxhub.app".try_into() {
        let _ = ashpd::register_host_app(app_id).await;
    }
    let portal_err = |e: ashpd::Error| format!("Hotkey-Portal nicht verfügbar: {e}");
    let gs = GlobalShortcuts::new().await.map_err(portal_err)?;
    let session = gs.create_session(Default::default()).await.map_err(portal_err)?;
    let shortcuts: Vec<NewShortcut> = wanted
        .iter()
        .map(|(id, desc, trigger)| NewShortcut::new(id.as_str(), desc.as_str()).preferred_trigger(trigger.as_str()))
        .collect();
    let bound = gs
        .bind_shortcuts(&session, &shortcuts, None, Default::default())
        .await
        .map_err(portal_err)?
        .response()
        .map_err(|e| format!("Hotkeys wurden nicht bestätigt: {e}"))?;

    let bindings = bound
        .shortcuts()
        .iter()
        .filter_map(|s| {
            let (tool, action) = s.id().split_once("::")?;
            Some(Binding {
                tool: tool.into(),
                action: action.into(),
                label: s.description().into(),
                trigger: s.trigger_description().into(),
            })
        })
        .collect();
    set_status(app, Status { available: true, message: String::new(), bindings });

    let mut activated = gs.receive_activated().await.map_err(portal_err)?;
    *slot = Some(session);
    drop(slot);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = activated.next().await {
            if GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            let Some((tool, action)) = ev.shortcut_id().split_once("::") else { continue };
            if let Err(e) = crate::run_action(&app, tool, action).await {
                let _ = app.emit("toast", format!("{tool}: {e}"));
            }
        }
    });
    Ok(())
}
