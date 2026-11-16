//! Reads the freedesktop `.desktop` entries of apps installed on this PC.

use crate::appstream::AppInfo;
use crate::tools::{Entry, Installation, PackageFormat, Programs, Source, is_shared_launcher};
use futures_util::StreamExt;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_DEPTH: usize = 8;
/// tuxhub's own start menu entries: the one it writes itself, and the one of
/// its rpm and deb packages.
const OWN_DESKTOP_IDS: [&str; 2] = ["io.github.sirsnolte.tuxhub.desktop", "tuxhub.desktop"];

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopApp {
    pub desktop_id: String,
    pub name: String,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub exec: Vec<String>,
    pub categories: Vec<String>,
    pub flatpak_id: Option<String>,
    pub no_display: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub path: PathBuf,
}

fn split_list(v: &str) -> Vec<String> {
    v.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Locale keys to try for `Name[..]`, best first: `de_DE`, then `de`.
fn locale_keys(lang: &str) -> Vec<String> {
    let base = lang.split(['.', '@']).next().unwrap_or("");
    let code = base.split('_').next().unwrap_or("");
    let mut keys = Vec::new();
    if !base.is_empty() && base != "C" && base != "POSIX" {
        keys.push(base.to_string());
        if code != base && !code.is_empty() {
            keys.push(code.to_string());
        }
    }
    keys
}

/// Parses a `.desktop` file with the language of `$LANG`. `None` for anything
/// that is not a visible-capable application (wrong type, `Hidden`, no name or `Exec`).
/// `desktop_id` is the file ID (`kde-foo.desktop`), `path` where it was read.
pub fn parse_desktop(text: &str, desktop_id: &str, path: &Path) -> Option<DesktopApp> {
    let lang = std::env::var("LANG").unwrap_or_default();
    parse_desktop_for(text, desktop_id, path, &lang)
}

pub fn parse_desktop_for(
    text: &str,
    desktop_id: &str,
    path: &Path,
    lang: &str,
) -> Option<DesktopApp> {
    let keys = locale_keys(lang);
    let mut in_entry = false;
    let (mut name, mut comment) = (None::<String>, None::<String>);
    // Best localized match so far: lower index = better.
    let (mut name_loc, mut comment_loc) = (None::<(usize, String)>, None::<(usize, String)>);
    let (mut icon, mut exec, mut flatpak_id) = (None, None, None);
    let (mut categories, mut only_show_in, mut not_show_in) = (vec![], vec![], vec![]);
    let (mut is_app, mut hidden, mut no_display, mut terminal) = (false, false, false, false);

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "Type" => is_app = v == "Application",
            "Name" => name = Some(v.to_string()),
            "Comment" => comment = Some(v.to_string()),
            "Icon" => icon = Some(v.to_string()),
            "Exec" => exec = Some(v.to_string()),
            "X-Flatpak" => flatpak_id = Some(v.to_string()),
            "Categories" => categories = split_list(v),
            "OnlyShowIn" => only_show_in = split_list(v),
            "NotShowIn" => not_show_in = split_list(v),
            "Hidden" => hidden = v == "true",
            "NoDisplay" => no_display = v == "true",
            "Terminal" => terminal = v == "true",
            _ => {
                let loc = k.strip_suffix(']').and_then(|k| {
                    k.split_once('[')
                        .map(|(f, l)| (f.to_string(), l.to_string()))
                });
                if let Some((field, l)) = loc
                    && let Some(i) = keys.iter().position(|x| *x == l)
                {
                    let slot = match field.as_str() {
                        "Name" => &mut name_loc,
                        "Comment" => &mut comment_loc,
                        _ => continue,
                    };
                    if slot.as_ref().is_none_or(|(j, _)| i < *j) {
                        *slot = Some((i, v.to_string()));
                    }
                }
            }
        }
    }
    if !is_app || hidden {
        return None;
    }
    let name = name_loc
        .map(|(_, v)| v)
        .or(name)
        .filter(|n| !n.is_empty())?;
    let exec = split_exec(&exec?);
    if exec.is_empty() {
        return None;
    }
    // A terminal app is a console app; reuse the category the filter already knows.
    if terminal && !categories.iter().any(|c| c == "ConsoleOnly") {
        categories.push("ConsoleOnly".into());
    }
    Some(DesktopApp {
        desktop_id: desktop_id.to_string(),
        name,
        comment: comment_loc
            .map(|(_, v)| v)
            .or(comment)
            .filter(|c| !c.is_empty()),
        // A name or a file, never a URL: nothing is fetched when the workshop opens.
        icon: icon.filter(|i| !i.is_empty() && !i.contains("://")),
        exec,
        categories,
        flatpak_id: flatpak_id.filter(|f| !f.is_empty()),
        no_display,
        only_show_in,
        not_show_in,
        path: path.to_path_buf(),
    })
}

/// Splits an `Exec=` value per the Desktop Entry spec: double-quote quoting with
/// backslash escapes, field codes removed, `%%` -> `%`, leading `env VAR=..` dropped.
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let (mut started, mut had_code, mut in_quotes) = (false, false, false);
    let mut chars = exec.chars().peekable();
    let mut finish = |cur: &mut String, started: &mut bool, had_code: &mut bool| {
        if *started && !(*had_code && cur.is_empty()) {
            args.push(std::mem::take(cur));
        }
        cur.clear();
        *started = false;
        *had_code = false;
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                started = true;
            }
            '\\' if in_quotes => {
                if let Some(n) = chars.next_if(|n| matches!(n, '"' | '`' | '$' | '\\')) {
                    cur.push(n);
                } else {
                    cur.push('\\');
                }
                started = true;
            }
            '%' => {
                started = true;
                match chars.next() {
                    Some('%') => cur.push('%'),
                    // Includes the deprecated codes, which launchers must drop too.
                    Some(
                        'f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm',
                    ) => had_code = true,
                    Some(o) => {
                        cur.push('%');
                        cur.push(o);
                    }
                    None => cur.push('%'),
                }
            }
            c if c.is_whitespace() && !in_quotes => finish(&mut cur, &mut started, &mut had_code),
            c => {
                cur.push(c);
                started = true;
            }
        }
    }
    finish(&mut cur, &mut started, &mut had_code);
    if args.first().is_some_and(|a| a == "env") {
        args.remove(0);
        while args.first().is_some_and(|a| is_env_assignment(a)) {
            args.remove(0);
        }
    }
    args
}

fn is_env_assignment(a: &str) -> bool {
    a.split_once('=').is_some_and(|(k, _)| {
        !k.is_empty()
            && !k.starts_with(|c: char| c.is_ascii_digit())
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Whether the start menu shows the app. `desktop` is the colon-separated
/// `XDG_CURRENT_DESKTOP` (for example `KDE` or `ubuntu:GNOME`). Settings, system
/// and console apps, Wine's own tools and help screens are hidden unless
/// `show_all`; `NoDisplay` apps, ones not
/// meant for this desktop and tuxhub itself are always hidden.
pub fn is_visible(app: &DesktopApp, desktop: &str, show_all: bool) -> bool {
    if app.no_display || OWN_DESKTOP_IDS.contains(&app.desktop_id.as_str()) {
        return false;
    }
    let current: Vec<&str> = desktop.split(':').filter(|s| !s.is_empty()).collect();
    if !app.only_show_in.is_empty()
        && !app
            .only_show_in
            .iter()
            .any(|d| current.contains(&d.as_str()))
    {
        return false;
    }
    if app
        .not_show_in
        .iter()
        .any(|d| current.contains(&d.as_str()))
    {
        return false;
    }
    if show_all {
        return true;
    }
    let has = |c: &str| app.categories.iter().any(|x| x == c);
    let system_only = has("System")
        && ![
            "Game",
            "AudioVideo",
            "Graphics",
            "Network",
            "Office",
            "Development",
            "Utility",
        ]
        .iter()
        .any(|c| has(c));
    let settings = app.categories.iter().any(|c| {
        c == "Settings" || c.starts_with("X-KDE-settings") || c.starts_with("X-GNOME-Settings")
    });
    // Wine's built-in tools (Notepad, Regedit, ...) and the desktop's help
    // and welcome screens name no main category of their own.
    let main = APP_CATEGORIES.iter().any(|c| has(c));
    let side_tool = !main && (has("X-Wine") || has("Documentation"));
    !(settings || system_only || side_tool || has("ConsoleOnly"))
}

/// Main categories of the Desktop Menu spec an app is filed under, without
/// `Settings` and `System`.
const APP_CATEGORIES: [&str; 11] = [
    "AudioVideo",
    "Audio",
    "Video",
    "Development",
    "Education",
    "Game",
    "Graphics",
    "Network",
    "Office",
    "Science",
    "Utility",
];

pub fn scan_dirs(dirs: &[PathBuf]) -> Vec<DesktopApp> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        scan_one(dir, dir, 0, &mut seen, &mut apps);
    }
    apps
}

fn scan_one(
    root: &Path,
    dir: &Path,
    depth: usize,
    seen: &mut HashSet<String>,
    apps: &mut Vec<DesktopApp>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        // Follows symlinks on purpose: Flatpak exports are symlinks.
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            // Don't follow symlinked directories (loop protection).
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                scan_one(root, &path, depth + 1, seen, apps);
            }
            continue;
        }
        if !meta.is_file() || path.extension().is_none_or(|x| x != "desktop") {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let id = rel.to_string_lossy().replace('/', "-");
        // The first file with an ID shadows later ones, even if it is hidden,
        // too big or unreadable.
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(text) = read_limited(&path) else {
            continue;
        };
        if let Some(app) = parse_desktop(&text, &id, &path) {
            apps.push(app);
        }
    }
}

/// The text of a file, or `None` when it cannot be read, is not UTF-8 or is
/// bigger than `MAX_FILE_BYTES`.
fn read_limited(path: &Path) -> Option<String> {
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    String::from_utf8(buf).ok()
}

pub fn application_dirs() -> Vec<PathBuf> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = env("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let system = env("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(
            system
                .split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        )
        .map(|d| d.join("applications"))
        .collect()
}

/// Where an installed app comes from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LocalSource {
    Flatpak {
        flatpak_id: String,
        installations: Vec<Installation>,
    },
    Rpm {
        package: String,
    },
    Deb {
        package: String,
    },
    Appimage {
        path: String,
    },
    Unknown {
        program: String,
    },
}

/// An app of the start menu, with its source resolved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalApp {
    pub desktop_id: String,
    pub name: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub homepage: Option<String>,
    pub repo: Option<String>,
    pub categories: Vec<String>,
    pub source: LocalSource,
    pub command: Vec<String>,
    pub autostart: bool,
}

/// Cached list: newest application folder time, `show_all`, apps.
pub type Cache = Option<(SystemTime, bool, Vec<LocalApp>)>;

/// Finds where an app comes from: Flatpak by its ID, otherwise the package
/// that owns its program, an AppImage by file name, or nothing known.
pub async fn resolve_source(app: &DesktopApp, programs: &Programs) -> LocalSource {
    if let Some(id) = &app.flatpak_id {
        return flatpak_source(id, &app.path, programs).await;
    }
    let program = app.exec.first().cloned().unwrap_or_default();
    let file = find_program(&program);
    // A shell, interpreter or launcher belongs to its own package, not to the app.
    if file.as_deref().map_or_else(
        || is_shared_launcher(Path::new(&program)),
        is_shared_launcher,
    ) {
        return LocalSource::Unknown { program };
    }
    if let Some(file) = file {
        let mut candidates = vec![file.clone()];
        if let Ok(real) = file.canonicalize()
            && real != file
        {
            candidates.push(real);
        }
        for path in candidates {
            if let Some(package) = rpm_owner(programs, &path).await {
                return LocalSource::Rpm { package };
            }
            if let Some(package) = deb_owner(programs, &path).await {
                return LocalSource::Deb { package };
            }
        }
    }
    if program.to_ascii_lowercase().ends_with(".appimage") {
        return LocalSource::Appimage { path: program };
    }
    LocalSource::Unknown { program }
}

async fn flatpak_source(id: &str, path: &Path, programs: &Programs) -> LocalSource {
    // The folder the file was found in tells one installation; ask for the other.
    let known = if path.starts_with("/var/lib/flatpak/exports") {
        Some(Installation::System)
    } else if path.to_string_lossy().contains("/flatpak/exports/") {
        Some(Installation::User)
    } else {
        None
    };
    let (system, user) = futures_util::future::join(
        async {
            known == Some(Installation::System)
                || programs.flatpak_installed(id, Installation::System).await
        },
        async {
            known == Some(Installation::User)
                || programs.flatpak_installed(id, Installation::User).await
        },
    )
    .await;
    let mut installations = Vec::new();
    if system {
        installations.push(Installation::System);
    }
    if user {
        installations.push(Installation::User);
    }
    if installations.is_empty() {
        installations.push(Installation::User);
    }
    LocalSource::Flatpak {
        flatpak_id: id.to_string(),
        installations,
    }
}

/// The file a program name stands for: a path as it is, a name looked up in `PATH`.
fn find_program(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let p = PathBuf::from(program);
        return p.is_file().then_some(p);
    }
    if program.is_empty() {
        return None;
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(program))
        .find(|p| p.is_file())
}

fn valid_package(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '_' | '-'))
}

async fn rpm_owner(programs: &Programs, file: &Path) -> Option<String> {
    let file = file.to_str()?;
    let out = programs
        .query(&programs.rpm, &["-qf", "--qf", "%{NAME}", file])
        .await?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let name = text.lines().next()?.trim();
    valid_package(name).then(|| name.to_string())
}

async fn deb_owner(programs: &Programs, file: &Path) -> Option<String> {
    let file = file.to_str()?;
    let out = programs.query(&programs.dpkg, &["-S", file]).await?;
    if !out.status.success() {
        return None;
    }
    // `package[:arch][, other]: /path`
    let text = String::from_utf8_lossy(&out.stdout);
    let (names, _) = text.lines().next()?.split_once(": ")?;
    let name = names.split(',').next()?.split(':').next()?.trim();
    valid_package(name).then(|| name.to_string())
}

/// Whether an autostart entry of `autostart_dir` starts this app: the same
/// Flatpak, or (for non-Flatpak apps) the same program.
pub fn autostart_matches(app: &DesktopApp, autostart_dir: &Path) -> bool {
    autostart_starts(
        app.flatpak_id.as_deref(),
        app.exec.first().map(String::as_str),
        autostart_dir,
    )
}

/// Whether an autostart entry of `autostart_dir` starts the Flatpak with
/// this ID or, without one, this program.
pub fn autostart_starts(
    flatpak_id: Option<&str>,
    program: Option<&str>,
    autostart_dir: &Path,
) -> bool {
    let Ok(rd) = std::fs::read_dir(autostart_dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "desktop") {
            return false;
        }
        let Some(text) = read_limited(&path) else {
            return false;
        };
        let Some(other) = parse_desktop(&text, "autostart.desktop", &path) else {
            return false;
        };
        match (flatpak_id, &other.flatpak_id) {
            (Some(a), Some(b)) => a == b,
            // Every Flatpak starts with `flatpak run`: the program says nothing.
            (Some(_), None) | (None, Some(_)) => false,
            (None, None) => {
                program.is_some_and(|p| other.exec.first().map(String::as_str) == Some(p))
            }
        }
    })
}

/// Position of the catalog entry that stands for this app, found by Flatpak
/// ID or package.
pub fn catalog_entry(catalog: &[Entry], source: &LocalSource) -> Option<usize> {
    catalog.iter().position(|e| match (&e.source, source) {
        (Source::Flatpak { flatpak_id, .. }, LocalSource::Flatpak { flatpak_id: id, .. }) => {
            flatpak_id == id
        }
        (
            Source::SystemPackage { package, .. },
            LocalSource::Rpm { package: p } | LocalSource::Deb { package: p },
        ) => package == p,
        // The same name in the other package format is another package.
        (
            Source::DistroPackage {
                package,
                manager: PackageFormat::Rpm,
                ..
            },
            LocalSource::Rpm { package: p },
        )
        | (
            Source::DistroPackage {
                package,
                manager: PackageFormat::Deb,
                ..
            },
            LocalSource::Deb { package: p },
        ) => package == p,
        _ => false,
    })
}

const LOCAL_PREFIX: &str = "local.";

/// The tool id of an app added from this computer: `local.` and its desktop
/// file name, lowercase, other characters as `-`, at most 64 characters.
pub(crate) fn local_id(desktop_id: &str) -> String {
    let stem = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    let mut id: String = stem
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    if id.is_empty() {
        id.push_str("app");
    }
    id.insert_str(0, LOCAL_PREFIX);
    while id.contains("..") {
        id = id.replace("..", ".");
    }
    // Only ASCII is left, so any length is a character boundary.
    id.truncate(64);
    id
}

/// The start command with its program by absolute path, as far as it can be
/// found; a program that cannot be found stays as it is.
fn absolute_command(command: &[String]) -> Vec<String> {
    let mut command = command.to_vec();
    if let Some(program) = command.first_mut()
        && !program.starts_with('/')
        && let Some(path) = find_program(program)
    {
        *program = path.to_string_lossy().into_owned();
    }
    command
}

/// The entry an app of this computer gets in tuxhub: a Flatpak stays a
/// Flatpak, a distribution package is updated by its package manager, and
/// anything else is only started. It has no manifest.
pub fn local_entry(app: &LocalApp) -> Entry {
    let command = absolute_command(&app.command);
    let source = match &app.source {
        LocalSource::Flatpak { flatpak_id, .. } => Source::Flatpak {
            flatpak_id: flatpak_id.clone(),
            remote: "flathub".into(),
        },
        LocalSource::Rpm { package } => Source::DistroPackage {
            package: package.clone(),
            manager: PackageFormat::Rpm,
            command,
        },
        LocalSource::Deb { package } => Source::DistroPackage {
            package: package.clone(),
            manager: PackageFormat::Deb,
            command,
        },
        LocalSource::Appimage { .. } | LocalSource::Unknown { .. } => Source::Launcher {
            desktop_id: app.desktop_id.clone(),
            command,
        },
    };
    Entry {
        id: local_id(&app.desktop_id),
        name: app.name.clone(),
        description: app.summary.clone().unwrap_or_default(),
        icon: app.icon.clone(),
        category: None,
        repo: app.repo.clone(),
        homepage: app.homepage.clone(),
        source,
        manifest: None,
        integration: None,
        translations: Default::default(),
    }
}

/// Whether the desktop file is named after the Flatpak ID or package.
fn named_after_source(app: &LocalApp) -> bool {
    let id = app
        .desktop_id
        .strip_suffix(".desktop")
        .unwrap_or(&app.desktop_id);
    match &app.source {
        LocalSource::Flatpak { flatpak_id, .. } => id == flatpak_id,
        LocalSource::Rpm { package } | LocalSource::Deb { package } => id == package,
        _ => false,
    }
}

/// Gives apps the catalog knows (by Flatpak ID or package) the catalog's name,
/// texts, icon and repo, keeping their `desktop_id`. A catalog app shows as one
/// entry: when several desktop files belong to it, the one named after the
/// Flatpak ID or package stays, otherwise the first by name. Sorted by name.
fn apply_catalog(mut apps: Vec<LocalApp>, catalog: &[Entry]) -> Vec<LocalApp> {
    let mut matched: Vec<Option<usize>> = Vec::with_capacity(apps.len());
    for app in &mut apps {
        let found = catalog_entry(catalog, &app.source);
        if let Some(i) = found {
            let e = &catalog[i];
            app.name = e.name.clone();
            if !e.description.is_empty() {
                app.summary = Some(e.description.clone());
            }
            if e.icon.is_some() {
                app.icon = e.icon.clone();
            }
            if e.repo.is_some() {
                app.repo = e.repo.clone();
            }
            if e.homepage.is_some() {
                app.homepage = e.homepage.clone();
            }
        }
        matched.push(found);
    }
    let mut rows: Vec<(LocalApp, Option<usize>)> = apps.into_iter().zip(matched).collect();
    rows.sort_by_cached_key(|(a, _)| (a.name.to_lowercase(), a.desktop_id.clone()));
    let mut chosen: HashMap<usize, usize> = HashMap::new();
    for (pos, (app, entry)) in rows.iter().enumerate() {
        if let Some(entry) = *entry {
            match chosen.get(&entry) {
                None => {
                    chosen.insert(entry, pos);
                }
                Some(&kept) if !named_after_source(&rows[kept].0) && named_after_source(app) => {
                    chosen.insert(entry, pos);
                }
                Some(_) => {}
            }
        }
    }
    rows.into_iter()
        .enumerate()
        .filter(|(pos, (_, entry))| entry.is_none_or(|e| chosen.get(&e) == Some(pos)))
        .map(|(_, (app, _))| app)
        .collect()
}

async fn build_app(
    app: DesktopApp,
    autostart_dir: &Path,
    index: &HashMap<String, AppInfo>,
    programs: &Programs,
) -> LocalApp {
    let source = resolve_source(&app, programs).await;
    let id = app.desktop_id.as_str();
    let info = app
        .flatpak_id
        .as_deref()
        .and_then(|f| index.get(f))
        .or_else(|| index.get(id))
        .or_else(|| index.get(id.strip_suffix(".desktop").unwrap_or(id)));
    let autostart = autostart_matches(&app, autostart_dir);
    LocalApp {
        desktop_id: app.desktop_id,
        name: app.name,
        summary: app.comment.or_else(|| info.and_then(|i| i.summary.clone())),
        description: info.and_then(|i| i.description.clone()),
        icon: app.icon.or_else(|| info.and_then(|i| i.icon.clone())),
        homepage: info.and_then(|i| i.homepage.clone()),
        repo: info.and_then(|i| crate::appstream::github_repo(&i.urls)),
        // `ConsoleOnly` is only the marker for `Terminal=true`, not a category.
        categories: app
            .categories
            .into_iter()
            .filter(|c| c != "ConsoleOnly")
            .collect(),
        source,
        command: app.exec,
        autostart,
    }
}

/// How many apps resolve their source at once: each needs a few short
/// processes, and too many at once run into the timeout.
const RESOLVE_CONCURRENCY: usize = 16;

/// The apps of the start menu with their source, without the catalog.
async fn scan_apps(
    dirs: &[PathBuf],
    desktop: &str,
    autostart_dir: &Path,
    index: &HashMap<String, AppInfo>,
    programs: &Programs,
    show_all: bool,
) -> Vec<LocalApp> {
    let visible: Vec<DesktopApp> = scan_dirs(dirs)
        .into_iter()
        .filter(|a| is_visible(a, desktop, show_all))
        .collect();
    futures_util::stream::iter(
        visible
            .into_iter()
            .map(|a| build_app(a, autostart_dir, index, programs)),
    )
    .buffered(RESOLVE_CONCURRENCY)
    .collect()
    .await
}

/// [`list_cached`] for given folders, without a cache.
#[cfg(test)]
async fn list_in(
    dirs: &[PathBuf],
    desktop: &str,
    autostart_dir: &Path,
    index: &HashMap<String, AppInfo>,
    programs: &Programs,
    catalog: &[Entry],
    show_all: bool,
) -> Vec<LocalApp> {
    let apps = scan_apps(dirs, desktop, autostart_dir, index, programs, show_all).await;
    apply_catalog(apps, catalog)
}

fn current_desktop() -> String {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    desktop.split(':').next().unwrap_or("").to_string()
}

pub fn autostart_dir() -> PathBuf {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default()
        .join("autostart")
}

/// Newest change time of the application folders: when it moves, apps were
/// installed or removed.
fn newest_mtime(dirs: &[PathBuf]) -> SystemTime {
    dirs.iter()
        .filter_map(|d| std::fs::metadata(d).and_then(|m| m.modified()).ok())
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Every app of the start menu with its source, sorted by name. Apps the
/// catalog knows (by Flatpak ID or package) take their texts, icon and repo
/// from the catalog and show once; their `desktop_id` stays.
///
/// The scanned apps stay in `cache` until an application folder changes (or
/// `show_all` does). The catalog is not part of the cache: it is applied on
/// every call.
pub async fn list_cached(
    cache: &std::sync::Mutex<Cache>,
    programs: &Programs,
    catalog: &[Entry],
    show_all: bool,
) -> Vec<LocalApp> {
    let apps = cached_apps(
        cache,
        &application_dirs(),
        &current_desktop(),
        &autostart_dir(),
        &crate::appstream::load_index,
        programs,
        show_all,
    )
    .await;
    apply_catalog(apps, catalog)
}

async fn cached_apps(
    cache: &std::sync::Mutex<Cache>,
    dirs: &[PathBuf],
    desktop: &str,
    autostart_dir: &Path,
    load_index: &(dyn Fn() -> HashMap<String, AppInfo> + Sync),
    programs: &Programs,
    show_all: bool,
) -> Vec<LocalApp> {
    let stamp = newest_mtime(dirs);
    if let Ok(guard) = cache.lock()
        && let Some((t, all, apps)) = guard.as_ref()
        && *t == stamp
        && *all == show_all
    {
        return apps.clone();
    }
    let apps = scan_apps(
        dirs,
        desktop,
        autostart_dir,
        &load_index(),
        programs,
        show_all,
    )
    .await;
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((stamp, show_all, apps.clone()));
    }
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLATPAK: &str = "[Desktop Entry]\nType=Application\nName=StreamController\n\
X-Flatpak=com.core447.StreamController\n\
Exec=/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=launch.sh com.core447.StreamController %U\n";

    fn app(categories: &[&str]) -> DesktopApp {
        DesktopApp {
            desktop_id: "x.desktop".into(),
            name: "X".into(),
            comment: None,
            icon: None,
            exec: vec!["x".into()],
            categories: categories.iter().map(|s| s.to_string()).collect(),
            flatpak_id: None,
            no_display: false,
            only_show_in: vec![],
            not_show_in: vec![],
            path: PathBuf::from("/x.desktop"),
        }
    }

    #[test]
    fn parses_flatpak_export() {
        let a = parse_desktop_for(FLATPAK, "a.desktop", Path::new("/a.desktop"), "en_US").unwrap();
        assert_eq!(
            a.flatpak_id.as_deref(),
            Some("com.core447.StreamController")
        );
        assert_eq!(a.exec.last().unwrap(), "com.core447.StreamController");
        assert!(!a.exec.iter().any(|p| p.contains("%U")));
    }

    #[test]
    fn split_exec_quotes_and_codes() {
        assert_eq!(
            split_exec("\"/opt/My App/run\" --x %F 100%%"),
            vec!["/opt/My App/run", "--x", "100%"]
        );
        assert_eq!(split_exec("env GDK_BACKEND=x11 foo %u"), vec!["foo"]);
        // Deprecated field codes are dropped like %f; unknown ones stay literal.
        assert_eq!(split_exec("foo %d %D %n %N %v %m bar"), vec!["foo", "bar"]);
        assert_eq!(split_exec("foo %x"), vec!["foo", "%x"]);
    }

    #[test]
    fn skips_hidden_and_non_apps() {
        let p = Path::new("/a.desktop");
        let hidden = "[Desktop Entry]\nType=Application\nName=A\nExec=a\nHidden=true\n";
        let link = "[Desktop Entry]\nType=Link\nName=A\nURL=http://x\n";
        assert!(parse_desktop_for(hidden, "a.desktop", p, "en").is_none());
        assert!(parse_desktop_for(link, "a.desktop", p, "en").is_none());
    }

    /// A desktop entry names its icon or a file; a URL would be fetched when
    /// the workshop shows the app.
    #[test]
    fn never_takes_a_remote_desktop_icon() {
        let p = Path::new("/a.desktop");
        let entry = |icon: &str| {
            let t = format!("[Desktop Entry]\nType=Application\nName=A\nExec=a\nIcon={icon}\n");
            parse_desktop_for(&t, "a.desktop", p, "en").unwrap().icon
        };
        assert_eq!(entry("org.example.A").as_deref(), Some("org.example.A"));
        assert_eq!(entry("/opt/a/a.png").as_deref(), Some("/opt/a/a.png"));
        assert_eq!(entry("https://example.com/a.png"), None);
        assert_eq!(entry("file:///opt/a/a.png"), None);
    }

    #[test]
    fn localized_name() {
        let t = "[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\nExec=files\n";
        let p = Path::new("/a.desktop");
        let a = parse_desktop_for(t, "a.desktop", p, "de_DE.UTF-8").unwrap();
        assert_eq!(a.name, "Dateien");
        let a = parse_desktop_for(t, "a.desktop", p, "en_US.UTF-8").unwrap();
        assert_eq!(a.name, "Files");
    }

    #[test]
    fn visibility_filter() {
        let mut a = app(&["Game"]);
        a.no_display = true;
        assert!(!is_visible(&a, "KDE", true));
        let mut a = app(&["Game"]);
        a.not_show_in = vec!["KDE".into()];
        assert!(!is_visible(&a, "KDE", false));
        let a = app(&["Settings", "X-KDE-settings-hardware"]);
        assert!(!is_visible(&a, "KDE", false));
        assert!(is_visible(&a, "KDE", true));
        assert!(is_visible(&app(&["Game"]), "KDE", false));
        let mut a = app(&["Game"]);
        a.desktop_id = "io.github.sirsnolte.tuxhub.desktop".into();
        assert!(!is_visible(&a, "KDE", true));
    }

    #[test]
    fn hides_own_package_wine_tools_and_help() {
        // tuxhub's own package installs `tuxhub.desktop`.
        let mut a = app(&["Game"]);
        a.desktop_id = "tuxhub.desktop".into();
        assert!(!is_visible(&a, "KDE", true));
        // Wine's built-in tools (Notepad, Regedit, ...) carry only X-Wine.
        let a = app(&["X-Wine"]);
        assert!(!is_visible(&a, "KDE", false));
        assert!(is_visible(&a, "KDE", true));
        assert!(is_visible(
            &app(&["Game", "LogicGame", "X-Wine"]),
            "KDE",
            false
        ));
        // A Windows program Wine made a shortcut for has no categories.
        assert!(is_visible(&app(&[]), "KDE", false));
        // Help and welcome screens of the desktop.
        assert!(!is_visible(
            &app(&["Qt", "KDE", "Core", "Documentation"]),
            "KDE",
            false
        ));
        assert!(!is_visible(
            &app(&["Qt", "KDE", "Documentation"]),
            "KDE",
            false
        ));
        assert!(is_visible(
            &app(&["Development", "Documentation"]),
            "KDE",
            false
        ));
    }

    #[test]
    fn scan_first_wins() {
        let base = std::env::temp_dir().join(format!("tuxhub-localapps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (d1, d2) = (base.join("one"), base.join("two"));
        std::fs::create_dir_all(d1.clone()).unwrap();
        std::fs::create_dir_all(d2.join("kde")).unwrap();
        let entry = |n: &str| format!("[Desktop Entry]\nType=Application\nName={n}\nExec=x\n");
        std::fs::write(d1.join("a.desktop"), entry("First")).unwrap();
        std::fs::write(d2.join("a.desktop"), entry("Second")).unwrap();
        std::fs::write(d2.join("kde/b.desktop"), entry("B")).unwrap();
        let apps = scan_dirs(&[d1, d2]);
        let mut ids: Vec<_> = apps.iter().map(|a| a.desktop_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["a.desktop", "kde-b.desktop"]);
        let a = apps.iter().find(|a| a.desktop_id == "a.desktop").unwrap();
        assert_eq!(a.name, "First");
        let _ = std::fs::remove_dir_all(&base);
    }

    // --- source resolution ---

    use crate::tools::Fakes;

    fn exec_app(exec: &str) -> DesktopApp {
        let mut a = app(&[]);
        a.exec = vec![exec.into()];
        a
    }

    /// An executable file the fake package managers can be asked about.
    fn program_file(f: &Fakes, name: &str) -> String {
        let p = f.dir.join("bin").join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        p.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn resolves_flatpak_both_installations() {
        let f = Fakes::new("local-flatpak-both");
        f.flatpak_with(&[Installation::User, Installation::System]);
        let mut a = app(&[]);
        a.flatpak_id = Some("com.core447.StreamController".into());
        a.path = PathBuf::from("/var/lib/flatpak/exports/share/applications/x.desktop");
        assert_eq!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Flatpak {
                flatpak_id: "com.core447.StreamController".into(),
                installations: vec![Installation::System, Installation::User],
            }
        );
    }

    #[tokio::test]
    async fn resolves_flatpak_installation_from_path() {
        let f = Fakes::new("local-flatpak-user");
        f.flatpak_with(&[Installation::User]);
        let mut a = app(&[]);
        a.flatpak_id = Some("org.x.App".into());
        a.path = PathBuf::from("/home/x/.local/share/flatpak/exports/share/applications/x.desktop");
        let LocalSource::Flatpak { installations, .. } = resolve_source(&a, &f.programs).await
        else {
            panic!("not a flatpak")
        };
        assert_eq!(installations, vec![Installation::User]);
    }

    #[tokio::test]
    async fn resolves_rpm_package() {
        let f = Fakes::new("local-rpm");
        f.set("rpm", "echo dolphin");
        let a = exec_app(&program_file(&f, "dolphin"));
        assert_eq!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Rpm {
                package: "dolphin".into()
            }
        );
        assert!(
            f.calls()
                .iter()
                .any(|c| c.starts_with("rpm -qf --qf %{NAME} "))
        );
    }

    #[tokio::test]
    async fn resolves_deb_package() {
        let f = Fakes::new("local-deb");
        f.set("rpm", "exit 1");
        f.set("dpkg", "echo 'konsole:amd64: /usr/bin/konsole'");
        let a = exec_app(&program_file(&f, "konsole"));
        assert_eq!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Deb {
                package: "konsole".into()
            }
        );
    }

    #[tokio::test]
    async fn resolves_appimage_and_unknown() {
        let f = Fakes::new("local-appimage");
        let a = exec_app("/home/x/Apps/Tool.AppImage");
        assert_eq!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Appimage {
                path: "/home/x/Apps/Tool.AppImage".into()
            }
        );
        let a = exec_app("/home/x/Apps/tool.appimage");
        assert!(matches!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Appimage { .. }
        ));
        f.set("rpm", "echo 'file is not owned by any package'; exit 1");
        f.set("dpkg", "exit 1");
        let file = program_file(&f, "homemade");
        let a = exec_app(&file);
        assert_eq!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Unknown { program: file }
        );
    }

    #[tokio::test]
    async fn package_names_are_validated() {
        let f = Fakes::new("local-bad-package");
        f.set("rpm", "echo '--evil; rm'");
        f.set("dpkg", "exit 1");
        let file = program_file(&f, "x");
        let a = exec_app(&file);
        assert!(matches!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Unknown { .. }
        ));
    }

    fn entry(json: &str) -> Entry {
        serde_json::from_str(json).unwrap()
    }

    #[tokio::test]
    async fn catalog_entry_wins() {
        let f = Fakes::new("local-catalog");
        f.flatpak_with(&[Installation::User]);
        let dir = f.dir.join("apps");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sc.desktop"), FLATPAK).unwrap();
        std::fs::write(
            dir.join("other.desktop"),
            "[Desktop Entry]\nType=Application\nName=Zed\nExec=zed\n",
        )
        .unwrap();
        let catalog = [entry(
            r#"{"id":"streamcontroller","name":"Stream Controller","description":"Deck control",
            "repo":"StreamController/StreamController",
            "source":{"type":"flatpak","flatpak_id":"com.core447.StreamController"}}"#,
        )];
        let apps = list_in(
            &[dir],
            "KDE",
            &f.dir.join("no-autostart"),
            &HashMap::new(),
            &f.programs,
            &catalog,
            true,
        )
        .await;
        let sc: Vec<_> = apps
            .iter()
            .filter(|a| a.desktop_id == "sc.desktop")
            .collect();
        assert_eq!(sc.len(), 1);
        assert_eq!(sc[0].name, "Stream Controller");
        assert_eq!(sc[0].summary.as_deref(), Some("Deck control"));
        assert_eq!(
            sc[0].repo.as_deref(),
            Some("StreamController/StreamController")
        );
        // sorted by name, case-insensitive
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Stream Controller", "Zed"]);
    }

    #[tokio::test]
    async fn list_hides_console_only_marker_and_filters() {
        let f = Fakes::new("local-list");
        let dir = f.dir.join("apps");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("t.desktop"),
            "[Desktop Entry]\nType=Application\nName=Term\nExec=t\nTerminal=true\nCategories=Game;\n",
        )
        .unwrap();
        let dirs = [dir];
        let (none, index) = (f.dir.join("none"), HashMap::new());
        let run = |show_all| list_in(&dirs, "KDE", &none, &index, &f.programs, &[], show_all);
        assert!(run(false).await.is_empty());
        let apps = run(true).await;
        assert_eq!(apps[0].categories, ["Game"]);
    }

    #[test]
    fn detects_autostart() {
        let dir = std::env::temp_dir().join(format!("tuxhub-autostart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("sc.desktop"),
            "[Desktop Entry]\nType=Application\nName=SC\nExec=flatpak run com.core447.StreamController\nX-Flatpak=com.core447.StreamController\n",
        )
        .unwrap();
        let mut a = app(&[]);
        a.flatpak_id = Some("com.core447.StreamController".into());
        a.exec = vec!["/usr/bin/flatpak".into()];
        assert!(autostart_matches(&a, &dir));
        // Another flatpak shares the `flatpak` program but is not the same app.
        a.flatpak_id = Some("org.other.App".into());
        assert!(!autostart_matches(&a, &dir));
        // Plain apps match by their program.
        std::fs::write(
            dir.join("plain.desktop"),
            "[Desktop Entry]\nType=Application\nName=P\nExec=/opt/plain/run --tray\n",
        )
        .unwrap();
        let mut p = app(&[]);
        p.exec = vec!["/opt/plain/run".into()];
        assert!(autostart_matches(&p, &dir));
        p.exec = vec!["/opt/other/run".into()];
        assert!(!autostart_matches(&p, &dir));
        assert!(!autostart_matches(&p, &dir.join("missing")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_skips_oversized_but_still_shadows() {
        let base = std::env::temp_dir().join(format!("tuxhub-oversize-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (d1, d2) = (base.join("one"), base.join("two"));
        std::fs::create_dir_all(&d1).unwrap();
        std::fs::create_dir_all(&d2).unwrap();
        let mut big = String::from("[Desktop Entry]\nType=Application\nName=Big\nExec=x\n#");
        big.push_str(&"x".repeat(64 * 1024));
        std::fs::write(d1.join("a.desktop"), big).unwrap();
        std::fs::write(
            d2.join("a.desktop"),
            "[Desktop Entry]\nType=Application\nName=Small\nExec=x\n",
        )
        .unwrap();
        assert!(scan_dirs(&[d1, d2]).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    fn desktop_file(dir: &Path, file: &str, name: &str, exec: &str) {
        std::fs::write(
            dir.join(file),
            format!("[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\n"),
        )
        .unwrap();
    }

    fn set_mtime(dir: &Path, secs: u64) {
        std::fs::File::open(dir)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    }

    #[tokio::test]
    async fn cache_keeps_scan_until_folder_changes_and_catalog_stays_fresh() {
        let f = Fakes::new("local-cache");
        let dir = f.dir.join("apps");
        std::fs::create_dir_all(&dir).unwrap();
        desktop_file(&dir, "a.desktop", "Alpha", "a");
        set_mtime(&dir, 1000);
        let dirs = [dir.clone()];
        let cache = std::sync::Mutex::new(None);
        let none = f.dir.join("none");
        let scan = || {
            cached_apps(
                &cache,
                &dirs,
                "KDE",
                &none,
                &HashMap::new,
                &f.programs,
                true,
            )
        };
        let first = scan().await;
        assert_eq!(first.len(), 1);

        // A new file without a changed folder time is not seen: the cache hit.
        desktop_file(&dir, "b.desktop", "Beta", "b");
        set_mtime(&dir, 1000);
        assert_eq!(scan().await.len(), 1);
        // A changed folder time scans again.
        set_mtime(&dir, 2000);
        assert_eq!(scan().await.len(), 2);

        // The catalog applies to the cached scan on every call.
        let flatpak_dir = f.dir.join("fp");
        std::fs::create_dir_all(&flatpak_dir).unwrap();
        std::fs::write(flatpak_dir.join("sc.desktop"), FLATPAK).unwrap();
        f.flatpak_with(&[Installation::User]);
        let dirs = [flatpak_dir];
        let cache = std::sync::Mutex::new(None);
        let scan = || {
            cached_apps(
                &cache,
                &dirs,
                "KDE",
                &none,
                &HashMap::new,
                &f.programs,
                true,
            )
        };
        let apps = scan().await;
        assert_eq!(apply_catalog(apps, &[])[0].name, "StreamController");
        let catalog = [entry(
            r#"{"id":"sc","name":"Deck","source":{"type":"flatpak","flatpak_id":"com.core447.StreamController"}}"#,
        )];
        let apps = scan().await;
        assert_eq!(apply_catalog(apps, &catalog)[0].name, "Deck");
    }

    #[tokio::test]
    async fn catalog_app_shows_once() {
        let f = Fakes::new("local-catalog-once");
        f.set("rpm", "echo foo");
        let dir = f.dir.join("apps");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = program_file(&f, "foo");
        desktop_file(&dir, "a-helper.desktop", "Foo Helper", &exe);
        desktop_file(&dir, "foo.desktop", "Foo", &exe);
        desktop_file(&dir, "z-other.desktop", "Foo Other", &exe);
        let catalog = [entry(
            r#"{"id":"foo","name":"Foo App","source":{"type":"system-package","package":"foo","command":"/usr/bin/foo"}}"#,
        )];
        let apps = list_in(
            std::slice::from_ref(&dir),
            "KDE",
            &f.dir.join("none"),
            &HashMap::new(),
            &f.programs,
            &catalog,
            true,
        )
        .await;
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].desktop_id, "foo.desktop");
        assert_eq!(apps[0].name, "Foo App");

        // Without a file named after the package, the first by name stays.
        std::fs::remove_file(dir.join("foo.desktop")).unwrap();
        let apps = list_in(
            &[dir],
            "KDE",
            &f.dir.join("none"),
            &HashMap::new(),
            &f.programs,
            &catalog,
            true,
        )
        .await;
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].desktop_id, "a-helper.desktop");
    }

    #[tokio::test]
    async fn many_apps_resolve_with_bounded_concurrency() {
        let f = Fakes::new("local-bounded");
        f.set("rpm", "echo pkg");
        let dir = f.dir.join("apps");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = program_file(&f, "p");
        for i in 0..60 {
            desktop_file(&dir, &format!("app{i}.desktop"), &format!("App {i}"), &exe);
        }
        let apps = list_in(
            &[dir],
            "KDE",
            &f.dir.join("none"),
            &HashMap::new(),
            &f.programs,
            &[],
            true,
        )
        .await;
        assert_eq!(apps.len(), 60);
        assert!(apps.iter().all(|a| a.source
            == LocalSource::Rpm {
                package: "pkg".into()
            }));
    }

    fn local_app(desktop_id: &str, source: LocalSource, command: &[&str]) -> LocalApp {
        LocalApp {
            desktop_id: desktop_id.into(),
            name: "Some App".into(),
            summary: Some("Does things".into()),
            description: Some("Long text".into()),
            icon: Some("some-app".into()),
            homepage: Some("https://example.org".into()),
            repo: Some("owner/some-app".into()),
            categories: vec!["Utility".into()],
            source,
            command: command.iter().map(|s| s.to_string()).collect(),
            autostart: false,
        }
    }

    fn unknown(program: &str) -> LocalSource {
        LocalSource::Unknown {
            program: program.into(),
        }
    }

    #[test]
    fn local_entry_id_is_valid() {
        let e = local_entry(&local_app("Some App+Pro.desktop", unknown("/a"), &["/a"]));
        assert_eq!(e.id, "local.some-app-pro");
        assert!(crate::manifest::valid_id(&e.id));
        let long = format!("{}.desktop", "N".repeat(80));
        let e = local_entry(&local_app(&long, unknown("/a"), &["/a"]));
        assert!(e.id.len() <= 64, "{}", e.id);
        assert!(crate::manifest::valid_id(&e.id));
        for odd in [
            "a..b.desktop",
            ".hidden.desktop",
            "Ünïcode Äpp.desktop",
            "kde-org.kde.dolphin.desktop",
            ".desktop",
        ] {
            let e = local_entry(&local_app(odd, unknown("/a"), &["/a"]));
            assert!(crate::manifest::valid_id(&e.id), "{odd} -> {}", e.id);
            assert!(e.id.starts_with("local."), "{}", e.id);
        }
        assert_eq!(
            local_entry(&local_app(
                "kde-org.kde.dolphin.desktop",
                unknown("/a"),
                &["/a"]
            ))
            .id,
            "local.kde-org.kde.dolphin"
        );
    }

    #[test]
    fn local_entry_follows_the_source() {
        let flatpak = LocalSource::Flatpak {
            flatpak_id: "org.x.App".into(),
            installations: vec![Installation::User],
        };
        let e = local_entry(&local_app(
            "org.x.App.desktop",
            flatpak,
            &["/usr/bin/flatpak", "run", "org.x.App"],
        ));
        assert!(matches!(&e.source, Source::Flatpak { flatpak_id, remote }
            if flatpak_id == "org.x.App" && remote == "flathub"));
        assert_eq!(e.name, "Some App");
        assert_eq!(e.description, "Does things");
        assert_eq!(e.icon.as_deref(), Some("some-app"));
        assert_eq!(e.repo.as_deref(), Some("owner/some-app"));
        assert_eq!(e.homepage.as_deref(), Some("https://example.org"));
        assert!(e.manifest.is_none());
        assert!(e.validate().is_ok());

        let rpm = LocalSource::Rpm {
            package: "clock".into(),
        };
        let e = local_entry(&local_app("clock.desktop", rpm, &["/usr/bin/clock", "--x"]));
        let Source::DistroPackage {
            package,
            manager,
            command,
        } = &e.source
        else {
            panic!("{:?}", e.source)
        };
        assert_eq!((package.as_str(), *manager), ("clock", PackageFormat::Rpm));
        assert_eq!(command, &["/usr/bin/clock", "--x"]);
        let deb = LocalSource::Deb {
            package: "clock".into(),
        };
        let e = local_entry(&local_app("clock.desktop", deb, &["/usr/bin/clock"]));
        assert!(matches!(
            e.source,
            Source::DistroPackage {
                manager: PackageFormat::Deb,
                ..
            }
        ));

        for source in [
            LocalSource::Appimage {
                path: "/opt/a.AppImage".into(),
            },
            unknown("/opt/a/run"),
        ] {
            let e = local_entry(&local_app("a.desktop", source, &["/opt/a/run", "-v"]));
            let Source::Launcher {
                desktop_id,
                command,
            } = &e.source
            else {
                panic!("{:?}", e.source)
            };
            assert_eq!(desktop_id, "a.desktop");
            assert_eq!(command, &["/opt/a/run", "-v"]);
        }
    }

    /// The stored command names its program by absolute path, so the start
    /// never depends on `PATH`.
    #[test]
    fn local_entry_resolves_the_program() {
        let sh = find_program("sh").expect("sh is installed");
        let e = local_entry(&local_app(
            "s.desktop",
            unknown("sh"),
            &["sh", "-c", "true"],
        ));
        let Source::Launcher { command, .. } = &e.source else {
            panic!()
        };
        assert_eq!(command[0], sh.to_string_lossy());
        assert_eq!(command[1..], ["-c", "true"]);
        // An unknown program stays as it is and the entry does not validate.
        let e = local_entry(&local_app(
            "n.desktop",
            unknown("no-such-program-x"),
            &["no-such-program-x"],
        ));
        assert!(e.validate().is_err());
    }

    #[test]
    fn catalog_matches_distro_packages() {
        let catalog = [entry(
            r#"{"id":"clock","name":"Clock",
            "source":{"type":"distro-package","package":"clock","manager":"rpm","command":["/usr/bin/clock"]}}"#,
        )];
        let rpm = LocalSource::Rpm {
            package: "clock".into(),
        };
        assert_eq!(catalog_entry(&catalog, &rpm), Some(0));
        // The same name in the other package format is another package.
        let deb = LocalSource::Deb {
            package: "clock".into(),
        };
        assert_eq!(catalog_entry(&catalog, &deb), None);
        let other = LocalSource::Rpm {
            package: "watch".into(),
        };
        assert_eq!(catalog_entry(&catalog, &other), None);
    }

    /// A desktop entry that starts through a shell, interpreter or launcher
    /// does not belong to that program's package.
    #[tokio::test]
    async fn shared_launchers_have_no_package() {
        let f = Fakes::new("local-shared-launcher");
        f.set("rpm", "echo bash");
        f.set("dpkg", "echo 'bash: /usr/bin/bash'");
        for exec in [
            "sh -c 'exec /opt/x/run'",
            "/usr/bin/env FOO=1 bash -c x",
            "python3 /opt/x/app.py",
        ] {
            let mut a = app(&[]);
            a.exec = split_exec(exec);
            let program = a.exec[0].clone();
            assert_eq!(
                resolve_source(&a, &f.programs).await,
                LocalSource::Unknown { program },
                "{exec}"
            );
        }
        assert!(f.calls().is_empty(), "{:?}", f.calls());
        // A Flatpak is still found by its id.
        f.flatpak_with(&[Installation::User]);
        let mut a = exec_app("/usr/bin/flatpak run org.x.App");
        a.flatpak_id = Some("org.x.App".into());
        assert!(matches!(
            resolve_source(&a, &f.programs).await,
            LocalSource::Flatpak { .. }
        ));
    }
}
