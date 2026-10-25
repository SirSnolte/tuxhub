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
}

fn default_remote() -> String {
    "flathub".into()
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
            // The name is passed to the package manager as an argument.
            let name_ok = !package.is_empty()
                && !package.starts_with('-')
                && package
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c));
            if !name_ok {
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
        if let Source::Flatpak { flatpak_id, remote } = &self.source {
            // Never let an id start with "-": it would be parsed as a flatpak option.
            let ok = |s: &str| {
                !s.is_empty()
                    && !s.starts_with('-')
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            };
            if !ok(flatpak_id) || !ok(remote) {
                return Err(format!("{}: invalid Flatpak id or remote", self.id));
            }
        }
        if let Some(m) = &self.manifest {
            m.validate()?;
        }
        Ok(())
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
            Source::Flatpak { .. } | Source::SystemPackage { .. } => None,
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
        translations: BTreeMap::new(),
        id: a.id,
    };
    entry.validate()?;
    Ok(Installed {
        version: m.version.clone(),
        entry,
        root: None,
        detected_run: None,
    })
}

fn write_record(dir: &Path, inst: &Installed) -> Result<(), String> {
    std::fs::write(
        dir.join("tool.json"),
        serde_json::to_string_pretty(inst).unwrap(),
    )
    .map_err(|e| e.to_string())
}

async fn run(cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("could not run {cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub async fn install<R: Runtime>(
    app: &AppHandle<R>,
    tools_dir: &Path,
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
                    "flatpak",
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
                "flatpak",
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
            let inst = Installed {
                entry: entry.clone(),
                version: None,
                root: None,
                detected_run: None,
            };
            write_record(&dir, &inst)?;
            Ok(inst)
        }
        Source::SystemPackage { .. } => install_system_package(app, tools_dir, entry).await,
        Source::Local { .. } => Err("local tools are linked, not installed".into()),
        Source::External { .. } => Err("this tool is installed outside tuxhub".into()),
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
            entry: entry.clone(),
            version: Some(tag),
            root: None,
            detected_run: None,
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
        entry: entry.clone(),
        version: Some(tag),
        root: Some(root),
        detected_run: None,
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
        entry,
        root: None,
        detected_run: None,
    };
    write_record(&dir, &inst)?;
    Ok(inst)
}

/// Removes a tool. The tool's own `uninstall` command runs first, so it can
/// remove what it set up itself. Tools installed outside tuxhub are removed
/// by that command alone.
pub async fn uninstall(tools_dir: &Path, inst: &Installed) -> Result<(), String> {
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
        run(
            "flatpak",
            &["uninstall", "-y", "--noninteractive", flatpak_id],
        )
        .await?;
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
            args: vec!["run".into(), flatpak_id.clone()],
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

/// The program of a tool that tuxhub only starts and stops (`embed = false`).
pub fn app_program(tools_dir: &Path, inst: &Installed) -> Option<PathBuf> {
    if inst.manifest(tools_dir).embed != Some(false) {
        return None;
    }
    launch_cmd(tools_dir, inst)
        .ok()
        .filter(|lc| !lc.flatpak)
        .map(|lc| lc.program)
}

/// Whether a tool that tuxhub only starts and stops is running, also when it
/// was started outside tuxhub.
pub fn app_running(tools_dir: &Path, inst: &Installed) -> bool {
    app_program(tools_dir, inst).is_some_and(|p| !processes(&p).is_empty())
}

/// Processes of the current user that run `program`. The processes of other
/// users cannot be read and are left out.
pub fn processes(program: &Path) -> Vec<u32> {
    let program = program
        .canonicalize()
        .unwrap_or_else(|_| program.to_path_buf());
    let Ok(rd) = std::fs::read_dir("/proc") else {
        return vec![];
    };
    rd.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| runs(&exe, &program))
        })
        .collect()
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

/// Returns the latest release tag if it differs from the installed one.
pub async fn check_update(inst: &Installed) -> Result<Option<String>, String> {
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

        let inst = install(app.handle(), &tmp, &entry).await.unwrap();
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
        install(app.handle(), &tmp, &entry).await.unwrap();
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
    fn system_packages_launch_their_command() {
        let entry = system_package(serde_json::json!({
            "type": "system-package",
            "package": "clock",
            "deb": "*.deb",
            "command": "/bin/sh",
        }));
        let inst = Installed {
            entry,
            version: None,
            root: None,
            detected_run: None,
        };
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
        let mut child = std::process::Command::new(&app).arg("30").spawn().unwrap();
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
}
