//! Tool manifest (`tuxhub.toml` in a tool release, or inline in a workshop
//! entry).
//!
//! Every field is optional. A tool without a manifest is level 0 (launch only),
//! `web_url` makes it level 1, and `rpc`, `ui`, `settings` or `actions` make it
//! level 2. `live = true` makes it level 3.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Required for locally linked tools only; otherwise the workshop provides it.
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

    /// Launch as a regular application (own window).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<Cmd>,
    /// `false`: the application never runs inside tuxhub; its page only
    /// starts and stops it. For overlays and other windows that belong on the
    /// desktop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed: Option<bool>,
    /// The running application offers the live interface (level 3) on
    /// `$XDG_RUNTIME_DIR/tuxhub/<id>.sock`: `live = true`, or a `[live]`
    /// section with the command that starts its service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<Live>,
    /// Background process that tuxhub talks to over JSON-RPC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc: Option<Cmd>,
    /// Runs before tuxhub removes the tool. It removes what the tool set up
    /// itself (login entries, launchers), and for tools installed outside
    /// tuxhub the installation itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uninstall: Option<Cmd>,
    /// HTML page relative to the tool folder, shown inside tuxhub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    /// Existing web interface of the tool (level 1).
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
#[serde(untagged)]
pub enum Live {
    On(bool),
    /// tuxhub starts the service in the background when it is needed.
    Start(Cmd),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Relative to the tool folder.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// `rpc` only: start the process together with tuxhub instead of on first
    /// use, for tools that work in the background.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub autostart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub label: String,
    /// `number`, `select`, `toggle` or `text`
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
    /// Preferred global hotkey in XDG format, for example `F8` or `CTRL+SHIFT+k`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

impl Manifest {
    /// Reads `tuxhub.toml` from `dir`, if present.
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
        for cmd in [
            self.run.as_ref(),
            self.rpc.as_ref(),
            self.uninstall.as_ref(),
            self.live_start(),
        ]
        .into_iter()
        .flatten()
        {
            if !safe_relative(&cmd.command) {
                return Err(format!(
                    "command {:?} must be relative to the tool folder",
                    cmd.command
                ));
            }
        }
        if let Some(ui) = &self.ui
            && !safe_relative(ui)
        {
            return Err(format!("ui {ui:?} must be relative to the tool folder"));
        }
        Ok(())
    }

    /// 0 = launch only, 1 = web interface, 2 = fully integrated, 3 = live
    /// interface of the running application
    pub fn level(&self) -> u8 {
        if self.is_live() {
            3
        } else if self.rpc.is_some() || self.ui.is_some() || !self.settings.is_empty() {
            2
        } else if self.web_url.is_some() {
            1
        } else {
            0
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self.live, Some(Live::On(true) | Live::Start(_)))
    }

    /// Command that starts the live service, from the `[live]` section.
    pub fn live_start(&self) -> Option<&Cmd> {
        match &self.live {
            Some(Live::Start(cmd)) => Some(cmd),
            _ => None,
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

/// Ids end up in file paths: only `a-z0-9-_.` are allowed, and no `..`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.contains("..")
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_is_switched_off_explicitly() {
        // Tools without a manifest, or without the field, can run inside tuxhub.
        assert_eq!(Manifest::default().embed, None);
        let m: Manifest = toml::from_str("name = \"Clock\"").unwrap();
        assert_eq!(m.embed, None);
        let m: Manifest = toml::from_str("embed = false").unwrap();
        assert_eq!(m.embed, Some(false));
        assert_eq!(m.level(), 0);
    }

    #[test]
    fn live_interface_is_level_3() {
        assert!(Manifest::default().live.is_none());
        let m: Manifest = toml::from_str("embed = false\nlive = true").unwrap();
        assert!(m.is_live());
        assert_eq!(m.level(), 3);
        // Also above a web interface or an own process.
        let m: Manifest = toml::from_str("live = true\nweb_url = \"http://127.0.0.1:1\"").unwrap();
        assert_eq!(m.level(), 3);
        let m: Manifest = toml::from_str("live = false").unwrap();
        assert_eq!(m.level(), 0);
    }

    #[test]
    fn live_section_makes_a_startable_live_tool() {
        let m: Manifest =
            toml::from_str("[live]\ncommand = \"bin/tool\"\nargs = [\"daemon\"]").unwrap();
        assert!(m.is_live());
        assert_eq!(m.level(), 3);
        let start = m.live_start().unwrap();
        assert_eq!(start.command, "bin/tool");
        assert_eq!(start.args, ["daemon"]);
        m.validate().unwrap();
        // `live = true` alone: live, but tuxhub cannot start it.
        let m: Manifest = toml::from_str("live = true").unwrap();
        assert!(m.is_live());
        assert!(m.live_start().is_none());
        assert!(
            !toml::from_str::<Manifest>("live = false")
                .unwrap()
                .is_live()
        );
    }

    /// Edge case 14: the start command stays inside the tool folder.
    #[test]
    fn live_command_outside_the_tool_folder_is_refused() {
        for cmd in ["../evil", "bin/../../evil", "/usr/bin/evil", ""] {
            let m: Manifest = toml::from_str(&format!("[live]\ncommand = {cmd:?}")).unwrap();
            assert!(m.validate().is_err(), "{cmd:?} was accepted");
        }
    }
}
