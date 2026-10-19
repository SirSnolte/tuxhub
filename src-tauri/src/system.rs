//! Hub-Einstellungen, Tray-Icon, Autostart und Fensterverhalten.
//!
//! Der Hub soll im Hintergrund weiterlaufen, damit globale Hotkeys auch bei
//! geschlossenem Fenster funktionieren.

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
    /// Beim Anmelden automatisch starten
    pub autostart: bool,
    /// Beim Autostart nur im Tray starten
    pub start_minimized: bool,
    /// Fenster schließen = in den Tray minimieren
    pub close_to_tray: bool,
    /// Regelmäßig nach Tool-Updates suchen
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

fn autostart_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("autostart/dev.tuxhub.app.desktop"))
}

/// Legt den Autostart-Eintrag an oder entfernt ihn.
pub fn apply_autostart(s: &HubSettings) -> Result<(), String> {
    let path = autostart_file().ok_or("HOME nicht gesetzt")?;
    if !s.autostart {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    // Als AppImage gestartet: auf das AppImage zeigen, nicht auf den entpackten Pfad.
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or("Programmpfad unbekannt")?;
    let args = if s.start_minimized {
        format!(" {MINIMIZED_ARG}")
    } else {
        String::new()
    };
    let content = format!(
        "[Desktop Entry]\nType=Application\nName=tuxhub\nComment=Modularer Hub für Linux-Gaming\n\
         Exec=\"{}\"{args}\nIcon=applications-games\nX-GNOME-Autostart-enabled=true\n",
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

/// Menüeinträge, die sich später ändern (z. B. Anzahl Updates).
pub struct TrayItems {
    pub updates: MenuItem<Wry>,
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "tuxhub öffnen", true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "Keine Updates", false, None::<&str>)?;
    let stop = MenuItem::with_id(
        app,
        "stop-all",
        "Alle Tool-Prozesse beenden",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &updates, &sep, &stop, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("tuxhub")
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

/// Aktualisiert den Update-Eintrag im Tray-Menü.
pub fn set_tray_updates(app: &AppHandle, count: usize) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let text = match count {
            0 => "Keine Updates".to_string(),
            1 => "1 Update verfügbar".to_string(),
            n => format!("{n} Updates verfügbar"),
        };
        let _ = items.updates.set_text(text);
        let _ = items.updates.set_enabled(count > 0);
    }
}
