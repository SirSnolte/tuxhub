//! Global hotkeys.
//!
//! Every action with a `shortcut` in a `tuxhub.toml`, and every hotkey a tool
//! reports at runtime, is registered as `<tool>::<action>`. On Linux this goes
//! through the XDG GlobalShortcuts portal, which works on Wayland (KDE Plasma,
//! recent GNOME) without reading keyboard input; the desktop decides the final
//! key binding, and users can change it there. On Windows the keys are
//! registered with the system as the tool names them.

use crate::{Hub, tools};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

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

fn set_status(app: &AppHandle, status: Status) {
    *app.state::<Hub>().shortcuts.lock().unwrap() = status.clone();
    let _ = app.emit("shortcuts-changed", status);
}

/// A hotkey to register: `<tool>::<action>`, description, preferred trigger.
type Wanted = (String, String, String);

/// The hotkeys of all installed tools.
fn wanted(app: &AppHandle) -> Vec<Wanted> {
    let mut wanted = Vec::new();
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
    wanted
}

/// Runs the action of an activated hotkey `<tool>::<action>`.
async fn activated(app: &AppHandle, id: &str) {
    let Some((tool, action)) = id.split_once("::") else {
        return;
    };
    if let Err(e) = crate::run_action(app, tool, action).await {
        let _ = app.emit("toast", format!("{tool}: {e}"));
    }
}

/// Registers (again) the hotkeys of all installed tools.
pub fn rebind(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let wanted = wanted(&app);
        if let Err(e) = backend::bind(&app, generation, wanted).await {
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

/// Opens the desktop's dialog to change the keys, where there is one.
pub async fn configure() -> Result<(), String> {
    backend::configure().await
}

/// The status after binding nothing.
fn none_wanted(app: &AppHandle) {
    set_status(
        app,
        Status {
            available: true,
            message: String::new(),
            bindings: vec![],
        },
    );
}

/// Converts a trigger in the portal's notation (`CTRL+SHIFT+F8`, `LOGO+a`)
/// into the notation Windows hotkeys are parsed from (`ctrl+shift+F8`,
/// `super+a`). Unknown modifiers are kept and fail when registered.
#[cfg(any(windows, test))]
pub fn accelerator(trigger: &str) -> String {
    trigger
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| match p.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => "ctrl".to_string(),
            "SHIFT" => "shift".to_string(),
            "ALT" => "alt".to_string(),
            "LOGO" | "SUPER" | "META" | "WIN" => "super".to_string(),
            _ if p.len() == 1 => p.to_ascii_uppercase(),
            _ => p.to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(target_os = "linux")]
mod backend {
    use super::*;
    use ashpd::desktop::Session;
    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
    use futures_util::StreamExt;
    use tokio::sync::Mutex;

    /// Active portal session, kept for rebinding and the configuration dialog.
    struct Active {
        portal: GlobalShortcuts,
        session: Session<GlobalShortcuts>,
    }
    static SESSION: Mutex<Option<Active>> = Mutex::const_new(None);

    const APP_ID: &str = "io.github.sirsnolte.tuxhub";

    /// Registers tuxhub with the portal under its application id. The portal
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
            "[Desktop Entry]\nType=Application\nName=tuxhub\nExec=\"{}\"\nIcon=tuxhub\nNoDisplay=true\n",
            exe.display()
        );
        let path = dir.join(&name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
            std::fs::write(path, content)?;
        }
        Ok(())
    }

    pub async fn bind(app: &AppHandle, generation: u64, wanted: Vec<Wanted>) -> Result<(), String> {
        let mut slot = SESSION.lock().await;
        if let Some(old) = slot.take() {
            let _ = old.session.close().await;
        }
        if wanted.is_empty() {
            none_wanted(app);
            return Ok(());
        }

        register_app_id().await;
        let portal_err =
            |e: ashpd::Error| format!("the global shortcuts portal is not available: {e}");
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
                super::activated(&app, ev.shortcut_id()).await;
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
}

#[cfg(windows)]
mod backend {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    /// Registered hotkey -> `<tool>::<action>`.
    static BOUND: Mutex<Option<HashMap<u32, String>>> = Mutex::new(None);

    pub async fn bind(
        app: &AppHandle,
        _generation: u64,
        wanted: Vec<Wanted>,
    ) -> Result<(), String> {
        let gs = app.global_shortcut();
        gs.unregister_all().map_err(|e| e.to_string())?;
        *BOUND.lock().unwrap() = Some(HashMap::new());
        if wanted.is_empty() {
            none_wanted(app);
            return Ok(());
        }
        let mut bindings = Vec::new();
        let mut failed = Vec::new();
        for (id, desc, trigger) in wanted {
            let keys = accelerator(&trigger);
            let registered = keys
                .parse::<Shortcut>()
                .map_err(|e| e.to_string())
                .and_then(|s| gs.register(s).map(|()| s).map_err(|e| e.to_string()));
            match registered {
                Ok(shortcut) => {
                    BOUND
                        .lock()
                        .unwrap()
                        .get_or_insert_default()
                        .insert(shortcut.id(), id.clone());
                    let (tool, action) = id.split_once("::").unwrap_or((&id, ""));
                    bindings.push(Binding {
                        tool: tool.into(),
                        action: action.into(),
                        label: desc,
                        trigger: keys,
                    });
                }
                Err(e) => failed.push(format!("{desc} ({keys}): {e}")),
            }
        }
        set_status(
            app,
            Status {
                available: true,
                message: failed.join("; "),
                bindings,
            },
        );
        Ok(())
    }

    /// Called by the global shortcut plugin for every key press.
    pub fn handle(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
        if state != ShortcutState::Pressed {
            return;
        }
        let id = BOUND
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(&shortcut.id()).cloned());
        if let Some(id) = id {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { activated(&app, &id).await });
        }
    }

    pub async fn configure() -> Result<(), String> {
        Err("on Windows, the keys are set in each tool's settings".into())
    }
}

#[cfg(windows)]
pub use backend::handle;

#[cfg(test)]
mod tests {
    use super::accelerator;

    #[test]
    fn portal_triggers_become_accelerators() {
        assert_eq!(accelerator("CTRL+SHIFT+F8"), "ctrl+shift+F8");
        assert_eq!(accelerator("LOGO+a"), "super+A");
        assert_eq!(accelerator("F8"), "F8");
        assert_eq!(accelerator(" ALT + Space "), "alt+Space");
    }
}
