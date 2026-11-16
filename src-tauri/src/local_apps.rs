//! Reads the freedesktop `.desktop` entries of apps installed on this PC.

use crate::appstream::AppInfo;
use crate::tools::{Entry, Installation, Programs, Source};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILE_BYTES: u64 = 64 * 1024;
const MAX_DEPTH: usize = 8;
const OWN_DESKTOP_ID: &str = "io.github.sirsnolte.tuxhub.desktop";

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
        icon: icon.filter(|i| !i.is_empty()),
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
/// and console apps are hidden unless `show_all`; `NoDisplay` apps, ones not
/// meant for this desktop and tuxhub itself are always hidden.
pub fn is_visible(app: &DesktopApp, desktop: &str, show_all: bool) -> bool {
    if app.no_display || app.desktop_id == OWN_DESKTOP_ID {
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
    !(settings || system_only || has("ConsoleOnly"))
}

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
    if let Some(file) = find_program(&program) {
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
        match (&app.flatpak_id, &other.flatpak_id) {
            (Some(a), Some(b)) => a == b,
            // Every Flatpak starts with `flatpak run`: the program says nothing.
            (Some(_), None) | (None, Some(_)) => false,
            (None, None) => !app.exec.is_empty() && app.exec.first() == other.exec.first(),
        }
    })
}

/// The catalog entry that stands for this app, found by Flatpak ID or package.
fn catalog_entry<'a>(catalog: &'a [Entry], source: &LocalSource) -> Option<&'a Entry> {
    catalog.iter().find(|e| match (&e.source, source) {
        (Source::Flatpak { flatpak_id, .. }, LocalSource::Flatpak { flatpak_id: id, .. }) => {
            flatpak_id == id
        }
        (Source::SystemPackage { package, .. }, LocalSource::Rpm { package: p })
        | (Source::SystemPackage { package, .. }, LocalSource::Deb { package: p }) => package == p,
        _ => false,
    })
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

async fn list_in(
    dirs: &[PathBuf],
    desktop: &str,
    autostart_dir: &Path,
    index: &HashMap<String, AppInfo>,
    programs: &Programs,
    catalog: &[Entry],
    show_all: bool,
) -> Vec<LocalApp> {
    let visible: Vec<DesktopApp> = scan_dirs(dirs)
        .into_iter()
        .filter(|a| is_visible(a, desktop, show_all))
        .collect();
    let mut apps = futures_util::future::join_all(
        visible
            .into_iter()
            .map(|a| build_app(a, autostart_dir, index, programs)),
    )
    .await;
    for app in &mut apps {
        if let Some(e) = catalog_entry(catalog, &app.source) {
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
    }
    apps.sort_by_cached_key(|a| a.name.to_lowercase());
    apps
}

/// Every app of the start menu with its source, sorted by name. Apps the
/// catalog knows (by Flatpak ID or package) take their texts, icon and repo
/// from the catalog; their `desktop_id` stays.
pub async fn list(programs: &Programs, catalog: &[Entry], show_all: bool) -> Vec<LocalApp> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let desktop = desktop.split(':').next().unwrap_or("");
    list_in(
        &application_dirs(),
        desktop,
        &autostart_dir(),
        &crate::appstream::load_index(),
        programs,
        catalog,
        show_all,
    )
    .await
}

fn autostart_dir() -> PathBuf {
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

/// Like [`list`], but keeps the result in `cache` until an application folder
/// changes (or `show_all` does). The catalog texts are applied on every call.
pub async fn list_cached(
    cache: &std::sync::Mutex<Cache>,
    programs: &Programs,
    catalog: &[Entry],
    show_all: bool,
) -> Vec<LocalApp> {
    let stamp = newest_mtime(&application_dirs());
    if let Ok(guard) = cache.lock()
        && let Some((t, all, apps)) = guard.as_ref()
        && *t == stamp
        && *all == show_all
    {
        return apps.clone();
    }
    let apps = list(programs, catalog, show_all).await;
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
}
