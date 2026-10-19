//! Globale Hotkeys über das XDG-GlobalShortcuts-Portal.
//!
//! Funktioniert unter Wayland (KDE, neuere GNOME-Versionen), ohne dass der Hub
//! Tastatureingaben mitlesen muss. Jede Aktion mit `shortcut` in einem
//! `tuxhub.toml` wird als `<tool>::<aktion>` registriert. Die endgültige
//! Taste legt der Desktop fest (der Nutzer kann sie dort ändern).

use crate::{Hub, tools};
use ashpd::desktop::Session;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
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
/// Aktive Portal-Sitzung (für Neu-Bindung und den Einstellungsdialog).
struct Active {
    portal: GlobalShortcuts,
    session: Session<GlobalShortcuts>,
}
static SESSION: Mutex<Option<Active>> = Mutex::const_new(None);

fn set_status(app: &AppHandle, status: Status) {
    *app.state::<Hub>().shortcuts.lock().unwrap() = status.clone();
    let _ = app.emit("shortcuts-changed", status);
}

const APP_ID: &str = "dev.tuxhub.app";

/// Meldet den Hub beim Portal unter seiner App-ID an. Das Portal verlangt
/// dafür eine passende `.desktop`-Datei; fehlt sie, legt der Hub eine für den
/// aktuellen Nutzer an. Nur einmal pro Prozess möglich.
async fn register_app_id() {
    static DONE: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    DONE.get_or_init(|| async {
        if let Err(e) = ensure_desktop_file() {
            eprintln!("Hotkeys: .desktop-Datei konnte nicht angelegt werden: {e}");
        }
        match APP_ID.try_into() {
            Ok(app_id) => {
                if let Err(e) = ashpd::register_host_app(app_id).await {
                    eprintln!("Hotkeys: App-ID-Registrierung fehlgeschlagen: {e}");
                }
            }
            Err(e) => eprintln!("Hotkeys: ungültige App-ID: {e}"),
        }
    })
    .await;
}

fn ensure_desktop_file() -> std::io::Result<()> {
    let name = format!("{APP_ID}.desktop");
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    if std::env::split_paths(&data_dirs).any(|d| d.join("applications").join(&name).is_file()) {
        return Ok(());
    }
    let home = std::env::var_os("HOME").ok_or(std::io::ErrorKind::NotFound)?;
    let dir = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(&home).join(".local/share"))
        .join("applications");
    std::fs::create_dir_all(&dir)?;
    let exe = std::env::current_exe()?;
    // Im Entwicklungsmodus nicht im Startmenü anzeigen.
    let hidden = if cfg!(debug_assertions) {
        "NoDisplay=true\n"
    } else {
        ""
    };
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=tuxhub\nComment=Modularer Hub für Linux-Gaming\n\
         Exec=\"{}\"\nIcon=applications-games\nCategories=Game;Utility;\n{hidden}",
        exe.display()
    );
    let path = dir.join(&name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
        std::fs::write(path, content)?;
    }
    Ok(())
}

/// (Neu) registriert alle Hotkeys der installierten Tools.
pub fn rebind(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = bind(&app).await {
            eprintln!("Hotkeys: {e}");
            set_status(
                &app,
                Status {
                    available: false,
                    message: e,
                    bindings: vec![],
                },
            );
        }
    });
}

async fn bind(app: &AppHandle) -> Result<(), String> {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let mut slot = SESSION.lock().await;
    if let Some(old) = slot.take() {
        let _ = old.session.close().await;
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
        set_status(
            app,
            Status {
                available: true,
                message: "Keine installierten Tools mit Hotkeys.".into(),
                bindings: vec![],
            },
        );
        return Ok(());
    }

    register_app_id().await;
    let portal_err = |e: ashpd::Error| format!("Hotkey-Portal nicht verfügbar: {e}");
    let gs = GlobalShortcuts::new().await.map_err(portal_err)?;
    let session = gs
        .create_session(Default::default())
        .await
        .map_err(portal_err)?;
    let shortcuts: Vec<NewShortcut> = wanted
        .iter()
        .map(|(id, desc, trigger)| {
            NewShortcut::new(id.as_str(), desc.as_str()).preferred_trigger(trigger.as_str())
        })
        .collect();
    // Bricht der Nutzer den Dialog des Desktops ab, sind die Hotkeys trotzdem
    // registriert – nur eben nicht belegt. Dann den aktuellen Stand abfragen.
    let bound = match gs
        .bind_shortcuts(&session, &shortcuts, None, Default::default())
        .await
        .map_err(portal_err)?
        .response()
    {
        Ok(b) => b.shortcuts().to_vec(),
        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => gs
            .list_shortcuts(&session, Default::default())
            .await
            .map_err(portal_err)?
            .response()
            .map_err(portal_err)?
            .shortcuts()
            .to_vec(),
        Err(e) => return Err(format!("Hotkeys wurden nicht bestätigt: {e}")),
    };

    let bindings: Vec<Binding> = bound
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
    let unassigned = bindings.iter().any(|b| b.trigger.is_empty());
    set_status(
        app,
        Status {
            available: true,
            message: if unassigned {
                "Einige Hotkeys sind noch nicht belegt.".into()
            } else {
                String::new()
            },
            bindings,
        },
    );

    let mut activated = gs.receive_activated().await.map_err(portal_err)?;
    *slot = Some(Active {
        portal: gs,
        session,
    });
    drop(slot);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(ev) = activated.next().await {
            if GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            let Some((tool, action)) = ev.shortcut_id().split_once("::") else {
                continue;
            };
            if let Err(e) = crate::run_action(&app, tool, action).await {
                let _ = app.emit("toast", format!("{tool}: {e}"));
            }
        }
    });
    Ok(())
}

/// Öffnet den Hotkey-Dialog des Desktops (Portal-Version 2+).
pub async fn configure() -> Result<(), String> {
    let slot = SESSION.lock().await;
    let active = slot.as_ref().ok_or("Keine Hotkeys registriert")?;
    if active.portal.version() < 2 {
        return Err(
            "Dein Desktop unterstützt das nicht – bitte in den Systemeinstellungen ändern".into(),
        );
    }
    active
        .portal
        .configure_shortcuts(&active.session, None, Default::default())
        .await
        .map_err(|e| e.to_string())
}
