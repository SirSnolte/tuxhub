//! Installed tools: install workshop entries, launch, update and remove them.
//!
//! Layout below `<data>/tools/<id>/`:
//! - `tool.json`: the workshop entry and the installed version
//! - `files/`: the extracted release (GitHub releases only)
//!
//! Tools installed outside tuxhub announce themselves with a file
//! `tuxhub/apps/<id>.toml` in `$XDG_DATA_HOME` or one of `$XDG_DATA_DIRS`
//! (see [`discover`]). They appear like any other installed tool.

use crate::embed::which;
use crate::github;
use crate::manifest::{Manifest, safe_relative, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::process::Command;

/// An entry of the workshop index (`registry/index.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// HTTPS URL of the icon, or a path relative to the tool folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// `owner/name` on GitHub.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub source: Source,
    /// Replaces a `tuxhub.toml` shipped by the tool, so third-party tools can
    /// be integrated without changes to their repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Manifest>,
    /// How deeply tuxhub integrates the app, shown in the workshop while it
    /// is not installed (the installed manifest wins). An unknown value, for
    /// example a level of a newer tuxhub, counts as missing.
    #[serde(
        default,
        deserialize_with = "lenient_integration",
        skip_serializing_if = "Option::is_none"
    )]
    pub integration: Option<Integration>,
    /// Localized texts by language code, for example `{"de": {"description": "…"}}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translations: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Source {
    /// Latest release of `repo`; `asset` is a pattern such as `*x86_64.AppImage`.
    GithubRelease { asset: String },
    Flatpak {
        flatpak_id: String,
        #[serde(default = "default_remote")]
        remote: String,
    },
    /// A distribution package from the latest release of `repo`, installed
    /// system-wide with the package manager (dnf, zypper or apt).
    SystemPackage {
        /// Package name, used to check and remove the installation.
        package: String,
        /// Asset pattern of the `.rpm` package, for example `*.x86_64.rpm`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rpm: Option<String>,
        /// Asset pattern of the `.deb` package, for example `*_amd64.deb`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deb: Option<String>,
        /// Absolute path of the program the package installs.
        command: String,
    },
    /// A local folder linked in developer mode.
    Local { path: String },
    /// Installed outside tuxhub and announced in a `tuxhub/apps/<id>.toml`.
    External {
        path: String,
        /// Release asset pattern; together with `repo` it allows updates.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        asset: Option<String>,
        /// Announced in the user's data folder: tuxhub may update and remove
        /// the installation. System-wide installations belong to the package
        /// manager.
        #[serde(default)]
        user: bool,
    },
    /// A package from the distribution's repositories, found on this
    /// computer; it is updated with the package manager, never from GitHub.
    DistroPackage {
        package: String,
        manager: PackageFormat,
        /// The start command of its desktop entry; the program by absolute path.
        command: Vec<String>,
    },
    /// An app found on this computer that tuxhub only starts (an AppImage
    /// or a program of unknown origin); it updates itself.
    Launcher {
        desktop_id: String,
        /// The start command of its desktop entry; the program by absolute path.
        command: Vec<String>,
    },
}

/// The package format of a distribution package, which tells the package
/// manager that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    Rpm,
    Deb,
}

/// Longest stored start command, in arguments and in bytes per argument.
const MAX_COMMAND_ARGS: usize = 64;
const MAX_ARG_BYTES: usize = 4096;

/// Whether a stored start command can be run without a shell: a program by
/// absolute path and a bounded list of arguments.
fn valid_command(command: &[String]) -> bool {
    command
        .first()
        .is_some_and(|program| program.starts_with('/') && !program.split('/').any(|c| c == ".."))
        && command.len() <= MAX_COMMAND_ARGS
        && command
            .iter()
            .all(|a| a.len() <= MAX_ARG_BYTES && !a.contains('\0'))
}

/// Whether a package name can be passed to a package manager as an argument.
fn valid_package_name(package: &str) -> bool {
    !package.is_empty()
        && !package.starts_with('-')
        && package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

/// How deeply tuxhub integrates an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Integration {
    /// tuxhub's own menu, live with the app over its socket.
    Socket,
    /// Runs inside tuxhub (xpra or its own page) or in its own window.
    Embedded,
    /// Always runs in its own window; tuxhub installs, starts and updates it.
    Standalone,
}

/// The integration a manifest describes.
pub fn integration_of(manifest: &Manifest) -> Integration {
    if manifest.is_live() {
        Integration::Socket
    } else if manifest.embed == Some(false) {
        Integration::Standalone
    } else {
        Integration::Embedded
    }
}

/// Reads `integration` without failing on unknown values: one bad entry must
/// not drop the whole workshop.
fn lenient_integration<'de, D>(de: D) -> Result<Option<Integration>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(de)?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()))
}

/// Whether a Flatpak ID or remote name is safe to pass to flatpak. It never
/// starts with "-", which flatpak would parse as an option.
pub(crate) fn valid_flatpak_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn default_remote() -> String {
    "flathub".into()
}

/// Brings the stored workshop entries of installed tools up to date with
/// the workshop (new descriptions, new settings, …). Only entries of the
/// same kind of installation are replaced; how the tool is installed stays.
/// Returns the number of updated tools.
pub fn refresh_entries(tools_dir: &Path, entries: &[Entry]) -> usize {
    let mut updated = 0;
    for mut inst in load_all(tools_dir) {
        let Some(entry) = entries.iter().find(|e| e.id == inst.entry.id) else {
            continue;
        };
        let same_kind =
            std::mem::discriminant(&entry.source) == std::mem::discriminant(&inst.entry.source);
        let changed = serde_json::to_value(entry).ok() != serde_json::to_value(&inst.entry).ok();
        if !same_kind || !changed {
            continue;
        }
        inst.entry = entry.clone();
        match save(tools_dir, &inst) {
            Ok(()) => updated += 1,
            Err(e) => eprintln!("{}: could not update the stored entry: {e}", inst.entry.id),
        }
    }
    updated
}

impl Entry {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("invalid tool id {:?}", self.id));
        }
        if matches!(
            self.source,
            Source::GithubRelease { .. } | Source::SystemPackage { .. }
        ) && self.repo.is_none()
        {
            return Err(format!("{}: a release source requires `repo`", self.id));
        }
        if let Source::SystemPackage {
            package,
            rpm,
            deb,
            command,
        } = &self.source
        {
            if !valid_package_name(package) {
                return Err(format!("{}: invalid package name {package:?}", self.id));
            }
            if rpm.is_none() && deb.is_none() {
                return Err(format!(
                    "{}: a system package needs `rpm` or `deb`",
                    self.id
                ));
            }
            if !command.starts_with('/') || command.split('/').any(|c| c == "..") {
                return Err(format!("{}: `command` must be an absolute path", self.id));
            }
        }
        if let Source::DistroPackage { package, .. } = &self.source
            && !valid_package_name(package)
        {
            return Err(format!("{}: invalid package name {package:?}", self.id));
        }
        if let Source::DistroPackage { command, .. } | Source::Launcher { command, .. } =
            &self.source
            && !valid_command(command)
        {
            return Err(format!(
                "{}: `command` must start with an absolute path",
                self.id
            ));
        }
        if let Source::Launcher { desktop_id, .. } = &self.source
            && (desktop_id.is_empty() || desktop_id.len() > 255)
        {
            return Err(format!("{}: invalid desktop file id", self.id));
        }
        if let Source::Flatpak { flatpak_id, remote } = &self.source
            && (!valid_flatpak_name(flatpak_id) || !valid_flatpak_name(remote))
        {
            return Err(format!("{}: invalid Flatpak id or remote", self.id));
        }
        if let Some(m) = &self.manifest {
            m.validate()?;
        }
        Ok(())
    }
}

/// The Flatpak installation an app lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Installation {
    /// The user's own installation, where tuxhub installs Flatpaks.
    #[default]
    User,
    /// The system-wide installation; changes need the password.
    System,
}

impl Installation {
    /// The flatpak option that selects this installation.
    pub fn flag(self) -> &'static str {
        match self {
            Self::User => "--user",
            Self::System => "--system",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub entry: Entry,
    /// Release tag (GitHub) or version from the manifest.
    #[serde(default)]
    pub version: Option<String>,
    /// Folder with the tool files, relative to `tools/<id>`.
    #[serde(default)]
    pub root: Option<String>,
    /// Detected executable when the manifest does not name one.
    #[serde(default)]
    pub detected_run: Option<String>,
    /// The app was installed before; tuxhub only added it to the sidebar.
    #[serde(default)]
    pub adopted: bool,
    /// The Flatpak installation tuxhub uses (Flatpaks only).
    #[serde(default)]
    pub installation: Installation,
    /// The user keeps both Flatpak installations: no hint about the copy.
    #[serde(default)]
    pub duplicate_dismissed: bool,
    /// Where an app in the hub runs when it is started (embedded apps only).
    #[serde(default)]
    pub window: WindowMode,
}

/// Where an embedded app runs: inside tuxhub (xpra) or in its own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    #[default]
    Hub,
    Own,
}

/// Installation progress; the user interface turns `stage` into text.
#[derive(Clone, Serialize)]
struct Progress<'a> {
    id: &'a str,
    stage: &'static str,
    detail: Option<&'a str>,
    bytes: Option<u64>,
    fraction: Option<f64>,
}

fn progress<R: Runtime>(app: &AppHandle<R>, p: Progress<'_>) {
    let _ = app.emit("tool-progress", p);
}

fn stage<'a>(id: &'a str, stage: &'static str, detail: Option<&'a str>) -> Progress<'a> {
    Progress {
        id,
        stage,
        detail,
        bytes: None,
        fraction: None,
    }
}

impl Installed {
    /// A record of an own installation, without version or files.
    pub fn new(entry: Entry) -> Self {
        Self {
            entry,
            version: None,
            root: None,
            detected_run: None,
            adopted: false,
            installation: Installation::User,
            duplicate_dismissed: false,
            window: WindowMode::Hub,
        }
    }

    pub fn id(&self) -> &str {
        &self.entry.id
    }

    /// Folder that contains the tool files (Flatpaks have none).
    pub fn root_dir(&self, tools_dir: &Path) -> Option<PathBuf> {
        match &self.entry.source {
            Source::Local { path } | Source::External { path, .. } => Some(PathBuf::from(path)),
            Source::GithubRelease { .. } => Some(
                tools_dir
                    .join(&self.entry.id)
                    .join(self.root.as_deref().unwrap_or("files")),
            ),
            Source::Flatpak { .. }
            | Source::SystemPackage { .. }
            | Source::DistroPackage { .. }
            | Source::Launcher { .. } => None,
        }
    }

    /// Manifest from the workshop entry, otherwise the tool's `tuxhub.toml`.
    pub fn manifest(&self, tools_dir: &Path) -> Manifest {
        if let Some(m) = &self.entry.manifest {
            return m.clone();
        }
        self.root_dir(tools_dir)
            .and_then(|dir| match Manifest::load(&dir) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("{}: {e}", self.entry.id);
                    None
                }
            })
            .unwrap_or_default()
    }
}

pub fn load_all(tools_dir: &Path) -> Vec<Installed> {
    let Ok(rd) = std::fs::read_dir(tools_dir) else {
        return vec![];
    };
    let mut out: Vec<Installed> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let s = std::fs::read_to_string(e.path().join("tool.json")).ok()?;
            match serde_json::from_str::<Installed>(&s) {
                Ok(t) if valid_id(&t.entry.id) => Some(t),
                Ok(_) => None,
                Err(err) => {
                    eprintln!("{}: invalid tool.json: {err}", e.path().display());
                    None
                }
            }
        })
        .collect();
    // Tools installed through tuxhub take precedence over announcements.
    for ext in discover() {
        if !out.iter().any(|t| t.entry.id == ext.entry.id) {
            out.push(ext);
        }
    }
    out.sort_by_key(|a| a.entry.name.to_lowercase());
    out
}

pub fn load(tools_dir: &Path, id: &str) -> Result<Installed, String> {
    if !valid_id(id) {
        return Err(format!("invalid tool id {id:?}"));
    }
    match std::fs::read_to_string(tools_dir.join(id).join("tool.json")) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
        Err(_) => discover()
            .into_iter()
            .find(|t| t.entry.id == id)
            .ok_or_else(|| format!("tool {id:?} is not installed")),
    }
}

/// Announcement of a tool installed outside tuxhub.
#[derive(Deserialize)]
struct Announcement {
    id: String,
    /// Absolute path of the folder that contains the tool's `tuxhub.toml`.
    path: String,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    asset: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

/// Folders with announcements, the user's first, with a flag for the user's.
fn announcement_dirs() -> Vec<(PathBuf, bool)> {
    let mut dirs = Vec::new();
    let user =
        env_path("XDG_DATA_HOME").or_else(|| env_path("HOME").map(|h| h.join(".local/share")));
    if let Some(d) = user {
        dirs.push((d.join("tuxhub/apps"), true));
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for d in system
        .split(':')
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        dirs.push((d.join("tuxhub/apps"), false));
    }
    dirs
}

/// Tools installed outside tuxhub. An announcement is only accepted if its
/// folder contains a `tuxhub.toml` with the same id.
pub fn discover() -> Vec<Installed> {
    let mut out: Vec<Installed> = Vec::new();
    for (dir, user) in announcement_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        for file in files {
            match announced(&file, user) {
                Ok(inst) if !out.iter().any(|t| t.entry.id == inst.entry.id) => out.push(inst),
                Ok(_) => {}
                Err(e) => eprintln!("{}: {e}", file.display()),
            }
        }
    }
    out
}

fn announced(file: &Path, user: bool) -> Result<Installed, String> {
    let s = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let a: Announcement = toml::from_str(&s).map_err(|e| e.to_string())?;
    if !valid_id(&a.id) || file.file_stem().and_then(|s| s.to_str()) != Some(a.id.as_str()) {
        return Err(format!("the id {:?} must match the file name", a.id));
    }
    let path = PathBuf::from(&a.path);
    if !path.is_absolute() {
        return Err("`path` must be absolute".into());
    }
    let m = Manifest::load(&path)?
        .ok_or_else(|| format!("{} contains no tuxhub.toml", path.display()))?;
    if m.id.as_deref() != Some(a.id.as_str()) {
        return Err(format!(
            "the tuxhub.toml in {} belongs to another tool",
            path.display()
        ));
    }
    let entry = Entry {
        name: m.name.clone().unwrap_or_else(|| a.id.clone()),
        description: m.description.clone().unwrap_or_default(),
        icon: m.icon.clone(),
        category: a.category,
        repo: a.repo,
        homepage: None,
        source: Source::External {
            path: a.path,
            asset: a.asset,
            user,
        },
        manifest: None,
        integration: None,
        translations: BTreeMap::new(),
        id: a.id,
    };
    entry.validate()?;
    Ok(Installed {
        version: m.version.clone(),
        ..Installed::new(entry)
    })
}

/// Saves the changed record of a tool in tuxhub.
pub fn save(tools_dir: &Path, inst: &Installed) -> Result<(), String> {
    write_record(&tools_dir.join(inst.id()), inst)
}

fn write_record(dir: &Path, inst: &Installed) -> Result<(), String> {
    std::fs::write(
        dir.join("tool.json"),
        serde_json::to_string_pretty(inst).unwrap(),
    )
    .map_err(|e| e.to_string())
}

async fn run(cmd: impl AsRef<Path>, args: &[&str]) -> Result<(), String> {
    let cmd = cmd.as_ref();
    let out = Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("could not run {}: {e}", cmd.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Programs that tell and change where an app is installed.
#[derive(Debug, Clone)]
pub struct Programs {
    pub flatpak: PathBuf,
    pub rpm: PathBuf,
    pub dpkg_query: PathBuf,
    /// Tells which Debian package owns a file (`dpkg -S`).
    pub dpkg: PathBuf,
    /// Lists and installs updates of RPM packages.
    pub dnf: PathBuf,
    /// Lists updates of Debian packages.
    pub apt: PathBuf,
    /// Installs updates of Debian packages.
    pub apt_get: PathBuf,
    /// Runs a program as root; it asks for the password.
    pub pkexec: PathBuf,
    /// Longest time a query may take before it counts as "not installed".
    pub timeout: Duration,
    /// Longest time a query of a remote may take.
    pub network_timeout: Duration,
}

impl Default for Programs {
    fn default() -> Self {
        Self {
            flatpak: "flatpak".into(),
            rpm: "rpm".into(),
            dpkg_query: "dpkg-query".into(),
            dpkg: "dpkg".into(),
            dnf: "dnf".into(),
            apt: "apt".into(),
            apt_get: "apt-get".into(),
            pkexec: "pkexec".into(),
            timeout: Duration::from_secs(5),
            network_timeout: Duration::from_secs(60),
        }
    }
}

impl Programs {
    /// Output of a query, or `None` when the program is missing, cannot run
    /// or takes longer than the timeout. A late program is killed.
    pub(crate) async fn query(
        &self,
        program: &Path,
        args: &[&str],
    ) -> Option<std::process::Output> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output();
        tokio::time::timeout(self.timeout, out).await.ok()?.ok()
    }

    /// Whether the installation has the app, or `None` when flatpak did not
    /// answer in time or could not run.
    async fn flatpak_answer(&self, flatpak_id: &str, installation: Installation) -> Option<bool> {
        self.query(&self.flatpak, &["info", installation.flag(), flatpak_id])
            .await
            .map(|o| o.status.success())
    }

    /// Whether the installation has the app and the remote it came from,
    /// with one `flatpak info --show-origin` (it fails exactly when the app
    /// is not installed). `None` when flatpak did not answer in time; the
    /// remote is `None` when flatpak names no valid one.
    pub(crate) async fn flatpak_origin(
        &self,
        flatpak_id: &str,
        installation: Installation,
    ) -> Option<(bool, Option<String>)> {
        let out = self
            .query(
                &self.flatpak,
                &["info", installation.flag(), "--show-origin", flatpak_id],
            )
            .await?;
        let origin = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let installed = out.status.success();
        Some((
            installed,
            (installed && valid_flatpak_name(&origin)).then_some(origin),
        ))
    }

    /// The version of the app in the installation, when flatpak tells it.
    async fn flatpak_version(
        &self,
        flatpak_id: &str,
        installation: Installation,
    ) -> Option<String> {
        let out = self
            .query(
                &self.flatpak,
                &[
                    "list",
                    "--app",
                    installation.flag(),
                    "--columns=application,version",
                ],
            )
            .await?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|line| {
                let (id, version) = line.split_once('\t')?;
                (id.trim() == flatpak_id && !version.trim().is_empty())
                    .then(|| version.trim().to_string())
            })
    }

    pub(crate) async fn flatpak_installed(
        &self,
        flatpak_id: &str,
        installation: Installation,
    ) -> bool {
        self.flatpak_answer(flatpak_id, installation).await == Some(true)
    }

    async fn rpm_installed(&self, package: &str) -> bool {
        self.query(&self.rpm, &["-q", "--quiet", package])
            .await
            .is_some_and(|o| o.status.success())
    }

    /// Version of an installed system package, without Debian's epoch and
    /// revision, so it compares with a release tag.
    async fn package_version(&self, package: &str, rpm: bool, deb: bool) -> Option<String> {
        let text = |o: std::process::Output| {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (o.status.success() && !v.is_empty()).then_some(v)
        };
        if rpm
            && let Some(v) = self
                .query(&self.rpm, &["-q", "--qf", "%{VERSION}", package])
                .await
                .and_then(text)
        {
            return Some(v);
        }
        if !deb {
            return None;
        }
        let v = self
            .query(&self.dpkg_query, &["-W", "-f=${Version}", package])
            .await
            .and_then(text)?;
        let v = v.split_once(':').map_or(v.as_str(), |(_, v)| v);
        Some(v.rsplit_once('-').map_or(v, |(v, _)| v).to_string())
    }

    async fn deb_installed(&self, package: &str) -> bool {
        // A removed package can leave its configuration: "deinstall ok config-files".
        self.query(&self.dpkg_query, &["-W", "-f=${Status}", package])
            .await
            .is_some_and(|o| o.status.success() && o.stdout.ends_with(b" installed"))
    }
}

/// Where the app of a workshop entry is installed on this computer.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Detected {
    /// Flatpak installations that contain the app.
    pub flatpak: Vec<Installation>,
    /// The system package is installed.
    pub package: bool,
}

impl Detected {
    pub fn any(&self) -> bool {
        !self.flatpak.is_empty() || self.package
    }
}

/// Looks for the app of a Flatpak or system package entry. Queries that
/// fail, hang or whose program is missing count as "not installed".
pub async fn detect(entry: &Entry, programs: &Programs) -> Detected {
    if entry.validate().is_err() {
        return Detected::default();
    }
    match &entry.source {
        Source::Flatpak { flatpak_id, .. } => {
            let (user, system) = tokio::join!(
                programs.flatpak_installed(flatpak_id, Installation::User),
                programs.flatpak_installed(flatpak_id, Installation::System),
            );
            Detected {
                flatpak: [(user, Installation::User), (system, Installation::System)]
                    .into_iter()
                    .filter_map(|(found, i)| found.then_some(i))
                    .collect(),
                package: false,
            }
        }
        Source::SystemPackage {
            package, rpm, deb, ..
        } => Detected {
            flatpak: vec![],
            package: (rpm.is_some() && programs.rpm_installed(package).await)
                || (deb.is_some() && programs.deb_installed(package).await),
        },
        Source::DistroPackage {
            package, manager, ..
        } => Detected {
            flatpak: vec![],
            package: match manager {
                PackageFormat::Rpm => programs.rpm_installed(package).await,
                PackageFormat::Deb => programs.deb_installed(package).await,
            },
        },
        _ => Detected::default(),
    }
}

/// Whether `path` is a file this user may run.
fn is_executable(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Whether the program of a launcher is still there.
fn launcher_present(command: &[String]) -> bool {
    command.first().is_some_and(|p| is_executable(Path::new(p)))
}

/// Adds an app that is already on this computer to tuxhub, without
/// installing anything. Of two Flatpak installations the system one is used.
pub async fn adopt(
    tools_dir: &Path,
    programs: &Programs,
    entry: &Entry,
) -> Result<Installed, String> {
    entry.validate()?;
    let found = detect(entry, programs).await;
    let missing = || format!("{} is not installed on this computer", entry.name);
    let inst = match &entry.source {
        Source::Flatpak { .. } => {
            let installation = [Installation::System, Installation::User]
                .into_iter()
                .find(|i| found.flatpak.contains(i))
                .ok_or_else(missing)?;
            Installed {
                adopted: true,
                installation,
                ..Installed::new(entry.clone())
            }
        }
        Source::SystemPackage {
            package, rpm, deb, ..
        } => {
            if !found.package {
                return Err(missing());
            }
            Installed {
                adopted: true,
                version: programs
                    .package_version(package, rpm.is_some(), deb.is_some())
                    .await,
                ..Installed::new(entry.clone())
            }
        }
        Source::DistroPackage {
            package, manager, ..
        } => {
            if !found.package {
                return Err(missing());
            }
            let rpm = *manager == PackageFormat::Rpm;
            Installed {
                adopted: true,
                version: programs.package_version(package, rpm, !rpm).await,
                ..Installed::new(entry.clone())
            }
        }
        Source::Launcher { command, .. } => {
            if !launcher_present(command) {
                return Err(missing());
            }
            Installed {
                adopted: true,
                ..Installed::new(entry.clone())
            }
        }
        _ => return Err(format!("{} cannot be added from this computer", entry.name)),
    };
    let dir = tools_dir.join(&entry.id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_record(&dir, &inst)?;
    Ok(inst)
}

/// What the sidebar tells about the installation of a tool.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InstallState {
    /// The app is no longer installed on this computer.
    pub missing: bool,
    /// The Flatpak is installed for the user and system-wide.
    pub duplicate: bool,
}

/// Checks that the app of a Flatpak or system package tool is still
/// installed. A Flatpak whose recorded installation is gone while the other
/// one has it is switched over, so tuxhub never installs it a second time.
pub async fn check_installation(
    tools_dir: &Path,
    programs: &Programs,
    inst: &mut Installed,
) -> InstallState {
    match &inst.entry.source {
        Source::Flatpak { flatpak_id, .. } => {
            let found = detect(&inst.entry, programs).await.flatpak;
            let Some(&other) = found.first() else {
                return InstallState {
                    missing: true,
                    duplicate: false,
                };
            };
            // Only a clear "not installed" moves the record, not a query
            // that hung or failed.
            if !found.contains(&inst.installation)
                && programs.flatpak_answer(flatpak_id, inst.installation).await == Some(false)
            {
                inst.installation = other;
                inst.adopted = true;
                if let Err(e) = save(tools_dir, inst) {
                    eprintln!("{}: {e}", inst.id());
                }
            }
            InstallState {
                missing: false,
                duplicate: found.len() > 1 && !inst.duplicate_dismissed,
            }
        }
        Source::SystemPackage { .. } | Source::DistroPackage { .. } => InstallState {
            missing: !detect(&inst.entry, programs).await.package,
            duplicate: false,
        },
        Source::Launcher { command, .. } => InstallState {
            missing: !launcher_present(command),
            duplicate: false,
        },
        _ => InstallState::default(),
    }
}

pub async fn install<R: Runtime>(
    app: &AppHandle<R>,
    tools_dir: &Path,
    programs: &Programs,
    entry: &Entry,
) -> Result<Installed, String> {
    entry.validate()?;
    match &entry.source {
        Source::GithubRelease { asset } => {
            install_github(app, tools_dir, entry, entry.repo.as_deref().unwrap(), asset).await
        }
        Source::Flatpak { flatpak_id, remote } => {
            // Per-user installation: no root required, and it avoids the
            // filtered Flathub remote that some distributions ship.
            if remote == "flathub" {
                progress(app, stage(&entry.id, "flathub", None));
                run(
                    &programs.flatpak,
                    &[
                        "remote-add",
                        "--user",
                        "--if-not-exists",
                        "flathub",
                        "https://dl.flathub.org/repo/flathub.flatpakrepo",
                    ],
                )
                .await?;
            }
            progress(app, stage(&entry.id, "flatpak", Some(flatpak_id)));
            run(
                &programs.flatpak,
                &[
                    "install",
                    "--user",
                    "-y",
                    "--noninteractive",
                    remote,
                    flatpak_id,
                ],
            )
            .await?;
            let dir = tools_dir.join(&entry.id);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let inst = Installed::new(entry.clone());
            write_record(&dir, &inst)?;
            Ok(inst)
        }
        Source::SystemPackage { .. } => install_system_package(app, tools_dir, entry).await,
        Source::Local { .. } => Err("local tools are linked, not installed".into()),
        Source::External { .. } => Err("this tool is installed outside tuxhub".into()),
        Source::DistroPackage { .. } | Source::Launcher { .. } => Err(format!(
            "{} is added from this computer, not installed",
            entry.name
        )),
    }
}

/// Downloads the latest release asset matching `pattern` into the emptied
/// folder `staging`. Returns the release tag and the downloaded file.
async fn download_release<R: Runtime>(
    app: &AppHandle<R>,
    staging: &Path,
    id: &str,
    repo: &str,
    pattern: &str,
) -> Result<(String, PathBuf), String> {
    progress(app, stage(id, "resolving", None));
    let rel = github::latest_release(repo).await?;
    let asset = rel
        .assets
        .iter()
        .find(|a| {
            github::glob_match(pattern, &a.name) && safe_relative(&a.name) && !a.name.contains('/')
        })
        .ok_or_else(|| format!("no asset of release {} matches {pattern:?}", rel.tag_name))?
        .clone();

    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(|e| e.to_string())?;

    let download = staging.join(&asset.name);
    let size = asset.size;
    github::download(repo, &asset, &download, |done| {
        progress(
            app,
            Progress {
                id,
                stage: "downloading",
                detail: Some(&asset.name),
                bytes: Some(done),
                fraction: (size > 0).then(|| done as f64 / size as f64),
            },
        );
    })
    .await?;
    Ok((rel.tag_name, download))
}

/// Downloads the latest release asset matching `pattern` and extracts it to
/// `staging/files`. Returns the release tag and the tool root relative to
/// `staging`.
async fn fetch_release<R: Runtime>(
    app: &AppHandle<R>,
    staging: &Path,
    id: &str,
    repo: &str,
    pattern: &str,
) -> Result<(String, String), String> {
    let (tag, download) = download_release(app, staging, id, repo, pattern).await?;
    let name = download
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let files = staging.join("files");
    std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;

    progress(app, stage(id, "extracting", None));
    let lower = name.to_lowercase();
    let is_tar = [".tar.gz", ".tgz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tar"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    if is_tar {
        run(
            "tar",
            &[
                "--no-same-owner",
                "-xf",
                &download.to_string_lossy(),
                "-C",
                &files.to_string_lossy(),
            ],
        )
        .await?;
        std::fs::remove_file(&download).map_err(|e| e.to_string())?;
    } else if lower.ends_with(".zip") {
        run(
            "unzip",
            &[
                "-q",
                &download.to_string_lossy(),
                "-d",
                &files.to_string_lossy(),
            ],
        )
        .await?;
        std::fs::remove_file(&download).map_err(|e| e.to_string())?;
    } else {
        // A single file (AppImage or binary).
        let target = files.join(&name);
        std::fs::rename(&download, &target).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }

    // Archives with exactly one top-level folder: use it as the root.
    let mut root = "files".to_string();
    let top: Vec<_> = std::fs::read_dir(&files)
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    if top.len() == 1 && top[0].path().is_dir() {
        root = format!("files/{}", top[0].file_name().to_string_lossy());
    }
    Ok((tag, root))
}

/// Package managers tuxhub installs distribution packages with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Dnf,
    Zypper,
    Apt,
}

impl PackageManager {
    /// The first available package manager for one of the offered formats.
    fn pick(rpm: bool, deb: bool, available: impl Fn(&str) -> bool) -> Option<Self> {
        [(Self::Dnf, rpm), (Self::Zypper, rpm), (Self::Apt, deb)]
            .into_iter()
            .find(|(pm, offered)| *offered && available(pm.program()))
            .map(|(pm, _)| pm)
    }

    /// The package manager of this system for an entry's packages.
    fn detect(rpm: bool, deb: bool) -> Option<Self> {
        Self::pick(rpm, deb, |cmd| which(cmd).is_some())
    }

    fn program(self) -> &'static str {
        match self {
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
            Self::Apt => "apt-get",
        }
    }

    fn rpm(self) -> bool {
        !matches!(self, Self::Apt)
    }

    fn install_args(self, file: &Path) -> Vec<String> {
        let file = file.to_string_lossy().into_owned();
        match self {
            Self::Dnf | Self::Apt => vec!["install".into(), "-y".into(), file],
            // Release packages are usually not signed.
            Self::Zypper => vec![
                "--non-interactive".into(),
                "install".into(),
                "--allow-unsigned-rpm".into(),
                file,
            ],
        }
    }

    fn remove_args(self, package: &str) -> Vec<String> {
        match self {
            Self::Dnf | Self::Apt => vec!["remove".into(), "-y".into(), package.into()],
            Self::Zypper => vec!["--non-interactive".into(), "remove".into(), package.into()],
        }
    }

    async fn installed(self, package: &str) -> bool {
        if self.rpm() {
            Command::new("rpm")
                .args(["-q", "--quiet", package])
                .status()
                .await
                .is_ok_and(|s| s.success())
        } else {
            // A removed package can leave its configuration: "deinstall ok config-files".
            Command::new("dpkg-query")
                .args(["-W", "-f=${Status}", package])
                .output()
                .await
                .is_ok_and(|o| o.status.success() && o.stdout.ends_with(b" installed"))
        }
    }

    /// Name of the package in a package file.
    async fn package_name(self, file: &Path) -> Result<String, String> {
        let mut cmd = if self.rpm() {
            let mut c = Command::new("rpm");
            c.args(["-qp", "--qf", "%{NAME}"]).arg(file);
            c
        } else {
            let mut c = Command::new("dpkg-deb");
            c.arg("-f").arg(file).arg("Package");
            c
        };
        let out = cmd
            .output()
            .await
            .map_err(|e| format!("could not read the package: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Runs the package manager as root. pkexec asks for the password.
    async fn run_privileged(self, args: Vec<String>) -> Result<(), String> {
        let pkexec = which("pkexec").ok_or("pkexec is not installed")?;
        let program =
            which(self.program()).ok_or_else(|| format!("{} is missing", self.program()))?;
        let out = Command::new(pkexec)
            .arg(program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| format!("could not run pkexec: {e}"))?;
        match out.status.code() {
            Some(0) => Ok(()),
            Some(126) => Err("the password prompt was cancelled".into()),
            Some(127) => Err("not authorized to install packages".into()),
            _ => {
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                Err(if err.is_empty() {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    err
                })
            }
        }
    }
}

/// Downloads the package for this distribution from the latest release and
/// installs it with the package manager.
async fn install_system_package<R: Runtime>(
    app: &AppHandle<R>,
    tools_dir: &Path,
    entry: &Entry,
) -> Result<Installed, String> {
    let (
        Source::SystemPackage {
            package,
            rpm,
            deb,
            command,
        },
        Some(repo),
    ) = (&entry.source, &entry.repo)
    else {
        return Err(format!("{} is not a system package", entry.id));
    };
    let id = entry.id.as_str();
    let pm = PackageManager::detect(rpm.is_some(), deb.is_some()).ok_or_else(|| {
        format!(
            "{} offers no package for this distribution (dnf, zypper or apt)",
            entry.name
        )
    })?;
    let pattern = if pm.rpm() { rpm } else { deb };
    let staging = tools_dir.join(format!(".{id}.new"));
    let result = async {
        let (tag, file) =
            download_release(app, &staging, id, repo, pattern.as_deref().unwrap()).await?;
        // Removal later uses the name from the workshop entry, so it must be
        // the package installed now.
        let name = pm.package_name(&file).await?;
        if name != *package {
            return Err(format!(
                "the release contains the package {name:?} instead of {package:?}"
            ));
        }
        progress(app, stage(id, "package", None));
        pm.run_privileged(pm.install_args(&file)).await?;
        if !Path::new(command).is_file() {
            return Err(format!("the package does not provide {command}"));
        }
        let dir = tools_dir.join(id);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let inst = Installed {
            version: Some(tag),
            ..Installed::new(entry.clone())
        };
        write_record(&dir, &inst)?;
        Ok(inst)
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Removes a system package unless it is already gone.
async fn remove_system_package(package: &str, rpm: bool, deb: bool) -> Result<(), String> {
    let Some(pm) = PackageManager::detect(rpm, deb) else {
        return Ok(());
    };
    if !pm.installed(package).await {
        return Ok(());
    }
    pm.run_privileged(pm.remove_args(package)).await
}

async fn install_github<R: Runtime>(
    app: &AppHandle<R>,
    tools_dir: &Path,
    entry: &Entry,
    repo: &str,
    pattern: &str,
) -> Result<Installed, String> {
    let id = entry.id.as_str();
    // Install into a staging folder and swap at the end, so an interrupted
    // update never breaks the installed version.
    let staging = tools_dir.join(format!(".{id}.new"));
    let (tag, root) = fetch_release(app, &staging, id, repo, pattern).await?;

    let mut inst = Installed {
        version: Some(tag),
        root: Some(root),
        ..Installed::new(entry.clone())
    };
    let root_dir = staging.join(inst.root.as_deref().unwrap());
    // Only guess an executable when there is no manifest at all. A manifest
    // without `run` means the tool is operated exclusively through tuxhub.
    let has_manifest = entry.manifest.is_some() || Manifest::load(&root_dir)?.is_some();
    if !has_manifest {
        inst.detected_run = detect_executable(&root_dir);
    }
    write_record(&staging, &inst)?;

    let dir = tools_dir.join(id);
    if dir.exists() {
        let old = tools_dir.join(format!(".{id}.old"));
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&dir, &old).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&old);
    }
    std::fs::rename(&staging, &dir).map_err(|e| e.to_string())?;
    Ok(inst)
}

/// Updates a tool installed outside tuxhub in its own folder. The new
/// release must contain a `tuxhub.toml` with the same id, so an announcement
/// can never make tuxhub replace an unrelated folder.
pub async fn update_external<R: Runtime>(
    app: &AppHandle<R>,
    inst: &Installed,
) -> Result<(), String> {
    let id = inst.id();
    let (
        Source::External {
            path,
            asset: Some(pattern),
            user: true,
        },
        Some(repo),
    ) = (&inst.entry.source, &inst.entry.repo)
    else {
        return Err(format!(
            "{id} is installed outside tuxhub and cannot be updated here"
        ));
    };
    let dest = PathBuf::from(path);
    let current = Manifest::load(&dest)?;
    if current.and_then(|m| m.id).as_deref() != Some(id) {
        return Err(format!("{} does not contain {id}", dest.display()));
    }
    let parent = dest.parent().ok_or("invalid installation folder")?;
    let name = dest
        .file_name()
        .ok_or("invalid installation folder")?
        .to_string_lossy();
    // Stage next to the installation, so the final rename stays on one file system.
    let staging = parent.join(format!(".{name}.tuxhub-update"));
    let result = async {
        let (_, root) = fetch_release(app, &staging, id, repo, pattern).await?;
        let new_root = staging.join(root);
        let m = Manifest::load(&new_root)?.ok_or("the release contains no tuxhub.toml")?;
        if m.id.as_deref() != Some(id) {
            return Err(format!("the release does not contain {id}"));
        }
        let old = parent.join(format!(".{name}.tuxhub-old"));
        let _ = std::fs::remove_dir_all(&old);
        std::fs::rename(&dest, &old).map_err(|e| e.to_string())?;
        if let Err(e) = std::fs::rename(&new_root, &dest) {
            let _ = std::fs::rename(&old, &dest);
            return Err(e.to_string());
        }
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Looks for something to launch: an AppImage first, otherwise the only
/// executable file.
fn detect_executable(dir: &Path) -> Option<String> {
    let files: Vec<(String, bool)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| {
            let exec = e
                .metadata()
                .is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
            (e.file_name().to_string_lossy().into_owned(), exec)
        })
        .collect();
    if let Some((name, _)) = files
        .iter()
        .find(|(n, _)| n.to_lowercase().ends_with(".appimage"))
    {
        return Some(name.clone());
    }
    let exec: Vec<_> = files.iter().filter(|(_, x)| *x).collect();
    (exec.len() == 1).then(|| exec[0].0.clone())
}

/// Links a local folder with a `tuxhub.toml` as a tool (developer mode).
pub fn link_local(tools_dir: &Path, path: &Path) -> Result<Installed, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let m = Manifest::load(&path)?.ok_or("the folder does not contain a tuxhub.toml")?;
    let id =
        m.id.clone()
            .ok_or("tuxhub.toml needs an `id` for local tools")?;
    let entry = Entry {
        name: m.name.clone().unwrap_or_else(|| id.clone()),
        description: m.description.clone().unwrap_or_default(),
        icon: m.icon.clone(),
        category: Some("local".into()),
        repo: None,
        homepage: None,
        source: Source::Local {
            path: path.to_string_lossy().into_owned(),
        },
        manifest: None,
        integration: None,
        translations: BTreeMap::new(),
        id,
    };
    entry.validate()?;
    let dir = tools_dir.join(&entry.id);
    if dir.join("tool.json").exists() {
        return Err(format!(
            "a tool with the id {:?} is already installed",
            entry.id
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let inst = Installed {
        version: m.version.clone(),
        ..Installed::new(entry)
    };
    write_record(&dir, &inst)?;
    Ok(inst)
}

/// Removes the Flatpak from one installation; the other one is left alone.
fn flatpak_uninstall_args(flatpak_id: &str, installation: Installation) -> [&str; 5] {
    [
        "uninstall",
        installation.flag(),
        "-y",
        "--noninteractive",
        flatpak_id,
    ]
}

/// Removes a Flatpak from one installation; one that is already gone counts
/// as removed.
pub async fn flatpak_uninstall(
    programs: &Programs,
    flatpak_id: &str,
    installation: Installation,
) -> Result<(), String> {
    match run(
        &programs.flatpak,
        &flatpak_uninstall_args(flatpak_id, installation),
    )
    .await
    {
        Err(e) if !flatpak_already_gone(&e) => Err(e),
        _ => Ok(()),
    }
}

/// Whether flatpak failed only because the app is no longer installed.
fn flatpak_already_gone(stderr: &str) -> bool {
    stderr.contains("No installed refs found")
}

/// Whether tuxhub may uninstall the app. An app added from this computer
/// belongs to its package manager or installer: tuxhub only forgets it.
pub fn check_removable(inst: &Installed) -> Result<(), String> {
    match inst.entry.source {
        Source::DistroPackage { .. } | Source::Launcher { .. } => Err(format!(
            "{} was added from this computer; remove it from tuxhub instead",
            inst.entry.name
        )),
        _ => Ok(()),
    }
}

/// Removes a tool. The tool's own `uninstall` command runs first, so it can
/// remove what it set up itself. Tools installed outside tuxhub are removed
/// by that command alone.
pub async fn uninstall(
    tools_dir: &Path,
    programs: &Programs,
    inst: &Installed,
) -> Result<(), String> {
    check_removable(inst)?;
    let hook = inst.manifest(tools_dir).uninstall;
    if let Source::External { user, .. } = &inst.entry.source
        && (!user || hook.is_none())
    {
        return Err(format!(
            "{} is installed outside tuxhub; remove it with its own uninstaller or the package manager",
            inst.entry.name
        ));
    }
    if let (Some(cmd), Some(root)) = (&hook, inst.root_dir(tools_dir)) {
        let out = Command::new(root.join(&cmd.command))
            .args(&cmd.args)
            .current_dir(&root)
            .stdin(std::process::Stdio::null())
            .output();
        match tokio::time::timeout(std::time::Duration::from_secs(30), out).await {
            Ok(Ok(o)) if o.status.success() => {}
            Ok(Ok(o)) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                if matches!(inst.entry.source, Source::External { .. }) {
                    return Err(err);
                }
                eprintln!("{}: uninstall command failed: {err}", inst.id());
            }
            Ok(Err(e)) => eprintln!("{}: could not run the uninstall command: {e}", inst.id()),
            Err(_) => eprintln!("{}: the uninstall command timed out", inst.id()),
        }
    }
    if matches!(inst.entry.source, Source::External { .. }) {
        return Ok(());
    }
    if let Source::Flatpak { flatpak_id, .. } = &inst.entry.source {
        flatpak_uninstall(programs, flatpak_id, inst.installation).await?;
    }
    if let Source::SystemPackage {
        package, rpm, deb, ..
    } = &inst.entry.source
    {
        remove_system_package(package, rpm.is_some(), deb.is_some()).await?;
    }
    // For local tools only the link is removed, never the folder itself.
    std::fs::remove_dir_all(tools_dir.join(inst.id())).map_err(|e| e.to_string())
}

/// How a tool is launched: program, arguments and working directory.
pub struct LaunchCmd {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub flatpak: bool,
}

pub fn launch_cmd(tools_dir: &Path, inst: &Installed) -> Result<LaunchCmd, String> {
    match &inst.entry.source {
        Source::Flatpak { flatpak_id, .. } => Ok(LaunchCmd {
            program: "flatpak".into(),
            args: vec![
                "run".into(),
                inst.installation.flag().into(),
                flatpak_id.clone(),
            ],
            cwd: None,
            flatpak: true,
        }),
        Source::SystemPackage { command, .. } => {
            let exe = PathBuf::from(command);
            if !exe.is_file() {
                return Err(format!("{command} is missing; install the tool again"));
            }
            Ok(LaunchCmd {
                program: exe,
                args: vec![],
                cwd: None,
                flatpak: false,
            })
        }
        // The command stored when the app was added, never one from elsewhere.
        Source::DistroPackage { command, .. } | Source::Launcher { command, .. } => {
            // tool.json may have been edited by hand since the app was added.
            if !valid_command(command) {
                return Err(format!(
                    "{}: the start command must start with an absolute path",
                    inst.entry.name
                ));
            }
            let (program, args) = command
                .split_first()
                .ok_or("this app has no start command")?;
            Ok(LaunchCmd {
                program: program.into(),
                args: args.to_vec(),
                cwd: None,
                flatpak: false,
            })
        }
        _ => {
            let root = inst.root_dir(tools_dir).ok_or("the tool has no folder")?;
            let manifest = inst.manifest(tools_dir);
            let (command, args) = match (&manifest.run, &inst.detected_run) {
                (Some(r), _) => (r.command.clone(), r.args.clone()),
                (None, Some(d)) => (d.clone(), vec![]),
                (None, None) => return Err("this tool has no launch command".into()),
            };
            let exe = root.join(&command);
            if !exe.is_file() {
                return Err(format!("{} is missing", exe.display()));
            }
            Ok(LaunchCmd {
                program: exe,
                args,
                cwd: Some(root),
                flatpak: false,
            })
        }
    }
}

/// Launches the tool as a regular application in its own window.
pub fn launch(tools_dir: &Path, inst: &Installed) -> Result<(), String> {
    let lc = launch_cmd(tools_dir, inst)?;
    let mut cmd = std::process::Command::new(&lc.program);
    cmd.args(&lc.args);
    if let Some(cwd) = &lc.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("could not start the tool: {e}"))?;
    // Reap the process so it does not linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The program of an app that runs as a regular application: one that
/// tuxhub only starts and stops (`embed = false`) or a level-0 app, which
/// may run in its own window. Flatpaks are found with `flatpak ps` instead.
pub fn app_program(tools_dir: &Path, inst: &Installed) -> Option<PathBuf> {
    let manifest = inst.manifest(tools_dir);
    if manifest.embed != Some(false) && manifest.level() != 0 {
        return None;
    }
    // A shared launcher runs many unrelated processes: never watch or quit them.
    if shared_launcher(inst) {
        return None;
    }
    launch_cmd(tools_dir, inst)
        .ok()
        .filter(|lc| !lc.flatpak)
        .map(|lc| lc.program)
}

/// Programs that start other programs: shells, interpreters and launchers.
/// Many unrelated processes run them, so an app started through one is
/// never found or quit by its program. A trailing `*` matches any ending
/// (`python*` matches `python3.13`).
const SHARED_LAUNCHERS: &[&str] = &[
    "sh",
    "bash",
    "dash",
    "zsh",
    "fish",
    "ksh",
    "csh",
    "tcsh",
    "env",
    "python*",
    "perl*",
    "ruby*",
    "node",
    "java",
    "wine*",
    "steam",
    "flatpak",
    "xdg-open",
    "gtk-launch",
    "gio",
    "kioclient*",
    "mono",
    "dotnet",
    "electron*",
    "busybox",
    // Every snap app's command links to it.
    "snap",
];

/// Whether `program` is a shell, interpreter or launcher of
/// [`SHARED_LAUNCHERS`], by its own name or the name of the file it links to
/// (`/bin/sh` is often `bash`).
pub fn is_shared_launcher(program: &Path) -> bool {
    let shared = |p: &Path| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|name| {
            SHARED_LAUNCHERS.iter().any(|l| match l.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => name == *l,
            })
        })
    };
    shared(program) || program.canonicalize().is_ok_and(|real| shared(&real))
}

/// Arguments that make a browser start one of its web apps.
const WEB_APP_ARGS: [&str; 2] = ["--app-id=", "--app="];

/// Whether `command` starts a browser's web app (`--app-id=…`, `--app=…`):
/// it runs as the browser, like a game started through its store client.
pub fn is_web_app(command: &[String]) -> bool {
    command
        .iter()
        .skip(1)
        .any(|a| WEB_APP_ARGS.iter().any(|w| a.starts_with(w)))
}

/// Whether the app was added from this computer and starts through a shared
/// launcher (or is a browser's web app), so tuxhub cannot tell its
/// processes from others.
pub fn shared_launcher(inst: &Installed) -> bool {
    match &inst.entry.source {
        Source::DistroPackage { command, .. } | Source::Launcher { command, .. } => {
            command
                .first()
                .is_some_and(|p| is_shared_launcher(Path::new(p)))
                || is_web_app(command)
        }
        _ => false,
    }
}

/// Whether the app was added from this computer and its program is a `#!`
/// script (like `/usr/bin/libreoffice`). The script runs as its interpreter,
/// so [`processes`] never finds the app by its program.
pub fn script_program(inst: &Installed) -> bool {
    use std::io::Read;
    let (Source::DistroPackage { command, .. } | Source::Launcher { command, .. }) =
        &inst.entry.source
    else {
        return false;
    };
    let Some(program) = command.first() else {
        return false;
    };
    // Opening a FIFO or device would block or read from it.
    if !std::fs::metadata(program).is_ok_and(|m| m.is_file()) {
        return false;
    }
    let mut head = [0u8; 2];
    std::fs::File::open(program)
        .and_then(|f| f.take(2).read_exact(&mut head))
        .is_ok_and(|()| &head == b"#!")
}

/// Whether the app of [`app_program`] is running, also when it was started
/// outside tuxhub (or inside tuxhub's xpra session).
pub fn app_running(tools_dir: &Path, inst: &Installed) -> bool {
    app_program(tools_dir, inst).is_some_and(|p| !processes(&p).is_empty())
}

/// Processes of the current user that run `program`. The processes of other
/// users cannot be read and are left out. The app of an AppImage runs from a
/// temporary mount; it is found by `APPIMAGE` in its environment.
pub fn processes(program: &Path) -> Vec<u32> {
    let program = program
        .canonicalize()
        .unwrap_or_else(|_| program.to_path_buf());
    let appimage = is_appimage(&program).then(|| {
        let mut var = b"APPIMAGE=".to_vec();
        var.extend_from_slice(program.as_os_str().as_encoded_bytes());
        var
    });
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return vec![];
    };
    rd.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| runs(&exe, &program))
                || appimage.as_deref().is_some_and(|var| started_by(*pid, var))
        })
        .collect()
}

fn is_appimage(program: &Path) -> bool {
    program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("appimage"))
}

/// Whether the environment of `pid` holds exactly `var` (`APPIMAGE=<path>`).
fn started_by(pid: u32, var: &[u8]) -> bool {
    std::fs::read(format!("/proc/{pid}/environ"))
        .is_ok_and(|env| env.split(|b| *b == 0).any(|entry| entry == var))
}

/// Whether `exe`, the target of `/proc/<pid>/exe`, is `program`. When an
/// update replaced the file, the kernel appends " (deleted)".
fn runs(exe: &Path, program: &Path) -> bool {
    exe == program
        || exe
            .to_str()
            .and_then(|s| s.strip_suffix(" (deleted)"))
            .is_some_and(|s| Path::new(s) == program)
}

/// Whether the output of `flatpak ps --columns=application` lists the app.
pub fn flatpak_ps_has(out: &str, flatpak_id: &str) -> bool {
    out.lines().any(|line| line.trim() == flatpak_id)
}

/// Whether the Flatpak runs, or `None` when flatpak did not answer.
pub async fn flatpak_running(programs: &Programs, flatpak_id: &str) -> Option<bool> {
    let out = programs
        .query(&programs.flatpak, &["ps", "--columns=application"])
        .await?;
    out.status
        .success()
        .then(|| flatpak_ps_has(&String::from_utf8_lossy(&out.stdout), flatpak_id))
}

/// Ends a running Flatpak with `flatpak kill`. Fails when it still runs
/// after `wait`; an app flatpak cannot tell about counts as ended.
pub async fn flatpak_kill(
    programs: &Programs,
    flatpak_id: &str,
    wait: Duration,
) -> Result<(), String> {
    if flatpak_running(programs, flatpak_id).await == Some(false) {
        return Ok(());
    }
    // The app may exit in between, so the exit status does not matter.
    let _ = programs
        .query(&programs.flatpak, &["kill", flatpak_id])
        .await;
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if flatpak_running(programs, flatpak_id).await != Some(true) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("{flatpak_id} is still running"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Ends the processes that run `program`: SIGTERM first, SIGKILL for those
/// still running after three seconds.
pub async fn quit(program: &Path) -> Result<(), String> {
    let mut left = processes(program);
    if left.is_empty() {
        return Ok(());
    }
    signal("TERM", &left).await?;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let running = processes(program);
        left.retain(|pid| running.contains(pid));
        if left.is_empty() {
            return Ok(());
        }
    }
    signal("KILL", &left).await
}

async fn signal(name: &str, pids: &[u32]) -> Result<(), String> {
    // A process may exit in between, so the exit status does not matter.
    Command::new("kill")
        .arg(format!("-{name}"))
        .args(pids.iter().map(u32::to_string))
        .stderr(Stdio::null())
        .status()
        .await
        .map(|_| ())
        .map_err(|e| format!("could not run kill: {e}"))
}

/// Returns the latest release tag if it differs from the installed one; for
/// a Flatpak the version its remote offers in the installation tuxhub uses.
pub async fn check_update(inst: &Installed, programs: &Programs) -> Result<Option<String>, String> {
    if let Source::Flatpak { flatpak_id, .. } = &inst.entry.source {
        return flatpak_update(programs, flatpak_id, inst.installation).await;
    }
    // An app adopted from this computer is updated by its package manager
    // only, never from GitHub. A launcher has no package manager.
    if let Source::DistroPackage {
        package, manager, ..
    } = &inst.entry.source
    {
        inst.entry.validate()?;
        return distro_update(programs, package, *manager).await;
    }
    let updatable = match &inst.entry.source {
        Source::GithubRelease { .. } | Source::SystemPackage { .. } => true,
        Source::External { asset, user, .. } => *user && asset.is_some(),
        _ => false,
    };
    let (true, Some(repo)) = (updatable, &inst.entry.repo) else {
        return Ok(None);
    };
    let rel = github::latest_release(repo).await?;
    // External tools report their manifest version, usually without the "v".
    let norm = |v: &str| v.trim_start_matches('v').to_string();
    let newer = inst.version.as_deref().map(norm) != Some(norm(&rel.tag_name));
    Ok(newer.then_some(rel.tag_name))
}

/// Version of a pending package update. Anything that is not a plain version
/// (spaces, control characters, an absurd length) is rejected.
fn clean_version(raw: &str) -> Result<String, String> {
    // Epoch ("2:") and release or revision ("-1.fc44") are not part of it;
    // the upstream version may hold `-` itself ("2.3-rc1-1").
    let v = raw.split_once(':').map_or(raw, |(_, v)| v);
    let v = v.rsplit_once('-').map_or(v, |(v, _)| v);
    let sane = !v.is_empty()
        && v.len() <= 64
        && v.starts_with(|c: char| c.is_ascii_alphanumeric())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+~^-".contains(c));
    if sane {
        Ok(v.to_string())
    } else {
        Err(format!(
            "unexpected version {raw:?} from the package manager"
        ))
    }
}

/// The version a pending update of a distribution package brings, or `None`
/// when it is up to date. Ends after `network_timeout`.
pub async fn distro_update(
    programs: &Programs,
    package: &str,
    manager: PackageFormat,
) -> Result<Option<String>, String> {
    if !valid_package_name(package) {
        return Err(format!("invalid package name {package:?}"));
    }
    let (program, args): (&Path, Vec<&str>) = match manager {
        PackageFormat::Rpm => (&programs.dnf, vec!["-q", "check-update", package]),
        PackageFormat::Deb => (&programs.apt, vec!["list", "--upgradable", package]),
    };
    let out = Command::new(program)
        .args(&args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(programs.network_timeout, out)
        .await
        .map_err(|_| format!("{} did not answer in time", program.display()))?
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match manager {
        PackageFormat::Rpm => match out.status.code() {
            Some(0) => Ok(None),
            Some(100) => {
                // dnf may put the columns of a long line on the next line.
                let prefix = format!("{package}.");
                let mut lines = stdout.lines();
                if let Some(line) = lines.find(|l| l.starts_with(&prefix)) {
                    let mut cols = line.split_whitespace();
                    cols.next();
                    let version = cols
                        .next()
                        .or_else(|| lines.next().and_then(|l| l.split_whitespace().next()));
                    return version
                        .ok_or_else(|| "dnf reported an update without a version".to_string())
                        .and_then(clean_version)
                        .map(Some);
                }
                Err("dnf reported an update that is not for this package".into())
            }
            _ => Err(failure_text(&out)),
        },
        PackageFormat::Deb => {
            if !out.status.success() {
                return Err(failure_text(&out));
            }
            let prefix = format!("{package}/");
            match stdout.lines().find(|l| l.starts_with(&prefix)) {
                None => Ok(None),
                Some(line) => line
                    .split_whitespace()
                    .nth(1)
                    .ok_or_else(|| "apt reported an update without a version".to_string())
                    .and_then(clean_version)
                    .map(Some),
            }
        }
    }
}

fn failure_text(out: &std::process::Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.is_empty() {
        format!("the package manager failed ({})", out.status)
    } else {
        err
    }
}

/// Installs the pending update of an app adopted from a package manager.
/// The password prompt of Polkit appears; when it is cancelled or the
/// upgrade fails, the record keeps its version.
pub async fn update_distro_package(
    tools_dir: &Path,
    programs: &Programs,
    inst: &Installed,
) -> Result<Installed, String> {
    let Source::DistroPackage {
        package, manager, ..
    } = &inst.entry.source
    else {
        return Err(format!(
            "{} is not updated by a package manager",
            inst.entry.name
        ));
    };
    inst.entry.validate()?;
    let rpm = *manager == PackageFormat::Rpm;
    let (program, args): (&Path, Vec<&str>) = if rpm {
        (&programs.dnf, vec!["-y", "upgrade", package])
    } else {
        (
            &programs.apt_get,
            vec!["install", "-y", "--only-upgrade", package],
        )
    };
    // pkexec runs only a program it finds by its full path.
    let program = if program.is_absolute() {
        program.to_path_buf()
    } else {
        which(&program.to_string_lossy())
            .ok_or_else(|| format!("{} is missing", program.display()))?
    };
    let out = Command::new(&programs.pkexec)
        .arg(&program)
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("could not run pkexec: {e}"))?;
    match out.status.code() {
        Some(0) => {}
        Some(126) => return Err("the password prompt was cancelled".into()),
        Some(127) => return Err("not authorized to install packages".into()),
        _ => {
            let err = failure_text(&out);
            return Err(err);
        }
    }
    let mut updated = inst.clone();
    if let Some(v) = programs.package_version(package, rpm, !rpm).await {
        updated.version = Some(v);
    }
    save(tools_dir, &updated)?;
    Ok(updated)
}

/// The version of a pending Flatpak update. Its short commit names an update
/// without a version, and is added when the version is the installed one
/// (a new build of the same version), e.g. "43.beta (0123456789ab)".
async fn flatpak_update(
    programs: &Programs,
    flatpak_id: &str,
    installation: Installation,
) -> Result<Option<String>, String> {
    let out = Command::new(&programs.flatpak)
        .args([
            "remote-ls",
            "--updates",
            "--app",
            installation.flag(),
            "--columns=application,version,commit",
        ])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(programs.network_timeout, out)
        .await
        .map_err(|_| "flatpak did not answer in time".to_string())?
        .map_err(|e| format!("could not run flatpak: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let pending = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| {
            let mut cols = line.split('\t').map(str::trim);
            if cols.next() != Some(flatpak_id) {
                return None;
            }
            let version = cols.next().unwrap_or_default().to_string();
            let commit: String = cols.next().unwrap_or_default().chars().take(12).collect();
            Some((version, commit))
        });
    let Some((version, commit)) = pending else {
        return Ok(None);
    };
    if version.is_empty() {
        return Ok(Some(commit));
    }
    let installed = programs.flatpak_version(flatpak_id, installation).await;
    Ok(Some(
        if installed.as_deref() == Some(version.as_str()) && !commit.is_empty() {
            format!("{version} ({commit})")
        } else {
            version
        },
    ))
}

/// Updates a Flatpak in the installation tuxhub uses. A system-wide
/// installation asks for the password through Polkit.
pub async fn update_flatpak<R: Runtime>(
    app: &AppHandle<R>,
    tools_dir: &Path,
    programs: &Programs,
    inst: &Installed,
    entry: &Entry,
) -> Result<Installed, String> {
    let Source::Flatpak { flatpak_id, .. } = &inst.entry.source else {
        return Err(format!("{} is not a Flatpak", inst.entry.name));
    };
    progress(app, stage(inst.id(), "flatpak", Some(flatpak_id)));
    run(
        &programs.flatpak,
        &[
            "update",
            inst.installation.flag(),
            "-y",
            "--noninteractive",
            flatpak_id,
        ],
    )
    .await?;
    // The workshop may have changed the entry, for example its icon. An
    // entry for another app or source does not describe this installation.
    let same_app =
        matches!(&entry.source, Source::Flatpak { flatpak_id: id, .. } if id == flatpak_id);
    let updated = Installed {
        entry: if same_app {
            entry.clone()
        } else {
            inst.entry.clone()
        },
        ..inst.clone()
    };
    save(tools_dir, &updated)?;
    Ok(updated)
}

/// Starts a program the test just copied. Another test may fork while the
/// copy is still open for writing; exec then fails with "Text file busy" for
/// a moment.
#[cfg(test)]
pub(crate) async fn spawn_copied(program: &Path, args: &[&str]) -> std::process::Child {
    for _ in 0..100 {
        match std::process::Command::new(program).args(args).spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            res => return res.unwrap(),
        }
    }
    panic!("{} stays busy", program.display());
}

/// Stand-ins for flatpak, rpm and dpkg-query: shell scripts that log each
/// call to `<dir>/calls` and then run the body a test gives them. A program
/// a test does not set is missing.
#[cfg(test)]
pub(crate) struct Fakes {
    pub dir: PathBuf,
    pub programs: Programs,
}

#[cfg(test)]
impl Fakes {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("tuxhub-fakes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let programs = Programs {
            flatpak: dir.join("flatpak"),
            rpm: dir.join("rpm"),
            dpkg_query: dir.join("dpkg-query"),
            dpkg: dir.join("dpkg"),
            dnf: dir.join("dnf"),
            apt: dir.join("apt"),
            apt_get: dir.join("apt-get"),
            pkexec: dir.join("pkexec"),
            timeout: Duration::from_secs(5),
            network_timeout: Duration::from_secs(5),
        };
        Self { dir, programs }
    }

    /// Makes `program` run `body` after logging its arguments. The file is
    /// written by a child process: a file this process held open for writing
    /// would be inherited by other tests' forks and fail with "Text file busy".
    pub fn set(&self, program: &str, body: &str) {
        let script = format!(
            "#!/bin/sh\necho \"$(basename \"$0\") $*\" >> '{}'\n{body}\n",
            self.dir.join("calls").display()
        );
        let path = self.dir.join(program);
        let mut child = std::process::Command::new("sh")
            .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
            .arg(&path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    }

    /// A flatpak that finds the app in the given installations and accepts
    /// every other command.
    pub fn flatpak_with(&self, installed: &[Installation]) {
        let flags: Vec<&str> = installed.iter().map(|i| i.flag()).collect();
        self.set(
            "flatpak",
            &format!(
                "if [ \"$1\" = info ]; then case \" {} \" in *\" $2 \"*) exit 0;; esac; exit 1; fi\nexit 0",
                flags.join(" ")
            ),
        );
    }

    /// A flatpak like [`Self::flatpak_with`] that names `origin` as the
    /// remote the app came from (`info --show-origin`).
    pub fn flatpak_from(&self, installed: &[Installation], origin: &str) {
        let flags: Vec<&str> = installed.iter().map(|i| i.flag()).collect();
        self.set(
            "flatpak",
            &format!(
                "if [ \"$1\" = info ]; then case \" {} \" in *\" $2 \"*) ;; *) exit 1;; esac\n  [ \"$3\" = --show-origin ] && printf '%s\\n' '{origin}'\n  exit 0\nfi\nexit 0",
                flags.join(" ")
            ),
        );
    }

    /// The logged calls, one line per call: program and arguments.
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }
}

#[cfg(test)]
impl Drop for Fakes {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_announcements_of_external_tools() {
        let tmp = std::env::temp_dir().join(format!("tuxhub-announce-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let apps = tmp.join("apps");
        let tool = tmp.join("tool");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::create_dir_all(&tool).unwrap();
        std::fs::write(
            tool.join("tuxhub.toml"),
            "id = \"demo\"\nname = \"Demo\"\nversion = \"1.2.0\"\n[uninstall]\ncommand = \"demo\"\n",
        )
        .unwrap();
        let announce = |file: &str, id: &str| {
            let p = apps.join(file);
            std::fs::write(
                &p,
                format!(
                    "id = \"{id}\"\npath = {:?}\nrepo = \"owner/demo\"\nasset = \"*.tar.gz\"\n",
                    tool.to_string_lossy()
                ),
            )
            .unwrap();
            p
        };

        let inst = announced(&announce("demo.toml", "demo"), true).unwrap();
        assert_eq!(inst.entry.name, "Demo");
        assert_eq!(inst.version.as_deref(), Some("1.2.0"));
        assert_eq!(inst.root_dir(&tmp), Some(tool.clone()));
        assert!(matches!(
            inst.entry.source,
            Source::External {
                user: true,
                asset: Some(_),
                ..
            }
        ));
        assert!(inst.manifest(&tmp).uninstall.is_some());

        // The id must match the file name and the tool's own manifest.
        assert!(announced(&announce("other.toml", "demo"), true).is_err());
        assert!(announced(&announce("other.toml", "other"), true).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Downloads the autoclicker release from GitHub (requires network
    /// access): `cargo test -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn installs_autoclicker_release() {
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../registry/index.json")).unwrap();
        let entry = entries.into_iter().find(|e| e.id == "autoclicker").unwrap();
        let app = tauri::test::mock_app();
        let tmp = std::env::temp_dir().join(format!("tuxhub-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let programs = Programs::default();
        let inst = install(app.handle(), &tmp, &programs, &entry)
            .await
            .unwrap();
        assert!(inst.version.as_deref().unwrap().starts_with('v'));
        assert_eq!(
            inst.detected_run, None,
            "a tool without [run] must not get a guessed command"
        );
        let m = inst.manifest(&tmp);
        assert_eq!(m.level(), 2);
        let root = inst.root_dir(&tmp).unwrap();
        assert!(root.join(&m.rpc.unwrap().command).is_file());
        if let Some(ui) = &m.ui {
            assert!(root.join(ui).is_file(), "UI {ui} missing from the release");
        }
        // Installing again exercises the update path.
        install(app.handle(), &tmp, &programs, &entry)
            .await
            .unwrap();
        assert_eq!(load_all(&tmp).len(), 1);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn workshop_index_parses() {
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../registry/index.json")).unwrap();
        assert!(!entries.is_empty());
        for e in &entries {
            e.validate().unwrap();
            assert!(!e.description.is_empty(), "{} has no description", e.id);
            if let Some(icon) = &e.icon {
                assert!(
                    icon.starts_with("https://"),
                    "{}: workshop icons must be HTTPS URLs",
                    e.id
                );
            }
        }
    }

    fn system_package(source: Value) -> Entry {
        serde_json::from_value(serde_json::json!({
            "id": "clock",
            "name": "Clock",
            "repo": "owner/clock",
            "source": source,
        }))
        .unwrap()
    }

    #[test]
    fn validates_system_packages() {
        let ok = system_package(serde_json::json!({
            "type": "system-package",
            "package": "clock",
            "rpm": "*.x86_64.rpm",
            "deb": "*_amd64.deb",
            "command": "/usr/bin/clock",
        }));
        ok.validate().unwrap();

        let mut no_repo = ok.clone();
        no_repo.repo = None;
        assert!(no_repo.validate().is_err());

        let bad = |package: &str, rpm: Option<&str>, command: &str| {
            system_package(serde_json::json!({
                "type": "system-package",
                "package": package,
                "rpm": rpm,
                "command": command,
            }))
        };
        for e in [
            bad("clock", None, "/usr/bin/clock"),
            bad("-clock", Some("*.rpm"), "/usr/bin/clock"),
            bad("clock x", Some("*.rpm"), "/usr/bin/clock"),
            bad("clock", Some("*.rpm"), "usr/bin/clock"),
            bad("clock", Some("*.rpm"), "/usr/bin/../clock"),
        ] {
            assert!(e.validate().is_err(), "{:?}", e.source);
        }
    }

    #[test]
    fn picks_a_package_manager_for_the_available_packages() {
        use PackageManager::*;
        let only = |cmds: &'static [&'static str]| move |c: &str| cmds.contains(&c);
        assert_eq!(
            PackageManager::pick(true, true, only(&["dnf", "apt-get"])),
            Some(Dnf)
        );
        assert_eq!(
            PackageManager::pick(true, true, only(&["zypper"])),
            Some(Zypper)
        );
        assert_eq!(
            PackageManager::pick(true, true, only(&["apt-get"])),
            Some(Apt)
        );
        // Only a package of the other family: nothing fits.
        assert_eq!(PackageManager::pick(true, false, only(&["apt-get"])), None);
        assert_eq!(PackageManager::pick(false, true, only(&["dnf"])), None);
        assert_eq!(PackageManager::pick(true, true, only(&[])), None);
    }

    #[test]
    fn package_manager_commands() {
        use PackageManager::*;
        let file = Path::new("/data/clock.pkg");
        assert_eq!(Dnf.install_args(file), ["install", "-y", "/data/clock.pkg"]);
        assert_eq!(
            Zypper.install_args(file),
            [
                "--non-interactive",
                "install",
                "--allow-unsigned-rpm",
                "/data/clock.pkg"
            ]
        );
        // apt-get treats the argument as a file only with a path.
        assert_eq!(Apt.install_args(file), ["install", "-y", "/data/clock.pkg"]);
        assert_eq!(Dnf.remove_args("clock"), ["remove", "-y", "clock"]);
        assert_eq!(
            Zypper.remove_args("clock"),
            ["--non-interactive", "remove", "clock"]
        );
        assert_eq!(Apt.remove_args("clock"), ["remove", "-y", "clock"]);
    }

    #[test]
    fn stored_entries_follow_the_workshop() {
        let base = std::env::temp_dir().join(format!("tuxhub-refresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let source = serde_json::json!({
            "type": "system-package",
            "package": "clock",
            "rpm": "*.x86_64.rpm",
            "command": "/usr/bin/clock",
        });
        let old = system_package(source.clone());
        let mut inst = Installed::new(old.clone());
        inst.version = Some("v2.0.0".into());
        inst.adopted = true;
        std::fs::create_dir_all(base.join("clock")).unwrap();
        save(&base, &inst).unwrap();

        let mut new = system_package(source);
        new.description = "New text".into();
        // A workshop entry of another kind never replaces an installed one.
        let mut other_kind: Entry = serde_json::from_value(serde_json::json!({
            "id": "clock",
            "name": "Clock",
            "repo": "owner/clock",
            "source": { "type": "github-release", "asset": "*.tar.gz" },
        }))
        .unwrap();
        other_kind.description = "Other".into();

        assert_eq!(refresh_entries(&base, &[other_kind]), 0);
        assert_eq!(
            load(&base, "clock").unwrap().entry.description,
            old.description
        );

        assert_eq!(refresh_entries(&base, std::slice::from_ref(&new)), 1);
        let stored = load(&base, "clock").unwrap();
        assert_eq!(stored.entry.description, "New text");
        // Everything else about the installation stays.
        assert_eq!(stored.version.as_deref(), Some("v2.0.0"));
        assert!(stored.adopted);
        // Nothing to do the second time.
        assert_eq!(refresh_entries(&base, &[new]), 0);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn system_packages_launch_their_command() {
        let entry = system_package(serde_json::json!({
            "type": "system-package",
            "package": "clock",
            "deb": "*.deb",
            "command": "/bin/sh",
        }));
        let inst = Installed::new(entry);
        assert!(inst.root_dir(Path::new("/tools")).is_none());
        let lc = launch_cmd(Path::new("/tools"), &inst).unwrap();
        assert_eq!(lc.program, PathBuf::from("/bin/sh"));
        assert!(!lc.flatpak);
    }

    /// A copy of `sleep`, so the test has an executable of its own.
    fn fake_app(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("tuxhub-app-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join(name);
        let sleep = ["/usr/bin/sleep", "/bin/sleep"]
            .into_iter()
            .find(|p| Path::new(p).is_file())
            .expect("sleep is installed");
        std::fs::copy(sleep, &app).unwrap();
        app
    }

    async fn wait_for(app: &Path, pid: u32) {
        for _ in 0..100 {
            if processes(app).contains(&pid) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("process {pid} does not run {}", app.display());
    }

    #[tokio::test]
    async fn finds_and_quits_running_applications() {
        use std::os::unix::process::ExitStatusExt;
        let app = fake_app("app");
        let mut child = spawn_copied(&app, &["30"]).await;
        let pid = child.id();
        wait_for(&app, pid).await;

        // An update replaces the file while the old version keeps running.
        std::fs::remove_file(&app).unwrap();
        assert!(processes(&app).contains(&pid));

        quit(&app).await.unwrap();
        assert!(!processes(&app).contains(&pid));
        assert_eq!(child.wait().unwrap().signal(), Some(15));
        let _ = std::fs::remove_dir_all(app.parent().unwrap());
    }

    #[tokio::test]
    async fn kills_applications_that_ignore_sigterm() {
        use std::os::unix::process::ExitStatusExt;
        let app = fake_app("stubborn");
        // An ignored signal stays ignored across exec.
        let mut child = std::process::Command::new("sh")
            .args(["-c", "trap '' TERM; exec \"$0\" 30"])
            .arg(&app)
            .spawn()
            .unwrap();
        let pid = child.id();
        wait_for(&app, pid).await;

        quit(&app).await.unwrap();
        assert_eq!(child.wait().unwrap().signal(), Some(9));
        let _ = std::fs::remove_dir_all(app.parent().unwrap());
    }

    #[test]
    fn rejects_unsafe_flatpak_ids() {
        let mut entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../registry/index.json")).unwrap();
        let mut e = entries.remove(1);
        e.source = Source::Flatpak {
            flatpak_id: "--help".into(),
            remote: "flathub".into(),
        };
        assert!(e.validate().is_err());
    }

    fn flatpak_entry() -> Entry {
        serde_json::from_value(flatpak_record("user")["entry"].clone()).unwrap()
    }

    fn package_entry(rpm: bool, deb: bool) -> Entry {
        system_package(serde_json::json!({
            "type": "system-package",
            "package": "clock",
            "rpm": rpm.then_some("*.rpm"),
            "deb": deb.then_some("*.deb"),
            "command": "/usr/bin/clock",
        }))
    }

    #[tokio::test]
    async fn detects_flatpak_installations() {
        use Installation::*;
        let f = Fakes::new("detect-flatpak");
        for found in [vec![], vec![System], vec![User], vec![User, System]] {
            f.flatpak_with(&found);
            let d = detect(&flatpak_entry(), &f.programs).await;
            assert_eq!(d.flatpak, found);
            assert!(!d.package);
            assert_eq!(d.any(), !found.is_empty());
        }
        let calls = f.calls();
        assert!(calls.contains(&"flatpak info --user net.example.App".to_string()));
        assert!(calls.contains(&"flatpak info --system net.example.App".to_string()));
    }

    #[tokio::test]
    async fn detects_system_packages() {
        let f = Fakes::new("detect-package");
        f.set("rpm", "exit 0");
        let d = detect(&package_entry(true, false), &f.programs).await;
        assert!(d.package);
        assert!(d.flatpak.is_empty());
        assert_eq!(f.calls(), ["rpm -q --quiet clock"]);

        f.set("rpm", "exit 1");
        assert!(
            !detect(&package_entry(true, false), &f.programs)
                .await
                .package
        );

        // A removed package can leave its configuration behind.
        f.set("dpkg-query", "printf 'deinstall ok config-files'");
        assert!(
            !detect(&package_entry(false, true), &f.programs)
                .await
                .package
        );
        f.set("dpkg-query", "printf 'install ok installed'");
        assert!(
            detect(&package_entry(false, true), &f.programs)
                .await
                .package
        );
    }

    /// `remote-ls` talks to the network and gets a timeout of its own.
    #[tokio::test]
    async fn a_hanging_update_check_fails() {
        let mut f = Fakes::new("update-hang");
        f.programs.network_timeout = Duration::from_millis(300);
        f.set("flatpak", "exec sleep 30");
        let inst = Installed::new(flatpak_entry());
        let start = std::time::Instant::now();
        assert!(check_update(&inst, &f.programs).await.is_err());
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn package_versions_compare_with_release_tags() {
        let f = Fakes::new("package-version");
        f.set("dpkg-query", "printf '1:1.4.2-1ubuntu1'");
        let p = &f.programs;
        assert_eq!(
            p.package_version("clock", false, true).await.as_deref(),
            Some("1.4.2")
        );
        f.set("rpm", "printf 2.0.1");
        assert_eq!(
            p.package_version("clock", true, true).await.as_deref(),
            Some("2.0.1")
        );
    }

    /// Without flatpak, rpm or dpkg-query nothing is installed, and no error.
    #[tokio::test]
    async fn missing_programs_find_nothing() {
        let f = Fakes::new("detect-missing");
        assert!(!detect(&flatpak_entry(), &f.programs).await.any());
        assert!(!detect(&package_entry(true, true), &f.programs).await.any());
    }

    #[tokio::test]
    async fn a_hanging_query_finds_nothing() {
        let mut f = Fakes::new("detect-hang");
        f.programs.timeout = Duration::from_millis(300);
        f.set("flatpak", "exec sleep 30");
        let start = std::time::Instant::now();
        assert!(!detect(&flatpak_entry(), &f.programs).await.any());
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }

    /// Ids from the catalog are only passed on after validation.
    #[tokio::test]
    async fn invalid_entries_are_not_queried() {
        let f = Fakes::new("detect-invalid");
        f.flatpak_with(&[Installation::User]);
        let mut e = flatpak_entry();
        e.source = Source::Flatpak {
            flatpak_id: "--help".into(),
            remote: "flathub".into(),
        };
        assert!(!detect(&e, &f.programs).await.any());
        assert!(f.calls().is_empty());
    }

    fn local_entry_with(id: &str, source: Value) -> Entry {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "name": "Some App",
            "source": source,
        }))
        .unwrap()
    }

    fn distro_entry(manager: &str) -> Entry {
        local_entry_with(
            "local.clock",
            serde_json::json!({
                "type": "distro-package",
                "package": "clock",
                "manager": manager,
                // Never a real program: a test must not start anything.
                "command": ["/nonexistent/tuxhub-test/clock", "--start"],
            }),
        )
    }

    fn launcher_entry(command: &[&str]) -> Entry {
        local_entry_with(
            "local.some-app",
            serde_json::json!({
                "type": "launcher",
                "desktop_id": "some-app.desktop",
                "command": command,
            }),
        )
    }

    #[test]
    fn validates_distro_packages_and_launchers() {
        assert!(distro_entry("rpm").validate().is_ok());
        assert!(distro_entry("deb").validate().is_ok());
        assert!(
            serde_json::from_value::<Source>(serde_json::json!({
                "type": "distro-package", "package": "clock", "manager": "pacman",
                "command": ["/usr/bin/clock"],
            }))
            .is_err()
        );
        for package in ["", "-x", "a b", "a;b"] {
            let mut e = distro_entry("rpm");
            let Source::DistroPackage { package: p, .. } = &mut e.source else {
                unreachable!()
            };
            *p = package.into();
            assert!(e.validate().is_err(), "{package:?}");
        }
        assert!(launcher_entry(&["/opt/a/run", "--x"]).validate().is_ok());
        let mut e = launcher_entry(&["/opt/a/run"]);
        if let Source::Launcher { desktop_id, .. } = &mut e.source {
            desktop_id.clear();
        }
        assert!(e.validate().is_err());
        // An argument list without bounds is refused.
        let many: Vec<String> = vec!["/opt/a/run".into(); 65];
        let refs: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(launcher_entry(&refs).validate().is_err());
        for command in [&[][..], &["run"], &["./run"], &["/opt/../bin/sh"], &[""]] {
            assert!(launcher_entry(command).validate().is_err(), "{command:?}");
            let mut e = distro_entry("rpm");
            let Source::DistroPackage { command: c, .. } = &mut e.source else {
                unreachable!()
            };
            *c = command.iter().map(|s| s.to_string()).collect();
            assert!(e.validate().is_err(), "{command:?}");
        }
    }

    #[tokio::test]
    async fn adopts_distro_package() {
        let f = Fakes::new("adopt-distro");
        let tools = f.dir.join("tools");
        f.set("rpm", "[ \"$2\" = --quiet ] && exit 0\nprintf 2.1");
        // An rpm package is never looked up with dpkg.
        f.set("dpkg-query", "exit 0");
        let inst = adopt(&tools, &f.programs, &distro_entry("rpm"))
            .await
            .unwrap();
        assert!(inst.adopted);
        assert_eq!(inst.version.as_deref(), Some("2.1"));
        let saved = load(&tools, "local.clock").unwrap();
        assert!(saved.adopted);
        assert!(matches!(
            saved.entry.source,
            Source::DistroPackage {
                manager: PackageFormat::Rpm,
                ..
            }
        ));
        assert!(f.calls().contains(&"rpm -q --quiet clock".to_string()));
        // Adding never installs or removes anything.
        assert!(f.calls().iter().all(|c| c.starts_with("rpm -q")));

        let _ = std::fs::remove_dir_all(&tools);
        f.set("rpm", "exit 1");
        let err = adopt(&tools, &f.programs, &distro_entry("rpm"))
            .await
            .unwrap_err();
        assert!(err.contains("is not installed on this computer"), "{err}");
        assert!(!tools.join("local.clock").exists());
    }

    #[tokio::test]
    async fn adopts_deb_distro_package() {
        let f = Fakes::new("adopt-distro-deb");
        let tools = f.dir.join("tools");
        f.set("rpm", "exit 0");
        f.set("dpkg-query", "printf 'deinstall ok config-files'");
        assert!(
            adopt(&tools, &f.programs, &distro_entry("deb"))
                .await
                .is_err()
        );
        f.set("dpkg-query", "printf 'install ok installed'");
        assert!(
            adopt(&tools, &f.programs, &distro_entry("deb"))
                .await
                .is_ok()
        );
        // A deb package is never looked up with rpm.
        assert!(!f.calls().iter().any(|c| c.starts_with("rpm")));
    }

    #[tokio::test]
    async fn adopts_local_command_only_if_executable() {
        let f = Fakes::new("adopt-launcher");
        let tools = f.dir.join("tools");
        let missing = f.dir.join("nothing-here");
        let entry = launcher_entry(&[missing.to_str().unwrap()]);
        assert!(adopt(&tools, &f.programs, &entry).await.is_err());
        assert!(!tools.join("local.some-app").exists());

        // A file that cannot be run is refused, too.
        let plain = f.dir.join("plain");
        std::fs::write(&plain, "text").unwrap();
        let entry = launcher_entry(&[plain.to_str().unwrap()]);
        assert!(adopt(&tools, &f.programs, &entry).await.is_err());

        f.set("run", "exit 0");
        let run = f.dir.join("run");
        let entry = launcher_entry(&[run.to_str().unwrap(), "--x"]);
        let inst = adopt(&tools, &f.programs, &entry).await.unwrap();
        assert!(inst.adopted);
        assert!(tools.join("local.some-app/tool.json").is_file());
        assert!(f.calls().is_empty(), "{:?}", f.calls());
    }

    #[test]
    fn launches_stored_command() {
        for entry in [launcher_entry(&["/opt/a/run", "--x"]), {
            let mut e = distro_entry("rpm");
            if let Source::DistroPackage { command, .. } = &mut e.source {
                *command = vec!["/opt/a/run".into(), "--x".into()];
            }
            e
        }] {
            let inst = Installed::new(entry);
            assert!(inst.root_dir(Path::new("/tools")).is_none());
            let lc = launch_cmd(Path::new("/tools"), &inst).unwrap();
            assert_eq!(lc.program, PathBuf::from("/opt/a/run"));
            assert_eq!(lc.args, ["--x"]);
            assert!(lc.cwd.is_none());
            assert!(!lc.flatpak);
        }
    }

    /// A hand-edited tool.json never starts a relative or unbounded command.
    #[test]
    fn launch_checks_the_stored_command() {
        for command in [vec![], vec!["run".to_string()], vec!["./run".into()]] {
            let mut e = launcher_entry(&["/opt/a/run"]);
            if let Source::Launcher { command: c, .. } = &mut e.source {
                *c = command.clone();
            }
            assert!(
                launch_cmd(Path::new("/tools"), &Installed::new(e)).is_err(),
                "{command:?}"
            );
        }
    }

    #[test]
    fn knows_shared_launchers() {
        for shared in [
            "/bin/sh",
            "/usr/bin/bash",
            "zsh",
            "/usr/bin/python3",
            "/usr/bin/python3.13",
            "/usr/bin/perl5.40",
            "ruby3.3",
            "node",
            "/usr/bin/java",
            "/usr/bin/wine64",
            "/usr/bin/steam",
            "/usr/bin/flatpak",
            "/usr/bin/env",
            "xdg-open",
            "kioclient5",
            "electron33",
            "/usr/bin/dotnet",
            "busybox",
            "/usr/bin/snap",
        ] {
            assert!(is_shared_launcher(Path::new(shared)), "{shared}");
        }
        // A snap app's command links to `snap`, which every snap app shares.
        let dir = std::env::temp_dir().join(format!("tuxhub-snap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("snap"), "").unwrap();
        std::os::unix::fs::symlink(dir.join("snap"), dir.join("copilot-desktop")).unwrap();
        assert!(is_shared_launcher(&dir.join("copilot-desktop")));
        std::fs::remove_dir_all(&dir).unwrap();
        for own in [
            "/opt/a/run",
            "/usr/bin/dolphin",
            "steamcmd-gui",
            "nodejs-app",
            "/usr/bin/gimp",
        ] {
            assert!(!is_shared_launcher(Path::new(own)), "{own}");
        }
    }

    /// An app started through a shell, interpreter or launcher shares its
    /// program with unrelated processes: tuxhub never watches or quits it.
    #[tokio::test]
    async fn shared_launchers_are_never_watched() {
        let f = Fakes::new("shared-launcher");
        let tools = f.dir.join("tools");
        // A link named like the app does not hide that it runs bash.
        let link = f.dir.join("my-app");
        std::os::unix::fs::symlink("/bin/sh", &link).unwrap();
        for command in [
            vec!["/bin/sh", "-c", "x"],
            vec![link.to_str().unwrap(), "-c", "x"],
        ] {
            let inst = adopt(&tools, &f.programs, &launcher_entry(&command))
                .await
                .unwrap();
            assert!(app_program(&tools, &inst).is_none(), "{command:?}");
            assert!(!app_running(&tools, &inst));
            assert!(shared_launcher(&inst));
            let _ = std::fs::remove_dir_all(tools.join(inst.id()));
        }
        let mut e = distro_entry("rpm");
        if let Source::DistroPackage { command, .. } = &mut e.source {
            *command = vec!["/usr/bin/python3".into(), "/opt/x/app.py".into()];
        }
        assert!(app_program(&tools, &Installed::new(e)).is_none());

        // An app's own program is watched as before.
        f.set("own-app", "exit 0");
        let own = f.dir.join("own-app");
        let inst = Installed::new(launcher_entry(&[own.to_str().unwrap()]));
        assert_eq!(app_program(&tools, &inst), Some(own));
        assert!(!shared_launcher(&inst));
    }

    /// The upstream version may hold `-` itself (Debian's `2.3-rc1-1`); only
    /// the last part is the release.
    #[test]
    fn package_versions_keep_hyphens_of_the_upstream_version() {
        let ok = |raw: &str| clean_version(raw).unwrap();
        assert_eq!(ok("2.3-rc1-1"), "2.3-rc1");
        assert_eq!(ok("1:2.0-beta-2.fc44"), "2.0-beta");
        assert_eq!(ok("26.08.2-1.fc44"), "26.08.2");
        assert_eq!(ok("1.2"), "1.2");
        for bad in ["", "-1", "1:-x-1", "a b-1", "1.0\n-1", &"9".repeat(65)] {
            assert!(clean_version(bad).is_err(), "{bad:?}");
        }
    }

    /// A browser's web app runs as the browser: tuxhub never watches or
    /// quits it.
    #[test]
    fn web_apps_share_the_browser() {
        for command in [
            [
                "/usr/bin/chromium-browser",
                "--profile-directory=Default",
                "--app-id=abc",
            ],
            [
                "/opt/google/chrome/chrome",
                "--app=https://example.org/",
                "-v",
            ],
        ] {
            let inst = Installed::new(launcher_entry(&command));
            assert!(shared_launcher(&inst), "{command:?}");
            assert!(app_program(Path::new("/nonexistent"), &inst).is_none());
        }
        let inst = Installed::new(launcher_entry(&["/usr/bin/app", "--appearance=dark"]));
        assert!(!shared_launcher(&inst));
    }

    /// A program that is a `#!` script (like `/usr/bin/libreoffice`) runs as
    /// its interpreter: its processes are never found by the program.
    #[test]
    fn knows_script_programs() {
        let f = Fakes::new("script-program");
        f.set("script-app", "exit 0");
        let script = f.dir.join("script-app");
        assert!(script_program(&Installed::new(launcher_entry(&[script
            .to_str()
            .unwrap()]))));
        let mut e = distro_entry("rpm");
        if let Source::DistroPackage { command, .. } = &mut e.source {
            *command = vec![script.to_str().unwrap().into()];
        }
        assert!(script_program(&Installed::new(e)));
        // A binary, a missing file and a short file are not scripts.
        for program in ["/usr/bin/true", "/nonexistent/tuxhub-test/app"] {
            assert!(
                !script_program(&Installed::new(launcher_entry(&[program]))),
                "{program}"
            );
        }
        let short = f.dir.join("short");
        std::fs::write(&short, "#").unwrap();
        assert!(!script_program(&Installed::new(launcher_entry(&[short
            .to_str()
            .unwrap()]))));
    }

    #[tokio::test]
    async fn distro_package_missing_after_removal() {
        let f = Fakes::new("distro-missing");
        let tools = f.dir.join("tools");
        f.set("rpm", "exit 0");
        let mut inst = adopt(&tools, &f.programs, &distro_entry("rpm"))
            .await
            .unwrap();
        assert!(
            !check_installation(&tools, &f.programs, &mut inst)
                .await
                .missing
        );
        f.set("rpm", "exit 1");
        let state = check_installation(&tools, &f.programs, &mut inst).await;
        assert!(state.missing);
        assert!(!state.duplicate);
        // Starting it fails with an error, nothing panics.
        assert!(launch(&tools, &inst).is_err());
    }

    #[tokio::test]
    async fn launcher_missing_after_removal() {
        let f = Fakes::new("launcher-missing");
        let tools = f.dir.join("tools");
        f.set("run", "exit 0");
        let run = f.dir.join("run");
        let mut inst = adopt(
            &tools,
            &f.programs,
            &launcher_entry(&[run.to_str().unwrap()]),
        )
        .await
        .unwrap();
        assert!(
            !check_installation(&tools, &f.programs, &mut inst)
                .await
                .missing
        );
        std::fs::remove_file(&run).unwrap();
        assert!(
            check_installation(&tools, &f.programs, &mut inst)
                .await
                .missing
        );
    }

    /// Apps added from this computer belong to their package manager or
    /// installer: tuxhub only forgets them, it never removes or installs them.
    #[tokio::test]
    async fn added_apps_are_never_uninstalled_or_installed() {
        let f = Fakes::new("added-uninstall");
        let tools = f.dir.join("tools");
        f.set("rpm", "exit 0");
        for entry in [distro_entry("rpm"), launcher_entry(&["/bin/sh"])] {
            let inst = adopt(&tools, &f.programs, &entry).await.unwrap();
            let err = uninstall(&tools, &f.programs, &inst).await.unwrap_err();
            assert!(err.contains("remove it from tuxhub"), "{err}");
            assert!(tools.join(inst.id()).join("tool.json").is_file());
            let app = tauri::test::mock_app();
            assert!(
                install(app.handle(), &tools, &f.programs, &entry)
                    .await
                    .is_err()
            );
        }
        assert!(f.calls().iter().all(|c| c.starts_with("rpm -q")));
    }

    /// tool.json files written before the new sources existed load as before.
    #[test]
    fn old_records_still_load() {
        // Each written with every field a record had, so it reads back as is.
        let records = [
            serde_json::json!({
                "entry": {
                    "id": "demo", "name": "Demo", "description": "",
                    "source": { "type": "flatpak", "flatpak_id": "net.example.App",
                                "remote": "flathub" },
                },
                "version": null, "installation": "system",
            }),
            serde_json::json!({
                "entry": {
                    "id": "clock", "name": "Clock", "description": "", "repo": "owner/clock",
                    "source": { "type": "system-package", "package": "clock",
                                "rpm": "*.rpm", "command": "/usr/bin/clock" },
                },
                "version": "1.0", "adopted": true,
            }),
            serde_json::json!({
                "entry": { "id": "dev", "name": "Dev", "description": "",
                           "source": { "type": "local", "path": "/home/u/dev" } },
                "version": null, "root": null, "detected_run": "run.sh",
            }),
            serde_json::json!({
                "entry": { "id": "rel", "name": "Rel", "description": "", "repo": "owner/rel",
                           "source": { "type": "github-release", "asset": "*.AppImage" } },
                "version": "v2", "root": "files", "window": "own",
            }),
        ];
        for record in records {
            let inst: Installed = serde_json::from_value(record.clone()).unwrap();
            let again = serde_json::to_value(&inst).unwrap();
            assert_eq!(again["entry"], record["entry"]);
            assert_eq!(again["version"], record["version"]);
            assert_eq!(inst.adopted, record["adopted"] == true);
        }
    }

    #[test]
    fn new_sources_round_trip() {
        for entry in [distro_entry("deb"), launcher_entry(&["/opt/a/run"])] {
            let json = serde_json::to_value(&entry).unwrap();
            let back: Entry = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(serde_json::to_value(&back).unwrap(), json);
        }
        assert_eq!(
            serde_json::to_value(distro_entry("deb")).unwrap()["source"]["type"],
            "distro-package"
        );
        assert_eq!(
            serde_json::to_value(launcher_entry(&["/a"])).unwrap()["source"]["type"],
            "launcher"
        );
    }

    fn flatpak_record(installation: &str) -> Value {
        serde_json::json!({
            "entry": {
                "id": "demo",
                "name": "Demo",
                "source": { "type": "flatpak", "flatpak_id": "net.example.App" },
            },
            "version": null,
            "installation": installation,
        })
    }

    /// tool.json written before installations were detected: tuxhub
    /// installed every Flatpak for the user itself.
    #[test]
    fn old_records_are_own_user_installations() {
        let inst: Installed = serde_json::from_value(serde_json::json!({
            "entry": flatpak_record("user")["entry"],
            "version": null,
            "root": null,
            "detected_run": null,
        }))
        .unwrap();
        assert!(!inst.adopted);
        assert_eq!(inst.installation, Installation::User);
        assert!(!inst.duplicate_dismissed);
    }

    #[test]
    fn rejects_an_unknown_installation() {
        assert!(serde_json::from_value::<Installed>(flatpak_record("root")).is_err());
        let inst: Installed = serde_json::from_value(flatpak_record("system")).unwrap();
        assert_eq!(inst.installation, Installation::System);
    }

    #[test]
    fn launches_a_flatpak_from_its_installation() {
        for (installation, flag) in [("system", "--system"), ("user", "--user")] {
            let inst: Installed = serde_json::from_value(flatpak_record(installation)).unwrap();
            let lc = launch_cmd(Path::new("/tools"), &inst).unwrap();
            assert_eq!(lc.program, PathBuf::from("flatpak"));
            assert_eq!(lc.args, ["run", flag, "net.example.App"]);
            assert!(lc.flatpak);
        }
    }

    /// tuxhub removes the Flatpak from the installation it uses, and a
    /// Flatpak that is already gone does not keep the tool in the list.
    #[test]
    fn flatpak_uninstall_targets_the_recorded_installation() {
        assert_eq!(
            flatpak_uninstall_args("net.example.App", Installation::User),
            [
                "uninstall",
                "--user",
                "-y",
                "--noninteractive",
                "net.example.App"
            ]
        );
        assert_eq!(
            flatpak_uninstall_args("net.example.App", Installation::System)[1],
            "--system"
        );
        assert!(flatpak_already_gone(
            "error: No installed refs found for \u{2018}net.example.App\u{2019}"
        ));
        assert!(!flatpak_already_gone(
            "error: Could not unmount revokefs-fuse"
        ));
    }

    fn manifest(toml: &str) -> Manifest {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn integration_follows_the_manifest() {
        assert_eq!(
            integration_of(&manifest("live = true")),
            Integration::Socket
        );
        assert_eq!(
            integration_of(&manifest("live = true\nembed = false")),
            Integration::Socket
        );
        assert_eq!(
            integration_of(&manifest("embed = false")),
            Integration::Standalone
        );
        assert_eq!(integration_of(&Manifest::default()), Integration::Embedded);
        assert_eq!(
            integration_of(&manifest("web_url = \"http://localhost:1/\"")),
            Integration::Embedded
        );
    }

    #[test]
    fn every_workshop_entry_names_its_integration() {
        let entries: Vec<Entry> =
            serde_json::from_str(include_str!("../../registry/index.json")).unwrap();
        assert!(!entries.is_empty());
        for e in &entries {
            let level = e
                .integration
                .unwrap_or_else(|| panic!("{} has no valid integration", e.id));
            if let Some(m) = &e.manifest {
                assert_eq!(level, integration_of(m), "{}", e.id);
            }
        }
    }

    #[test]
    fn an_unknown_integration_counts_as_missing() {
        let e: Entry = serde_json::from_value(serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "integration": "foo",
            "source": { "type": "flatpak", "flatpak_id": "net.example.Demo" },
        }))
        .unwrap();
        assert_eq!(e.integration, None);
        let e: Entry = serde_json::from_value(serde_json::json!({
            "id": "demo",
            "name": "Demo",
            "integration": "socket",
            "source": { "type": "flatpak", "flatpak_id": "net.example.Demo" },
        }))
        .unwrap();
        assert_eq!(e.integration, Some(Integration::Socket));
    }

    #[test]
    fn reads_running_flatpaks() {
        let out = "com.core447.StreamController\ncom.discordapp.Discord\n";
        assert!(flatpak_ps_has(out, "com.core447.StreamController"));
        assert!(flatpak_ps_has(
            "  com.core447.StreamController  \n",
            "com.core447.StreamController"
        ));
        assert!(!flatpak_ps_has(out, "com.core447"));
        assert!(!flatpak_ps_has(out, "com.core447.StreamController.Locale"));
        assert!(!flatpak_ps_has("", "com.core447.StreamController"));
        assert!(!flatpak_ps_has(
            "com.core447.StreamController.Extra\n",
            "com.core447.StreamController"
        ));
    }

    #[tokio::test]
    async fn asks_flatpak_whether_an_app_runs() {
        let f = Fakes::new("flatpak-ps");
        f.set(
            "flatpak",
            "printf 'net.example.other\\nnet.example.demo\\n'",
        );
        assert_eq!(
            flatpak_running(&f.programs, "net.example.demo").await,
            Some(true)
        );
        assert_eq!(
            flatpak_running(&f.programs, "net.example.gone").await,
            Some(false)
        );
        assert!(
            f.calls()
                .contains(&"flatpak ps --columns=application".to_string())
        );
        f.set("flatpak", "exit 1");
        assert_eq!(flatpak_running(&f.programs, "net.example.demo").await, None);
        let mut missing = f.programs.clone();
        missing.flatpak = f.dir.join("no-flatpak");
        assert_eq!(flatpak_running(&missing, "net.example.demo").await, None);
    }

    #[tokio::test]
    async fn ends_a_running_flatpak() {
        let f = Fakes::new("flatpak-kill");
        let state = f.dir.join("running");
        std::fs::write(&state, "net.example.demo\n").unwrap();
        f.set(
            "flatpak",
            &format!(
                "case \"$1\" in ps) cat '{0}';; kill) : > '{0}';; esac",
                state.display()
            ),
        );
        flatpak_kill(&f.programs, "net.example.demo", Duration::from_millis(500))
            .await
            .unwrap();
        assert!(
            f.calls()
                .contains(&"flatpak kill net.example.demo".to_string())
        );
        assert_eq!(
            flatpak_running(&f.programs, "net.example.demo").await,
            Some(false)
        );

        // The app is still there after kill: the caller must not go on.
        f.set("flatpak", "case \"$1\" in ps) echo net.example.demo;; esac");
        assert!(
            flatpak_kill(&f.programs, "net.example.demo", Duration::from_millis(300))
                .await
                .is_err()
        );
    }

    /// An AppImage runs its app from a temporary mount; only `APPIMAGE` in
    /// the environment names the file tuxhub started.
    #[tokio::test]
    async fn finds_the_app_an_appimage_started() {
        let dir = std::env::temp_dir().join(format!("tuxhub-appimage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("App.AppImage");
        std::fs::write(&image, "not started").unwrap();
        let inner = dir.join("inner");
        std::fs::copy("/usr/bin/sleep", &inner).unwrap();
        let mut child = loop {
            match std::process::Command::new(&inner)
                .arg("30")
                .env("APPIMAGE", &image)
                .spawn()
            {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                r => break r.unwrap(),
            }
        };
        assert_eq!(processes(&image), [child.id()]);
        // Another file with the same start of its name is not this app.
        assert!(processes(&dir.join("App.App")).is_empty());
        quit(&image).await.unwrap();
        assert!(child.wait().is_ok());
        assert!(processes(&image).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Updates of apps adopted from the distribution

    fn distro_inst(manager: &str) -> Installed {
        let mut inst = Installed::new(distro_entry(manager));
        inst.version = Some("2.1".into());
        inst.adopted = true;
        inst
    }

    #[tokio::test]
    async fn dnf_reports_update() {
        let f = Fakes::new("dnf-update");
        f.set(
            "dnf",
            "echo 'Last metadata check'; printf 'other.x86_64  1.0-1.fc44  updates\\nclock.x86_64  2:25.08.2-1.fc44  updates\\n'; exit 100",
        );
        let got = check_update(&distro_inst("rpm"), &f.programs).await;
        assert_eq!(got, Ok(Some("25.08.2".into())));
        assert_eq!(f.calls(), ["dnf -q check-update clock"]);
    }

    #[tokio::test]
    async fn dnf_no_update() {
        let f = Fakes::new("dnf-none");
        f.set("dnf", "exit 0");
        assert_eq!(
            check_update(&distro_inst("rpm"), &f.programs).await,
            Ok(None)
        );
    }

    #[tokio::test]
    async fn dnf_failure_is_error() {
        let f = Fakes::new("dnf-fail");
        let tools = f.dir.join("tools");
        f.set("dnf", "echo 'locked' >&2; exit 1");
        let inst = distro_inst("rpm");
        std::fs::create_dir_all(tools.join("local.clock")).unwrap();
        save(&tools, &inst).unwrap();
        let err = check_update(&inst, &f.programs).await.unwrap_err();
        assert!(err.contains("locked"), "{err}");
        assert_eq!(
            load(&tools, "local.clock").unwrap().version.as_deref(),
            Some("2.1")
        );
    }

    #[tokio::test]
    async fn dnf_odd_output_is_rejected() {
        let f = Fakes::new("dnf-odd");
        for body in [
            "printf 'clock.x86_64\\n'; exit 100",
            "printf 'clock.x86_64  bad\\001ver-1  updates\\n'; exit 100",
            "printf 'clock.x86_64  -1.fc44  updates\\n'; exit 100",
            "printf 'clock.x86_64  1.%s-1.fc44  updates\\n' \"$(printf 'x%.0s' $(seq 1 100))\"; exit 100",
            "exit 100",
        ] {
            f.set("dnf", body);
            assert!(
                check_update(&distro_inst("rpm"), &f.programs)
                    .await
                    .is_err(),
                "{body}"
            );
        }
    }

    #[tokio::test]
    async fn apt_reports_update() {
        let f = Fakes::new("apt-update");
        f.set(
            "apt",
            "echo 'Listing...'; echo 'clock/stable 1:3.0.21-1 amd64 [upgradable from: 3.0.20-1]'",
        );
        let got = check_update(&distro_inst("deb"), &f.programs).await;
        assert_eq!(got, Ok(Some("3.0.21".into())));
        assert_eq!(f.calls(), ["apt list --upgradable clock"]);
        f.set("apt", "echo 'Listing...'");
        assert_eq!(
            check_update(&distro_inst("deb"), &f.programs).await,
            Ok(None)
        );
        f.set("apt", "exit 1");
        assert!(
            check_update(&distro_inst("deb"), &f.programs)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_hanging_package_manager_fails() {
        let mut f = Fakes::new("dnf-hang");
        f.programs.network_timeout = Duration::from_millis(300);
        f.set("dnf", "exec sleep 30");
        let start = std::time::Instant::now();
        assert!(
            check_update(&distro_inst("rpm"), &f.programs)
                .await
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn launcher_never_updates() {
        let f = Fakes::new("launcher-update");
        let inst = Installed::new(launcher_entry(&["/bin/sh", "-c", "x"]));
        assert_eq!(check_update(&inst, &f.programs).await, Ok(None));
        assert!(f.calls().is_empty());
        let tools = f.dir.join("tools");
        assert!(
            update_distro_package(&tools, &f.programs, &inst)
                .await
                .is_err()
        );
        assert!(f.calls().is_empty());
    }

    #[tokio::test]
    async fn upgrade_runs_through_pkexec_and_reads_the_new_version() {
        let f = Fakes::new("upgrade-ok");
        let tools = f.dir.join("tools");
        let inst = distro_inst("rpm");
        std::fs::create_dir_all(tools.join("local.clock")).unwrap();
        save(&tools, &inst).unwrap();
        f.set("pkexec", "exit 0");
        f.set("rpm", "printf 25.08.2");
        let out = update_distro_package(&tools, &f.programs, &inst)
            .await
            .unwrap();
        assert_eq!(out.version.as_deref(), Some("25.08.2"));
        assert!(out.adopted);
        assert_eq!(
            load(&tools, "local.clock").unwrap().version.as_deref(),
            Some("25.08.2")
        );
        let calls = f.calls();
        assert!(
            calls[0].starts_with("pkexec ") && calls[0].ends_with("dnf -y upgrade clock"),
            "{calls:?}"
        );

        let f = Fakes::new("upgrade-deb");
        let tools = f.dir.join("tools");
        let inst = distro_inst("deb");
        std::fs::create_dir_all(tools.join("local.clock")).unwrap();
        f.set("pkexec", "exit 0");
        f.set("dpkg-query", "printf 3.0.21-1");
        let out = update_distro_package(&tools, &f.programs, &inst)
            .await
            .unwrap();
        assert_eq!(out.version.as_deref(), Some("3.0.21"));
        assert!(f.calls()[0].ends_with("apt-get install -y --only-upgrade clock"));
    }

    #[tokio::test]
    async fn pkexec_cancel_keeps_version() {
        let f = Fakes::new("upgrade-cancel");
        let tools = f.dir.join("tools");
        let inst = distro_inst("rpm");
        std::fs::create_dir_all(tools.join("local.clock")).unwrap();
        save(&tools, &inst).unwrap();
        f.set("pkexec", "exit 126");
        f.set("rpm", "printf 99");
        let err = update_distro_package(&tools, &f.programs, &inst)
            .await
            .unwrap_err();
        assert!(err.contains("cancelled"), "{err}");
        assert_eq!(
            load(&tools, "local.clock").unwrap().version.as_deref(),
            Some("2.1")
        );
        assert!(f.calls().iter().all(|c| !c.starts_with("rpm")));
    }
}
