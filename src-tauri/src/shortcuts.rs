//! Global hotkeys through the XDG GlobalShortcuts portal.
//!
//! This works on Wayland (KDE Plasma, recent GNOME) without reading keyboard
//! input. Every action with a `shortcut` in a `tuxly.toml`, and every hotkey a
//! tool reports at runtime, is registered as `<tool>::<action>`. The desktop
//! decides the final key binding, and users can change it there.

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

/// Generation of the current binding; listeners of older generations exit.
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Active portal session, kept for rebinding and the configuration dialog.
struct Active {
    portal: GlobalShortcuts,
    session: Session<GlobalShortcuts>,
}
static SESSION: Mutex<Option<Active>> = Mutex::const_new(None);

fn set_status(app: &AppHandle, status: Status) {
    *app.state::<Hub>().shortcuts.lock().unwrap() = status.clone();
    let _ = app.emit("shortcuts-changed", status);
}

const APP_ID: &str = "io.github.sirsnolte.tuxly";

/// Registers tuxly with the portal under its application id. The portal
/// requires a matching `.desktop` file; if none is installed, one is created
/// for the current user. Registration is possible once per process.
async fn register_app_id() {
    static DONE: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    DONE.get_or_init(|| async {
        if let Err(e) = ensure_desktop_file() {
            eprintln!("hotkeys: could not create the desktop entry: {e}");
        }
        match APP_ID.try_into() {
            Ok(app_id) => {
                if let Err(e) = ashpd::register_host_app(app_id).await {
                    eprintln!("hotkeys: application id registration failed: {e}");
                }
            }
            Err(e) => eprintln!("hotkeys: invalid application id: {e}"),
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
    // Only used for the portal; hidden so the menu shows the packaged entry once.
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=tuxly\nExec=\"{}\"\nIcon=tuxly\nNoDisplay=true\n",
        exe.display()
    );
    let path = dir.join(&name);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
        std::fs::write(path, content)?;
    }
    Ok(())
}

/// Registers (again) the hotkeys of all installed tools.
pub fn rebind(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = bind(&app).await {
            eprintln!("hotkeys: {e}");
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
            for d in hub.dyn_shortcuts(&inst.entry.id) {
                let desc = format!("{}: {}", inst.entry.name, d.label);
                wanted.push((format!("{}::{}", inst.entry.id, d.id), desc, d.shortcut));
            }
        }
    }
    if wanted.is_empty() {
        set_status(
            app,
            Status {
                available: true,
                message: String::new(),
                bindings: vec![],
            },
        );
        return Ok(());
    }

    register_app_id().await;
    let portal_err = |e: ashpd::Error| format!("the global shortcuts portal is not available: {e}");
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
    // If the user cancels the desktop dialog, the hotkeys are still
    // registered, just unassigned. Query the current state in that case.
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
        Err(e) => return Err(format!("the hotkeys were not confirmed: {e}")),
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
    set_status(
        app,
        Status {
            available: true,
            message: String::new(),
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

/// Opens the hotkey configuration dialog of the desktop (portal version 2 or later).
pub async fn configure() -> Result<(), String> {
    let slot = SESSION.lock().await;
    let active = slot.as_ref().ok_or("no hotkeys are registered")?;
    if active.portal.version() < 2 {
        return Err(
            "the desktop does not support this; change the keys in the system settings".into(),
        );
    }
    active
        .portal
        .configure_shortcuts(&active.session, None, Default::default())
        .await
        .map_err(|e| e.to_string())
}
