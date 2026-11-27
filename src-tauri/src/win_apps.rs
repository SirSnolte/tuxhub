//! Programs installed with a Windows installer.
//!
//! tuxhub runs the installer of a release (`*-setup.exe` built with NSIS, or
//! an `.msi` package) and then finds what it installed through the uninstall
//! entries of the registry: the entry that is new or has a new version names
//! the program and the command that removes it again. Everything read from
//! the registry is checked before it is used; a command is never run through
//! a shell.

use crate::tools::Setup;
use std::path::{Path, PathBuf};

/// An uninstall entry of the registry (`…\CurrentVersion\Uninstall\<key>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallEntry {
    /// The hive and name of the key, for example `HKCU\Fliq`.
    pub key: String,
    pub name: String,
    pub version: Option<String>,
    /// `DisplayIcon`: usually the program, sometimes with `,<index>`.
    pub icon: Option<String>,
    pub location: Option<String>,
    pub uninstall: Option<String>,
    pub quiet_uninstall: Option<String>,
    /// Marked as a part of the system, or an update of another entry.
    pub system: bool,
}

/// The kinds of installers tuxhub runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    /// An NSIS installer (`*-setup.exe`), silent with `/S`.
    Nsis,
    /// A Windows Installer package, installed with `msiexec`.
    Msi,
}

/// The installer kind of a release asset; `None` for anything else, such as
/// a portable program or an archive.
pub fn installer_of(asset: &str) -> Option<Installer> {
    let lower = asset.to_ascii_lowercase();
    if lower.ends_with(".msi") {
        Some(Installer::Msi)
    } else if lower.ends_with(".exe") && (lower.contains("setup") || lower.contains("install")) {
        Some(Installer::Nsis)
    } else {
        None
    }
}

/// The program and arguments that run `file` without questions. An
/// installer that needs administrator rights still asks Windows for them.
pub fn install_command(kind: Installer, file: &Path) -> (PathBuf, Vec<String>) {
    match kind {
        Installer::Nsis => (file.to_path_buf(), vec!["/S".into()]),
        Installer::Msi => (
            "msiexec".into(),
            vec![
                "/i".into(),
                file.to_string_lossy().into_owned(),
                "/passive".into(),
                "/norestart".into(),
            ],
        ),
    }
}

/// Letters and digits only, in lower case: `GIF Color Changer` and
/// `gif-color-changer` compare equal.
fn plain(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Whether the entry's name is the app's name.
fn names_app(entry: &UninstallEntry, app: &str) -> bool {
    let (name, app) = (plain(&entry.name), plain(app));
    !app.is_empty() && (name == app || name.starts_with(&app))
}

/// The entry an installer created or updated: new or with a new version
/// since `before`, the one named like the app when there are several. A
/// reinstall of the same version changes nothing; then the entry of
/// `known_key` (from the last installation) or the one named like the app
/// counts.
pub fn pick<'a>(
    before: &[UninstallEntry],
    after: &'a [UninstallEntry],
    app: &str,
    known_key: Option<&str>,
) -> Option<&'a UninstallEntry> {
    let candidates: Vec<&UninstallEntry> = after
        .iter()
        .filter(|e| !e.system)
        .filter(|e| {
            before
                .iter()
                .find(|b| b.key == e.key)
                .is_none_or(|b| b.version != e.version)
        })
        .collect();
    if let Some(e) = candidates.iter().find(|e| names_app(e, app)) {
        return Some(e);
    }
    if let [only] = candidates.as_slice() {
        return Some(only);
    }
    let unchanged = after.iter().filter(|e| !e.system);
    known_key
        .and_then(|k| unchanged.clone().find(|e| e.key == k))
        .or_else(|| unchanged.clone().find(|e| names_app(e, app)))
}

/// The program `DisplayIcon` names: quotes and an icon index removed. Only
/// an absolute path to an `.exe` counts.
pub fn program_of_icon(icon: &str) -> Option<PathBuf> {
    let mut s = icon.trim();
    if let Some((path, index)) = s.rsplit_once(',')
        && index
            .trim()
            .trim_start_matches('-')
            .chars()
            .all(|c| c.is_ascii_digit())
        && !index.trim().is_empty()
    {
        s = path.trim();
    }
    let s = s.trim_matches('"');
    let lower = s.to_ascii_lowercase();
    let absolute =
        s.len() > 3 && s.as_bytes()[1] == b':' && matches!(s.as_bytes()[2], b'\\' | b'/');
    (absolute && lower.ends_with(".exe") && !lower.ends_with("uninstall.exe"))
        .then(|| PathBuf::from(s))
}

/// Splits a Windows command line into its arguments the way programs read
/// them: spaces separate, double quotes group, `\"` is a quote.
pub fn split_command_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                chars.next();
                cur.push('"');
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ' ' | '\t' if !quoted => {
                if started {
                    args.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            _ => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(cur);
    }
    args
}

/// The command that removes the entry's app without questions: its quiet
/// uninstall command, else its uninstall command made quiet (`msiexec /x`
/// with `/qn`, an NSIS uninstaller with `/S`). The program must be given by
/// absolute path, or be msiexec.
pub fn uninstall_command(entry: &UninstallEntry) -> Option<Vec<String>> {
    let (line, quiet) = match (&entry.quiet_uninstall, &entry.uninstall) {
        (Some(q), _) => (q, true),
        (None, Some(u)) => (u, false),
        (None, None) => return None,
    };
    let mut args = split_command_line(line);
    let program = args.first()?.to_ascii_lowercase();
    let msiexec =
        matches!(program.as_str(), "msiexec" | "msiexec.exe") || program.ends_with("\\msiexec.exe");
    let absolute = program.len() > 3 && program.as_bytes()[1] == b':';
    if !msiexec && !absolute {
        return None;
    }
    if msiexec {
        // `MsiExec.exe /I{GUID}` in an uninstall entry opens the repair
        // dialog; `/X` removes.
        for a in args.iter_mut().skip(1) {
            if let (Some(flag), Some(product)) = (a.get(..2), a.get(2..))
                && flag.eq_ignore_ascii_case("/i")
                && product.starts_with('{')
            {
                *a = format!("/X{product}");
            }
        }
        if !args.iter().any(|a| a.eq_ignore_ascii_case("/qn")) {
            args.push("/qn".into());
        }
    } else if !quiet && !args.iter().skip(1).any(|a| a.eq_ignore_ascii_case("/S")) {
        args.push("/S".into());
    }
    Some(args)
}

/// What tuxhub keeps of an entry it installed, or why it cannot use it.
pub fn setup_of(entry: &UninstallEntry, program: Option<PathBuf>) -> Result<Setup, String> {
    let program = program
        .or_else(|| entry.icon.as_deref().and_then(program_of_icon))
        .ok_or_else(|| format!("{}: the installed program was not found", entry.name))?;
    let uninstall = uninstall_command(entry)
        .ok_or_else(|| format!("{}: there is no way to remove it again", entry.name))?;
    Ok(Setup {
        key: entry.key.clone(),
        program: program.to_string_lossy().into_owned(),
        uninstall,
    })
}

/// The uninstall entries of the current user and of the computer, 64- and
/// 32-bit.
#[cfg(windows)]
pub fn read_all() -> Vec<UninstallEntry> {
    use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE};
    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    let roots: [(&Key, &str, &str); 3] = [
        (CURRENT_USER, "HKCU", UNINSTALL),
        (LOCAL_MACHINE, "HKLM", UNINSTALL),
        (LOCAL_MACHINE, "HKLM32", UNINSTALL_32),
    ];
    let text = |k: &Key, name: &str| {
        k.get_string(name)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let mut out = Vec::new();
    for (hive, label, path) in roots {
        let Ok(list) = hive.open(path) else { continue };
        let Ok(keys) = list.keys() else { continue };
        for name in keys {
            let Ok(k) = list.open(&name) else { continue };
            let Some(display) = text(&k, "DisplayName") else {
                continue;
            };
            out.push(UninstallEntry {
                key: format!(r"{label}\{name}"),
                name: display,
                version: text(&k, "DisplayVersion"),
                icon: text(&k, "DisplayIcon"),
                location: text(&k, "InstallLocation"),
                uninstall: text(&k, "UninstallString"),
                quiet_uninstall: text(&k, "QuietUninstallString"),
                system: k.get_u32("SystemComponent").is_ok_and(|v| v == 1)
                    || text(&k, "ParentKeyName").is_some(),
            });
        }
    }
    out
}

/// The only program in the installation folder of an entry without a
/// usable `DisplayIcon` (some `.msi` packages).
#[cfg(windows)]
pub fn program_in(location: &str) -> Option<PathBuf> {
    let exes: Vec<PathBuf> = std::fs::read_dir(location)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe"))
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().contains("uninst"))
        })
        .collect();
    match exes.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, name: &str, version: &str) -> UninstallEntry {
        UninstallEntry {
            key: key.into(),
            name: name.into(),
            version: Some(version.into()),
            ..Default::default()
        }
    }

    #[test]
    fn installer_kinds() {
        assert_eq!(
            installer_of("Fliq_2.2.0_x64-setup.exe"),
            Some(Installer::Nsis)
        );
        assert_eq!(
            installer_of("Fliq_2.2.0_x64_en-US.MSI"),
            Some(Installer::Msi)
        );
        assert_eq!(installer_of("tool-x86_64-windows.exe"), None);
        assert_eq!(installer_of("tool.zip"), None);
    }

    #[test]
    fn install_commands() {
        let (p, a) = install_command(Installer::Nsis, Path::new(r"C:\t\Fliq-setup.exe"));
        assert_eq!(
            (p, a),
            (
                PathBuf::from(r"C:\t\Fliq-setup.exe"),
                vec!["/S".to_string()]
            )
        );
        let (p, a) = install_command(Installer::Msi, Path::new(r"C:\t\a b.msi"));
        assert_eq!(p, PathBuf::from("msiexec"));
        assert_eq!(a, ["/i", r"C:\t\a b.msi", "/passive", "/norestart"]);
    }

    #[test]
    fn picks_the_new_entry() {
        let before = vec![entry("HKCU\\Steam", "Steam", "1")];
        let after = vec![
            entry("HKCU\\Steam", "Steam", "1"),
            entry("HKCU\\Fliq", "Fliq", "2.2.0"),
        ];
        assert_eq!(
            pick(&before, &after, "Fliq", None).unwrap().key,
            "HKCU\\Fliq"
        );
    }

    #[test]
    fn picks_the_updated_entry_named_like_the_app() {
        let before = vec![
            entry("HKCU\\gcc", "gif-color-changer", "0.1.0"),
            entry("HKCU\\x", "X", "1"),
        ];
        let after = vec![
            entry("HKCU\\gcc", "gif-color-changer", "0.2.0"),
            entry("HKCU\\x", "X", "2"),
        ];
        assert_eq!(
            pick(&before, &after, "GIF Color Changer", None)
                .unwrap()
                .key,
            "HKCU\\gcc"
        );
    }

    #[test]
    fn reinstall_of_the_same_version_finds_the_known_entry() {
        let list = vec![
            entry("HKCU\\Fliq", "Fliq", "2.2.0"),
            entry("HKCU\\Other", "Other", "1"),
        ];
        assert_eq!(
            pick(&list, &list, "Something", Some("HKCU\\Fliq"))
                .unwrap()
                .key,
            "HKCU\\Fliq"
        );
        assert_eq!(pick(&list, &list, "fliq", None).unwrap().key, "HKCU\\Fliq");
        assert!(pick(&list, &list, "Unknown", None).is_none());
    }

    #[test]
    fn system_entries_and_several_unrelated_changes_are_not_picked() {
        let mut sys = entry("HKLM\\KB1", "Update", "1");
        sys.system = true;
        let after = vec![sys, entry("HKCU\\a", "A", "1"), entry("HKCU\\b", "B", "1")];
        assert!(pick(&[], &after, "Fliq", None).is_none());
    }

    #[test]
    fn program_from_display_icon() {
        assert_eq!(
            program_of_icon(r#""C:\Users\Jo\AppData\Local\Fliq\Fliq.exe""#),
            Some(PathBuf::from(r"C:\Users\Jo\AppData\Local\Fliq\Fliq.exe"))
        );
        assert_eq!(
            program_of_icon(r"C:\Program Files\App\app.exe,0"),
            Some(PathBuf::from(r"C:\Program Files\App\app.exe"))
        );
        assert_eq!(
            program_of_icon(r"C:\Windows\Installer\{GUID}\icon.ico"),
            None
        );
        assert_eq!(program_of_icon(r"app.exe"), None);
        assert_eq!(program_of_icon(r"C:\App\uninstall.exe"), None);
    }

    #[test]
    fn command_lines_split_like_windows() {
        assert_eq!(
            split_command_line(r#""C:\Program Files\Fliq\uninstall.exe" /S"#),
            [r"C:\Program Files\Fliq\uninstall.exe", "/S"]
        );
        assert_eq!(
            split_command_line(r#"MsiExec.exe /X{1-2}"#),
            ["MsiExec.exe", "/X{1-2}"]
        );
        assert_eq!(split_command_line(r#"a "" b"#), ["a", "", "b"]);
        assert!(split_command_line("   ").is_empty());
    }

    #[test]
    fn uninstall_commands_are_quiet() {
        let mut e = entry("k", "Fliq", "1");
        e.quiet_uninstall = Some(r#""C:\U\Fliq\uninstall.exe" /S"#.into());
        e.uninstall = Some(r#""C:\U\Fliq\uninstall.exe""#.into());
        assert_eq!(
            uninstall_command(&e).unwrap(),
            [r"C:\U\Fliq\uninstall.exe", "/S"]
        );

        e.quiet_uninstall = None;
        assert_eq!(
            uninstall_command(&e).unwrap(),
            [r"C:\U\Fliq\uninstall.exe", "/S"]
        );

        e.uninstall = Some("MsiExec.exe /I{ABC}".into());
        assert_eq!(
            uninstall_command(&e).unwrap(),
            ["MsiExec.exe", "/X{ABC}", "/qn"]
        );

        e.uninstall = Some("MsiExec.exe /\u{c4}x".into());
        assert_eq!(
            uninstall_command(&e).unwrap(),
            ["MsiExec.exe", "/\u{c4}x", "/qn"]
        );

        e.uninstall = Some("uninstall.exe".into());
        assert_eq!(
            uninstall_command(&e),
            None,
            "a program without path is refused"
        );
        e.uninstall = None;
        assert_eq!(uninstall_command(&e), None);
    }

    #[test]
    fn setup_needs_program_and_uninstaller() {
        let mut e = entry("HKCU\\Fliq", "Fliq", "2.2.0");
        e.icon = Some(r#""C:\U\Fliq\Fliq.exe""#.into());
        assert!(setup_of(&e, None).unwrap_err().contains("remove"));
        e.uninstall = Some(r#""C:\U\Fliq\uninstall.exe""#.into());
        let s = setup_of(&e, None).unwrap();
        assert_eq!(s.program, r"C:\U\Fliq\Fliq.exe");
        assert_eq!(s.key, "HKCU\\Fliq");
        e.icon = None;
        assert!(setup_of(&e, None).unwrap_err().contains("not found"));
    }
}
