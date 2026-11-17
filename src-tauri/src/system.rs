//! Hub settings, tray icon, autostart and window behavior.
//!
//! tuxhub keeps running in the background so that global hotkeys work while
//! the window is closed.

use crate::Hub;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Closing the window keeps tuxhub running in the tray.
    pub close_to_tray: bool,
    /// Check for tool updates periodically.
    pub check_updates: bool,
    /// Desktop id -> the workshop area the user chose for the app.
    pub app_areas: BTreeMap<String, String>,
    /// How the workshop lists apps.
    pub workshop_view: WorkshopView,
    /// Workshop areas the user folded.
    pub workshop_collapsed: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkshopView {
    #[default]
    Grid,
    List,
}

/// The areas of the workshop, in their order.
pub const AREAS: [&str; 9] = [
    "gaming", "games", "comm", "media", "office", "graphics", "tools", "dev", "system",
];

/// Longest desktop id and most area choices kept.
const MAX_DESKTOP_ID: usize = 255;
const MAX_AREA_CHOICES: usize = 2000;
/// Largest `hub.json` that is read.
const MAX_SIZE: u64 = 64 * 1024;

impl Default for HubSettings {
    fn default() -> Self {
        Self {
            autostart: false,
            start_minimized: true,
            close_to_tray: true,
            check_updates: true,
            app_areas: BTreeMap::new(),
            workshop_view: WorkshopView::Grid,
            workshop_collapsed: vec!["system".into()],
        }
    }
}

impl HubSettings {
    pub fn load(path: &Path) -> Self {
        if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_SIZE) {
            eprintln!("{}: too large, ignored", path.display());
            return Self::default();
        }
        let mut s: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.app_areas
            .retain(|d, a| valid_desktop_id(d) && AREAS.contains(&a.as_str()));
        s.workshop_collapsed.retain(|a| AREAS.contains(&a.as_str()));
        s
    }

    /// Writes a temporary file and moves it over the old one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap())?;
        std::fs::rename(&tmp, path)
    }

    /// The area of an app: one of [`AREAS`], or `None` for the automatic one.
    pub fn set_area(&mut self, desktop_id: &str, area: Option<String>) -> Result<(), String> {
        if !valid_desktop_id(desktop_id) {
            return Err("invalid desktop id".into());
        }
        match area {
            None => {
                self.app_areas.remove(desktop_id);
            }
            Some(a) if AREAS.contains(&a.as_str()) => {
                if self.app_areas.len() >= MAX_AREA_CHOICES && !self.app_areas.contains_key(desktop_id) {
                    return Err("too many area choices".into());
                }
                self.app_areas.insert(desktop_id.to_string(), a);
            }
            Some(a) => return Err(format!("unknown area {a:?}")),
        }
        Ok(())
    }

    /// The workshop's view (`grid` or `list`) and folded areas.
    pub fn set_workshop_prefs(&mut self, view: &str, collapsed: Vec<String>) -> Result<(), String> {
        let view = match view {
            "grid" => WorkshopView::Grid,
            "list" => WorkshopView::List,
            v => return Err(format!("unknown view {v:?}")),
        };
        if let Some(a) = collapsed.iter().find(|a| !AREAS.contains(&a.as_str())) {
            return Err(format!("unknown area {a:?}"));
        }
        let mut folded: Vec<String> = Vec::new();
        for a in collapsed {
            if !folded.contains(&a) {
                folded.push(a);
            }
        }
        self.workshop_view = view;
        self.workshop_collapsed = folded;
        Ok(())
    }

    /// Takes the workshop's choices from `current`: the settings form sends
    /// only the general settings.
    pub fn keep_workshop(&mut self, current: &HubSettings) {
        self.app_areas = current.app_areas.clone();
        self.workshop_view = current.workshop_view;
        self.workshop_collapsed = current.workshop_collapsed.clone();
    }
}

fn valid_desktop_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_DESKTOP_ID && !id.contains('/')
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
    Some(config.join("autostart/io.github.sirsnolte.tuxhub.desktop"))
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
        "[Desktop Entry]\nType=Application\nName=tuxhub\nComment={}\n\
         Exec=\"{}\"{args}\nIcon=tuxhub\nX-GNOME-Autostart-enabled=true\n",
        tr(
            "Modular hub for gaming on Linux",
            "Modularer Hub für Gaming unter Linux"
        ),
        exe.display()
    );
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

/// Records the running version and returns the previous one when it differs,
/// that is on the first start after an update.
pub fn record_version(path: &Path, current: &str) -> Option<String> {
    let previous = std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string());
    if previous.as_deref() == Some(current) {
        return None;
    }
    if let Err(e) = std::fs::write(path, current) {
        eprintln!("version file: {e}");
    }
    previous.filter(|p| !p.is_empty())
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
        tr("Open tuxhub", "tuxhub öffnen"),
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

/// Updates the update counter in the tray menu.
pub fn set_tray_updates(app: &AppHandle, count: usize) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let text = match count {
            0 => tr("No updates", "Keine Updates").to_string(),
            1 => tr("1 update available", "1 Update verfügbar").to_string(),
            n if german() => format!("{n} Updates verfügbar"),
            n => format!("{n} tool updates available"),
        };
        let _ = items.updates.set_text(text);
        let _ = items.updates.set_enabled(count > 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_change_is_reported_once() {
        let dir = std::env::temp_dir().join(format!("tuxhub-version-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("version");
        // First start ever: nothing to report.
        assert_eq!(record_version(&path, "0.4.0"), None);
        assert_eq!(record_version(&path, "0.4.0"), None);
        // First start after an update.
        assert_eq!(record_version(&path, "0.4.1"), Some("0.4.0".into()));
        assert_eq!(record_version(&path, "0.4.1"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tuxhub-hubset-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("hub.json")
    }

    #[test]
    fn old_hub_json_loads_with_workshop_defaults() {
        let path = temp("old");
        std::fs::write(&path, r#"{"autostart":true,"check_updates":false}"#).unwrap();
        let s = HubSettings::load(&path);
        assert!(s.autostart && !s.check_updates);
        assert!(s.app_areas.is_empty());
        assert_eq!(s.workshop_view, WorkshopView::Grid);
        // System apps start folded.
        assert_eq!(s.workshop_collapsed, ["system"]);
    }

    #[test]
    fn area_choice_is_checked_and_none_resets() {
        let mut s = HubSettings::default();
        s.set_area("gimp.desktop", Some("media".into())).unwrap();
        assert_eq!(s.app_areas["gimp.desktop"], "media");
        assert!(s.set_area("gimp.desktop", Some("nope".into())).is_err());
        assert!(s.set_area(&"x".repeat(256), Some("media".into())).is_err());
        assert!(s.set_area("", Some("media".into())).is_err());
        assert_eq!(s.app_areas["gimp.desktop"], "media");
        s.set_area("gimp.desktop", None).unwrap();
        assert!(s.app_areas.is_empty());
    }

    #[test]
    fn workshop_prefs_are_checked() {
        let mut s = HubSettings::default();
        s.set_workshop_prefs("list", vec!["games".into(), "system".into()]).unwrap();
        assert_eq!(s.workshop_view, WorkshopView::List);
        assert_eq!(s.workshop_collapsed, ["games", "system"]);
        assert!(s.set_workshop_prefs("tiles", vec![]).is_err());
        assert!(s.set_workshop_prefs("grid", vec!["../x".into()]).is_err());
        assert_eq!(s.workshop_view, WorkshopView::List);
        // Duplicates are dropped.
        s.set_workshop_prefs("grid", vec!["dev".into(), "dev".into()]).unwrap();
        assert_eq!(s.workshop_collapsed, ["dev"]);
    }

    #[test]
    fn the_general_form_keeps_the_workshop_choices() {
        let mut current = HubSettings::default();
        current.set_area("a.desktop", Some("dev".into())).unwrap();
        current.set_workshop_prefs("list", vec![]).unwrap();
        // The settings form sends only its own fields.
        let mut sent: HubSettings = serde_json::from_str(r#"{"autostart":true}"#).unwrap();
        sent.keep_workshop(&current);
        assert!(sent.autostart);
        assert_eq!(sent.app_areas, current.app_areas);
        assert_eq!(sent.workshop_view, WorkshopView::List);
    }

    #[test]
    fn hub_json_is_written_atomically_and_read_with_a_limit() {
        let path = temp("atomic");
        let mut s = HubSettings::default();
        s.set_area("a.desktop", Some("dev".into())).unwrap();
        s.save(&path).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        assert_eq!(HubSettings::load(&path).app_areas, s.app_areas);
        std::fs::write(&path, format!("{{\"autostart\":true,\"x\":\"{}\"}}", " ".repeat(70_000))).unwrap();
        assert!(!HubSettings::load(&path).autostart);
    }
}
