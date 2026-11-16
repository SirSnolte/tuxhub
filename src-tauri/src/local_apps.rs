//! Reads the freedesktop `.desktop` entries of apps installed on this PC.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

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
                    Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k') => had_code = true,
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
        if !meta.is_file()
            || meta.len() > MAX_FILE_BYTES
            || path.extension().is_none_or(|x| x != "desktop")
        {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let id = rel.to_string_lossy().replace('/', "-");
        // The first file with an ID shadows later ones, even if it is hidden.
        if !seen.insert(id.clone()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(app) = parse_desktop(&text, &id, &path) {
            apps.push(app);
        }
    }
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
}
