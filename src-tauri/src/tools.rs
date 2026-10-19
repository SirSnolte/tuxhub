//! Installierte Tools: Workshop-Einträge installieren, starten, aktualisieren
//! und entfernen.
//!
//! Layout unter `<data>/tools/<id>/`:
//! - `tool.json` – Workshop-Eintrag + installierte Version
//! - `files/`    – entpacktes Release (nur bei GitHub-Releases)

use crate::github;
use crate::manifest::{Manifest, safe_relative, valid_id};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::process::Command;

/// Ein Eintrag im Workshop (`registry/index.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// `owner/name` auf GitHub
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    pub source: Source,
    /// Überschreibt ein `tuxhub.toml` aus dem Repo. So lassen sich fremde
    /// Tools integrieren, ohne dass ihr Repo etwas ändern muss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Manifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Source {
    /// Neuestes Release von `repo`; `asset` ist ein Muster wie `*x86_64.AppImage`.
    GithubRelease { asset: String },
    Flatpak {
        flatpak_id: String,
        #[serde(default = "default_remote")]
        remote: String,
    },
    /// Lokal verknüpfter Ordner (Entwicklermodus).
    Local { path: String },
}

fn default_remote() -> String {
    "flathub".into()
}

impl Entry {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_id(&self.id) {
            return Err(format!("ungültige Tool-ID {:?}", self.id));
        }
        if matches!(self.source, Source::GithubRelease { .. }) && self.repo.is_none() {
            return Err(format!("{}: GitHub-Release braucht `repo`", self.id));
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
    /// Release-Tag (GitHub) bzw. Version aus dem Manifest
    #[serde(default)]
    pub version: Option<String>,
    /// Ordner mit den Tool-Dateien, relativ zu `tools/<id>`
    #[serde(default)]
    pub root: Option<String>,
    /// Automatisch erkannte Startdatei, falls das Manifest keine nennt
    #[serde(default)]
    pub detected_run: Option<String>,
}

#[derive(Clone, Serialize)]
struct Progress<'a> {
    id: &'a str,
    text: String,
    fraction: Option<f64>,
}

fn progress<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    text: impl Into<String>,
    fraction: Option<f64>,
) {
    let _ = app.emit(
        "tool-progress",
        Progress {
            id,
            text: text.into(),
            fraction,
        },
    );
}

impl Installed {
    pub fn id(&self) -> &str {
        &self.entry.id
    }

    /// Ordner, in dem die Dateien des Tools liegen (Flatpaks haben keinen).
    pub fn root_dir(&self, tools_dir: &Path) -> Option<PathBuf> {
        match &self.entry.source {
            Source::Local { path } => Some(PathBuf::from(path)),
            Source::GithubRelease { .. } => Some(
                tools_dir
                    .join(&self.entry.id)
                    .join(self.root.as_deref().unwrap_or("files")),
            ),
            Source::Flatpak { .. } => None,
        }
    }

    /// Manifest aus dem Workshop-Eintrag, sonst `tuxhub.toml` des Tools.
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
                    eprintln!("{}: tool.json kaputt: {err}", e.path().display());
                    None
                }
            }
        })
        .collect();
    out.sort_by_key(|a| a.entry.name.to_lowercase());
    out
}

pub fn load(tools_dir: &Path, id: &str) -> Result<Installed, String> {
    if !valid_id(id) {
        return Err(format!("ungültige Tool-ID {id:?}"));
    }
    let s = std::fs::read_to_string(tools_dir.join(id).join("tool.json"))
        .map_err(|_| format!("Tool {id:?} ist nicht installiert"))?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
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
        .map_err(|e| format!("{cmd} nicht gefunden: {e}"))?;
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
            // Benutzer-Installation: braucht kein root und umgeht Fedoras gefiltertes Flathub.
            if remote == "flathub" {
                progress(app, &entry.id, "Flathub einrichten …", None);
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
            progress(
                app,
                &entry.id,
                format!("Installiere {flatpak_id} – das kann dauern …"),
                None,
            );
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
        Source::Local { .. } => Err("Lokale Tools werden verknüpft, nicht installiert".into()),
    }
}

async fn install_github<R: Runtime>(
    app: &AppHandle<R>,
    tools_dir: &Path,
    entry: &Entry,
    repo: &str,
    pattern: &str,
) -> Result<Installed, String> {
    let id = entry.id.as_str();
    progress(app, id, "Suche neuestes Release …", None);
    let rel = github::latest_release(repo).await?;
    let asset = rel
        .assets
        .iter()
        .find(|a| {
            github::glob_match(pattern, &a.name) && safe_relative(&a.name) && !a.name.contains('/')
        })
        .ok_or_else(|| format!("Kein Download in {} passt zu {pattern:?}", rel.tag_name))?
        .clone();

    // In einen Staging-Ordner installieren und erst am Ende austauschen,
    // damit ein abgebrochenes Update die alte Version nicht zerstört.
    let staging = tools_dir.join(format!(".{id}.new"));
    let _ = std::fs::remove_dir_all(&staging);
    let files = staging.join("files");
    std::fs::create_dir_all(&files).map_err(|e| e.to_string())?;

    let download = staging.join(&asset.name);
    let size = asset.size;
    github::download(repo, &asset, &download, |done| {
        let mb = done as f64 / 1_048_576.0;
        progress(
            app,
            id,
            format!("Lade {} ({mb:.1} MB)", asset.name),
            (size > 0).then(|| done as f64 / size as f64),
        );
    })
    .await?;

    progress(app, id, "Entpacke …", None);
    let lower = asset.name.to_lowercase();
    let archive = [".tar.gz", ".tgz", ".tar.xz", ".tar.zst", ".tar.bz2", ".tar"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    if archive {
        run(
            "tar",
            &[
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
        // Einzelne Datei (AppImage oder Binary)
        let target = files.join(&asset.name);
        std::fs::rename(&download, &target).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }

    // Archive mit genau einem Oberordner: den als Wurzel nehmen.
    let mut root = "files".to_string();
    let top: Vec<_> = std::fs::read_dir(&files)
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    if top.len() == 1 && top[0].path().is_dir() {
        root = format!("files/{}", top[0].file_name().to_string_lossy());
    }

    let mut inst = Installed {
        entry: entry.clone(),
        version: Some(rel.tag_name.clone()),
        root: Some(root),
        detected_run: None,
    };
    let root_dir = staging.join(inst.root.as_deref().unwrap());
    // Nur raten, wenn es gar kein Manifest gibt (Stufe 0). Ein Manifest ohne
    // `run` heißt: Das Tool wird ausschließlich über den Hub bedient.
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

/// Sucht im Tool-Ordner etwas Startbares: zuerst ein AppImage, sonst die
/// einzige ausführbare Datei.
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

/// Verknüpft einen lokalen Ordner mit `tuxhub.toml` als Tool (Entwicklermodus).
pub fn link_local(tools_dir: &Path, path: &Path) -> Result<Installed, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let m = Manifest::load(&path)?.ok_or("Im Ordner liegt keine tuxhub.toml")?;
    let id =
        m.id.clone()
            .ok_or("tuxhub.toml braucht für lokale Tools eine `id`")?;
    let entry = Entry {
        name: m.name.clone().unwrap_or_else(|| id.clone()),
        description: m.description.clone().unwrap_or_default(),
        icon: m.icon.clone(),
        category: Some("Lokal".into()),
        repo: None,
        homepage: None,
        source: Source::Local {
            path: path.to_string_lossy().into_owned(),
        },
        manifest: None,
        id,
    };
    entry.validate()?;
    let dir = tools_dir.join(&entry.id);
    if dir.join("tool.json").exists() {
        return Err(format!(
            "Ein Tool mit der ID {:?} ist schon installiert",
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

pub async fn uninstall(tools_dir: &Path, inst: &Installed) -> Result<(), String> {
    if let Source::Flatpak { flatpak_id, .. } = &inst.entry.source {
        run(
            "flatpak",
            &["uninstall", "-y", "--noninteractive", flatpak_id],
        )
        .await?;
    }
    // Bei lokalen Tools wird nur die Verknüpfung entfernt, nie der Ordner selbst.
    std::fs::remove_dir_all(tools_dir.join(inst.id())).map_err(|e| e.to_string())
}

/// Startet das Tool ganz normal als eigenes Programm.
pub fn launch(tools_dir: &Path, inst: &Installed) -> Result<(), String> {
    let mut cmd = match &inst.entry.source {
        Source::Flatpak { flatpak_id, .. } => {
            let mut c = std::process::Command::new("flatpak");
            c.args(["run", flatpak_id]);
            c
        }
        _ => {
            let root = inst.root_dir(tools_dir).ok_or("Kein Tool-Ordner")?;
            let manifest = inst.manifest(tools_dir);
            let (command, args) = match (&manifest.run, &inst.detected_run) {
                (Some(r), _) => (r.command.clone(), r.args.clone()),
                (None, Some(d)) => (d.clone(), vec![]),
                (None, None) => return Err("Dieses Tool hat keinen Startbefehl".into()),
            };
            let exe = root.join(&command);
            if !exe.is_file() {
                return Err(format!("{} fehlt", exe.display()));
            }
            let mut c = std::process::Command::new(exe);
            c.args(args).current_dir(root);
            c
        }
    };
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Start fehlgeschlagen: {e}"))?;
    // Prozess abwarten, damit kein Zombie zurückbleibt.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Liefert das neueste Release-Tag, falls es neuer ist als das installierte.
pub async fn check_update(inst: &Installed) -> Result<Option<String>, String> {
    let (Source::GithubRelease { .. }, Some(repo)) = (&inst.entry.source, &inst.entry.repo) else {
        return Ok(None);
    };
    let rel = github::latest_release(repo).await?;
    Ok((inst.version.as_deref() != Some(rel.tag_name.as_str())).then_some(rel.tag_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lädt das Autoclicker-Release wirklich von GitHub (Netzwerk + Zugriff
    /// auf das Repo nötig): `cargo test -- --ignored`
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
            "Hub-Tool ohne [run] darf keinen Startbefehl raten"
        );
        let m = inst.manifest(&tmp);
        assert_eq!(m.level(), 2);
        let root = inst.root_dir(&tmp).unwrap();
        assert!(root.join(&m.rpc.unwrap().command).is_file());
        // Erneut installieren = Update-Pfad
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
        }
    }
}
