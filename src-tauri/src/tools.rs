//! Installed tools: install workshop entries, launch, update and remove them.
//!
//! Layout below `<data>/tools/<id>/`:
//! - `tool.json`: the workshop entry and the installed version
//! - `files/`: the extracted release (GitHub releases only)
//!
//! Tools installed outside tuxhub announce themselves with a file
//! `tuxhub/apps/<id>.toml` in `$XDG_DATA_HOME` or one of `$XDG_DATA_DIRS`
//! (see [`discover`]). They appear like any other installed tool.

use crate::github;
use crate::manifest::{Manifest, safe_relative, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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
        if matches!(self.source, Source::GithubRelease { .. }) && self.repo.is_none() {
            return Err(format!(
                "{}: a GitHub release source requires `repo`",
                self.id
            ));
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
            Source::Flatpak { .. } => None,
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
        Source::Local { .. } => Err("local tools are linked, not installed".into()),
        Source::External { .. } => Err("this tool is installed outside tuxhub".into()),
    }
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
    let files = staging.join("files");
    std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;

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

    progress(app, stage(id, "extracting", None));
    let lower = asset.name.to_lowercase();
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
        let target = files.join(&asset.name);
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
    Ok((rel.tag_name, root))
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

/// Returns the latest release tag if it differs from the installed one.
pub async fn check_update(inst: &Installed) -> Result<Option<String>, String> {
    let updatable = match &inst.entry.source {
        Source::GithubRelease { .. } => true,
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
