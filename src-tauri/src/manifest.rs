//! Tool-Manifest (`tuxhub.toml` im Release eines Tools oder inline im
//! Workshop-Eintrag).
//!
//! Alles ist optional: Ein Tool ohne Manifest ist Stufe 0 (nur starten).
//! `web_url` macht es zu Stufe 1, `rpc`/`ui`/`settings`/`actions` zu Stufe 2.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Nur für lokal verknüpfte Tools nötig; sonst kommt das aus dem Workshop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Normal starten (eigenes Fenster / eigenständig).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Cmd>,
    /// Hintergrundprozess, mit dem der Hub per JSON-RPC spricht.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc: Option<Cmd>,
    /// Eigene HTML-Seite, relativ zum Tool-Ordner. Wird im Hub als iframe gezeigt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    /// Vorhandenes Web-Interface des Tools (Stufe 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<Setting>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Relativ zum Tool-Ordner.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub label: String,
    /// `number`, `select`, `toggle` oder `text`
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub default: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub primary: bool,
    /// Gewünschter globaler Hotkey (XDG-Format, z. B. `F8` oder `CTRL+SHIFT+k`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

impl Manifest {
    /// Liest `tuxhub.toml` aus `dir`, falls vorhanden.
    pub fn load(dir: &Path) -> Result<Option<Self>, String> {
        let path = dir.join("tuxhub.toml");
        if !path.is_file() {
            return Ok(None);
        }
        let s = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let m: Manifest = toml::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))?;
        m.validate()?;
        Ok(Some(m))
    }

    pub fn validate(&self) -> Result<(), String> {
        for cmd in [&self.run, &self.rpc].into_iter().flatten() {
            if !safe_relative(&cmd.command) {
                return Err(format!(
                    "Befehl {:?} muss relativ zum Tool-Ordner sein",
                    cmd.command
                ));
            }
        }
        if let Some(ui) = &self.ui
            && !safe_relative(ui)
        {
            return Err(format!("ui {ui:?} muss relativ zum Tool-Ordner sein"));
        }
        Ok(())
    }

    /// 0 = nur starten, 1 = Web-UI, 2 = voll integriert
    pub fn level(&self) -> u8 {
        if self.rpc.is_some() || self.ui.is_some() || !self.settings.is_empty() {
            2
        } else if self.web_url.is_some() {
            1
        } else {
            0
        }
    }

    pub fn default_settings(&self) -> Map<String, Value> {
        self.settings
            .iter()
            .map(|s| (s.key.clone(), s.default.clone()))
            .collect()
    }
}

pub fn safe_relative(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('/') && !p.split('/').any(|c| c == "..")
}

/// IDs landen in Dateipfaden, deshalb nur `a-z0-9-_.` und kein `..`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.contains("..")
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}
