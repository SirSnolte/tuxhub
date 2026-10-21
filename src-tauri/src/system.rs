//! Hub settings, tray icon, autostart and window behavior.
//!
//! tuxly keeps running in the background so that global hotkeys work while
//! the window is closed.

use crate::Hub;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

pub const MINIMIZED_ARG: &str = "--minimized";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HubSettings {
    /// Start automatically at login.
    pub autostart: bool,
    /// When started at login, show only the tray icon.
    pub start_minimized: bool,
    /// Closing the window keeps tuxly running in the tray.
    pub close_to_tray: bool,
    /// Check for tool updates periodically.
    pub check_updates: bool,
}

impl Default for HubSettings {
    fn default() -> Self {
        Self {
            autostart: false,
            start_minimized: true,
            close_to_tray: true,
            check_updates: true,
        }
    }
}

impl HubSettings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap())
    }
}

/// Whether the system language is German. Used for the few texts shown
/// outside the web view (tray menu, desktop entries).
fn german() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .is_some_and(|l| l.starts_with("de"))
}

fn tr(en: &'static str, de: &'static str) -> &'static str {
    if german() { de } else { en }
}

fn autostart_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("autostart/io.github.sirsnolte.tuxly.desktop"))
}

/// Creates or removes the autostart entry. Also called at startup so the
/// entry always points to the running installation.
pub fn apply_autostart(s: &HubSettings) -> Result<(), String> {
    let path = autostart_file().ok_or("HOME is not set")?;
    if !s.autostart {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    // When running as an AppImage, point to the AppImage, not the mount point.
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or("the executable path is unknown")?;
    let args = if s.start_minimized {
        format!(" {MINIMIZED_ARG}")
    } else {
        String::new()
    };
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=tuxly\nComment={}\n\
         Exec=\"{}\"{args}\nIcon=tuxly\nX-GNOME-Autostart-enabled=true\n",
        tr(
            "Modular hub for gaming on Linux",
            "Modularer Hub für Gaming unter Linux"
        ),
        exe.display()
    );
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Menu items that change at runtime.
pub struct TrayItems {
    pub updates: MenuItem<Wry>,
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(
        app,
        "show",
        tr("Open tuxly", "tuxly öffnen"),
        true,
        None::<&str>,
    )?;
    let updates = MenuItem::with_id(
        app,
        "updates",
        tr("No updates", "Keine Updates"),
        false,
        None::<&str>,
    )?;
    let stop = MenuItem::with_id(
        app,
        "stop-all",
        tr("Stop all tool processes", "Alle Tool-Prozesse beenden"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", tr("Quit", "Beenden"), true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &updates, &sep, &stop, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("tuxly")
        .menu(&menu)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "show" | "updates" => show_window(app),
            "stop-all" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    app.state::<Hub>().runtime.stop_all().await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(TrayItems { updates });
    Ok(())
}

/// Updates the update counter in the tray menu.
pub fn set_tray_updates(app: &AppHandle, count: usize) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let text = match count {
            0 => tr("No updates", "Keine Updates").to_string(),
            1 => tr("1 tool update available", "1 Tool-Update verfügbar").to_string(),
            n if german() => format!("{n} Tool-Updates verfügbar"),
            n => format!("{n} tool updates available"),
        };
        let _ = items.updates.set_text(text);
        let _ = items.updates.set_enabled(count > 0);
    }
}
