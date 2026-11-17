//! Uninstalling apps of the start menu. A preview tells how an app is
//! removed and what goes with it; packages the desktop or system needs are
//! blocked. Removing runs the preview again and never trusts the interface.

use crate::local_apps::{LocalApp, LocalSource};
use crate::tools::{self, Installation, Programs};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// More packages than this going with an app blocks removing it.
const MAX_PACKAGES: usize = 25;

/// Packages tuxhub never removes, besides the ones of `protected.d`: the
/// desktop and its core apps, the system and the tools tuxhub needs. A
/// trailing `*` matches any ending.
const BLOCKED: &[&str] = &[
    "plasma-workspace",
    "plasma-desktop",
    "kwin*",
    "sddm*",
    "gnome-shell",
    "mutter",
    "gdm",
    "systemd*",
    "dnf*",
    "rpm",
    "kernel*",
    "glibc*",
    "polkit*",
    "NetworkManager*",
    "pipewire*",
    "wireplumber*",
    "flatpak",
    "sudo",
    "xpra",
    "tuxhub",
    // The desktop's own file manager, terminal, settings, system monitor and
    // software center.
    "dolphin",
    "konsole",
    "systemsettings",
    "kinfocenter",
    "plasma-systemmonitor",
    "plasma-discover*",
    "nautilus",
    "gnome-terminal",
    "ptyxis",
    "gnome-console",
    "gnome-control-center",
    "gnome-system-monitor",
    "gnome-software",
];

/// How an app is removed, what goes with it and whether tuxhub refuses.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemovePreview {
    /// "flatpak", "dnf", "steam", "heroic", "lutris", "appimage", "tool" or "none".
    pub method: String,
    /// What is removed: the Flatpak id, or every package dnf would remove.
    pub packages: Vec<String>,
    /// Why tuxhub does not remove it, as a code the interface words:
    /// `protected:<package>`, `too-many:<count>`, `dnf-refuses:<dnf's line>`,
    /// `dnf-silent`, `package-manager`, `outside-home`, `not-a-package`,
    /// `invalid`.
    pub blocked: Option<String>,
    /// The removal asks for the password.
    pub needs_admin: bool,
}

impl RemovePreview {
    fn new(method: &str, packages: Vec<String>, needs_admin: bool) -> Self {
        Self {
            method: method.into(),
            packages,
            blocked: None,
            needs_admin,
        }
    }

    fn blocked(method: &str, reason: &str) -> Self {
        Self {
            blocked: Some(reason.into()),
            ..Self::new(method, vec![], false)
        }
    }
}

/// What the preview and removal need besides the app.
pub struct Env<'a> {
    pub programs: &'a Programs,
    /// The user's home folder: AppImages are removed only inside it.
    pub home: &'a Path,
    /// `/etc/dnf/protected.d`.
    pub protected_dir: &'a Path,
    /// The app is a tool tuxhub installed (removed like one).
    pub tool: bool,
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub const PROTECTED_DIR: &str = "/etc/dnf/protected.d";

/// Names of the packages dnf protects: one per line in `*.conf`.
fn protected(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "conf"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|text| {
            text.lines()
                .map(|l| l.split('#').next().unwrap_or_default().trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

/// The package of `packages` that blocks removing them, if any.
fn blocking<'a>(packages: &'a [String], protected_dir: &Path) -> Option<&'a String> {
    let protected = protected(protected_dir);
    packages
        .iter()
        .find(|p| BLOCKED.iter().any(|b| matches(b, p)) || protected.iter().any(|b| matches(b, p)))
}

/// Machine architectures in the second column of a dnf package line.
const ARCHES: &[&str] = &[
    "x86_64", "noarch", "i686", "i386", "aarch64", "armv7hl", "ppc64le", "s390x", "riscv64",
];

/// The packages a `dnf remove --assumeno` listing would remove: indented
/// lines whose second column is an architecture.
pub fn parse_remove(out: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in out.lines() {
        if !line.starts_with(char::is_whitespace) {
            continue;
        }
        let mut cols = line.split_whitespace();
        let (Some(name), Some(arch)) = (cols.next(), cols.next()) else {
            continue;
        };
        if ARCHES.contains(&arch)
            && tools::valid_package_name(name)
            && !found.iter().any(|f| f == name)
        {
            found.push(name.to_string());
        }
    }
    found
}

async fn dnf_preview(programs: &Programs, package: &str, protected_dir: &Path) -> RemovePreview {
    if !tools::valid_package_name(package) {
        return RemovePreview::blocked("dnf", "invalid");
    }
    // Only resolves, without root and without the network.
    let out = Command::new(&programs.dnf)
        .args(["-q", "-C", "remove", "--assumeno", package])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = match tokio::time::timeout(programs.network_timeout, out).await {
        Ok(Ok(out)) => out,
        _ => return RemovePreview::blocked("dnf", "dnf-silent"),
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let packages = parse_remove(&text);
    if packages.is_empty() {
        // dnf refuses (a protected package needs it) or finds nothing.
        let why = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("dnf would remove nothing");
        return RemovePreview::blocked("dnf", &format!("dnf-refuses:{why}"));
    }
    let mut preview = RemovePreview::new("dnf", packages, true);
    if let Some(p) = blocking(&preview.packages, protected_dir) {
        preview.blocked = Some(format!("protected:{p}"));
    } else if preview.packages.len() > MAX_PACKAGES {
        preview.blocked = Some(format!("too-many:{}", preview.packages.len()));
    }
    preview
}

/// The file of an AppImage and its desktop entry, both inside the home
/// folder after following links; `None` otherwise.
fn appimage_files(app: &LocalApp, path: &str, home: &Path) -> Option<(PathBuf, PathBuf)> {
    let home = home.canonicalize().ok()?;
    let inside = |p: &Path| {
        let real = p.canonicalize().ok()?;
        (real.starts_with(&home) && real.is_file()).then_some(real)
    };
    Some((inside(Path::new(path))?, inside(&app.desktop_file)?))
}

/// How `app` would be removed.
pub async fn preview(env: &Env<'_>, app: &LocalApp) -> RemovePreview {
    if env.tool {
        return RemovePreview::new("tool", vec![], false);
    }
    match &app.source {
        LocalSource::Flatpak {
            flatpak_id,
            installations,
            ..
        } => {
            if !tools::valid_flatpak_name(flatpak_id) {
                return RemovePreview::blocked("flatpak", "invalid");
            }
            let system = installations.first() == Some(&Installation::System);
            RemovePreview::new("flatpak", vec![flatpak_id.clone()], system)
        }
        LocalSource::Rpm { package } => dnf_preview(env.programs, package, env.protected_dir).await,
        LocalSource::Deb { .. } => {
            RemovePreview::blocked("none", "package-manager")
        }
        LocalSource::Game { store, game } => match store.as_str() {
            "steam" if steam_app_id(game).is_some() => RemovePreview::new("steam", vec![], false),
            "heroic" | "lutris" => RemovePreview::new(store, vec![], false),
            _ => RemovePreview::blocked("none", "invalid"),
        },
        LocalSource::Appimage { path } => match appimage_files(app, path, env.home) {
            Some(_) => RemovePreview::new("appimage", vec![], false),
            None => RemovePreview::blocked("none", "outside-home"),
        },
        LocalSource::Unknown { .. } => RemovePreview::blocked("none", "not-a-package"),
    }
}

/// A Steam app id: digits only, at most 12.
fn steam_app_id(game: &str) -> Option<&str> {
    (!game.is_empty() && game.len() <= 12 && game.chars().all(|c| c.is_ascii_digit()))
        .then_some(game)
}

async fn run(program: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    match out.status.code() {
        Some(0) => Ok(()),
        _ => Err(tools::failure_text(&out)),
    }
}

/// Removes `app` the way [`preview`] tells, after asking it again; refused
/// when it is blocked. `keep_data`: a Flatpak keeps its data. A tool tuxhub
/// installed is not removed here (the caller uninstalls it as a tool).
pub async fn remove(env: &Env<'_>, app: &LocalApp, keep_data: bool) -> Result<(), String> {
    let preview = preview(env, app).await;
    if let Some(why) = preview.blocked {
        return Err(format!("{} is not removed: {why}", app.name));
    }
    match (preview.method.as_str(), &app.source) {
        ("flatpak", LocalSource::Flatpak { flatpak_id, .. }) => {
            let flag = if preview.needs_admin {
                "--system"
            } else {
                "--user"
            };
            let mut args = vec!["uninstall", "-y", "--noninteractive", "--app", flag];
            if !keep_data {
                args.push("--delete-data");
            }
            args.push(flatpak_id);
            run(&env.programs.flatpak, &args).await
        }
        ("dnf", LocalSource::Rpm { package }) => {
            let dnf = if env.programs.dnf.is_absolute() {
                env.programs.dnf.clone()
            } else {
                crate::embed::which(&env.programs.dnf.to_string_lossy()).ok_or("dnf is missing")?
            };
            let out = Command::new(&env.programs.pkexec)
                .arg(&dnf)
                .args(["-y", "remove", package])
                .stdin(Stdio::null())
                .output()
                .await
                .map_err(|e| format!("could not run pkexec: {e}"))?;
            match out.status.code() {
                Some(0) => Ok(()),
                Some(126) | Some(127) => Err(crate::local_updates::CANCELLED.into()),
                _ => Err(tools::failure_text(&out)),
            }
        }
        ("steam", LocalSource::Game { game, .. }) => {
            let id = steam_app_id(game).ok_or("invalid Steam app id")?;
            run(
                &env.programs.xdg_open,
                &[&format!("steam://uninstall/{id}")],
            )
            .await
        }
        // Heroic and Lutris remove their games themselves: tuxhub opens them.
        ("heroic", _) => run(&env.programs.xdg_open, &["heroic://"]).await,
        ("lutris", _) => run(&env.programs.xdg_open, &["lutris:"]).await,
        ("appimage", LocalSource::Appimage { path }) => {
            let (file, entry) =
                appimage_files(app, path, env.home).ok_or("not in your home folder")?;
            std::fs::remove_file(&file).map_err(|e| e.to_string())?;
            std::fs::remove_file(&entry).map_err(|e| e.to_string())
        }
        _ => Err(format!("{} is not removed by tuxhub", app.name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Fakes;

    fn app(source: LocalSource) -> LocalApp {
        LocalApp {
            desktop_id: "x.desktop".into(),
            name: "X".into(),
            summary: None,
            description: None,
            icon: None,
            homepage: None,
            repo: None,
            categories: vec![],
            source,
            command: vec![],
            autostart: false,
            system: false,
            desktop_file: PathBuf::new(),
        }
    }

    fn rpm(package: &str) -> LocalApp {
        app(LocalSource::Rpm {
            package: package.into(),
        })
    }

    struct Setup {
        f: Fakes,
        home: PathBuf,
        protected: PathBuf,
    }

    impl Setup {
        fn new(name: &str) -> Self {
            let f = Fakes::new(name);
            let home = f.dir.join("home");
            let protected = f.dir.join("protected.d");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::create_dir_all(&protected).unwrap();
            Self { f, home, protected }
        }

        fn env(&self) -> Env<'_> {
            Env {
                programs: &self.f.programs,
                home: &self.home,
                protected_dir: &self.protected,
                tool: false,
            }
        }

        /// dnf lists `packages` for `remove --assumeno`, runs anything else.
        fn dnf_removes(&self, packages: &[&str]) {
            let mut listing = String::from("Package  Arch  Version  Repository  Size\nRemoving:\n");
            for p in packages {
                listing.push_str(&format!(" {p}  x86_64  0:1.0-1.fc44  updates  1.0 MiB\n"));
            }
            listing.push_str("\nTransaction Summary:\n Removing:  4 packages\n");
            let file = self.f.dir.join("listing");
            std::fs::write(&file, listing).unwrap();
            self.f.set(
                "dnf",
                &format!(
                    "case \"$*\" in *--assumeno*) cat '{}'; exit 1;; esac; exit 0",
                    file.display()
                ),
            );
        }
    }

    #[test]
    fn parses_the_listing_of_dnf5() {
        let out = "\
Package          Arch   Version          Repository      Size
Removing:
 dolphin         x86_64 0:26.08.2-3.fc44 updates     14.0 MiB
Removing unused dependencies:
 baloo-widgets   x86_64 0:26.08.2-1.fc44 updates    670.1 KiB
 dolphin-plugins noarch 0:26.08.2-1.fc44 updates      3.1 MiB

Transaction Summary:
 Removing:           3 packages
";
        assert_eq!(
            parse_remove(out),
            ["dolphin", "baloo-widgets", "dolphin-plugins"]
        );
    }

    #[tokio::test]
    async fn a_desktop_package_among_the_removed_blocks() {
        let s = Setup::new("lr-desktop");
        s.dnf_removes(&["kdeconnect", "plasma-workspace"]);
        let p = preview(&s.env(), &rpm("kdeconnect")).await;
        assert_eq!(p.method, "dnf");
        assert!(p.needs_admin);
        assert_eq!(p.blocked.as_deref(), Some("protected:plasma-workspace"));
        // The desktop's file manager itself.
        s.dnf_removes(&["dolphin", "dolphin-libs"]);
        assert!(preview(&s.env(), &rpm("dolphin")).await.blocked.is_some());
        // A plain app is fine.
        s.dnf_removes(&["gimp", "gimp-libs"]);
        let p = preview(&s.env(), &rpm("gimp")).await;
        assert_eq!(p.blocked, None);
        assert_eq!(p.packages, ["gimp", "gimp-libs"]);
        assert!(
            s.f.calls()
                .contains(&"dnf -q -C remove --assumeno gimp".to_string())
        );
    }

    #[tokio::test]
    async fn more_than_25_packages_block() {
        let s = Setup::new("lr-many");
        let names: Vec<String> = (0..26).map(|i| format!("lib{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        s.dnf_removes(&refs);
        let p = preview(&s.env(), &rpm("lib0")).await;
        assert_eq!(p.blocked.as_deref(), Some("too-many:26"));
        s.dnf_removes(&refs[..25]);
        assert_eq!(preview(&s.env(), &rpm("lib0")).await.blocked, None);
    }

    #[tokio::test]
    async fn packages_of_protected_d_block() {
        let s = Setup::new("lr-protected");
        std::fs::write(s.protected.join("mine.conf"), "# comment\nmy-core\n").unwrap();
        std::fs::write(s.protected.join("other.txt"), "gimp\n").unwrap();
        s.dnf_removes(&["tool", "my-core"]);
        assert_eq!(
            preview(&s.env(), &rpm("tool")).await.blocked.as_deref(),
            Some("protected:my-core")
        );
        // Only `*.conf` files count.
        s.dnf_removes(&["gimp"]);
        assert_eq!(preview(&s.env(), &rpm("gimp")).await.blocked, None);
    }

    #[tokio::test]
    async fn dnf_refusing_or_listing_nothing_blocks() {
        let s = Setup::new("lr-refuse");
        s.f.set(
            "dnf",
            "echo 'Failed to resolve the transaction:'; echo 'Problem 1: protected packages: plasma-desktop'; exit 1",
        );
        let p = preview(&s.env(), &rpm("plasma-workspace")).await;
        assert!(
            p.blocked
                .unwrap()
                .starts_with("dnf-refuses:Failed to resolve")
        );
    }

    #[tokio::test]
    async fn remove_asks_again_and_runs_no_pkexec_when_blocked_now() {
        let s = Setup::new("lr-recheck");
        s.f.set("pkexec", "exit 0");
        s.dnf_removes(&["gimp"]);
        assert_eq!(preview(&s.env(), &rpm("gimp")).await.blocked, None);
        // Meanwhile something changed: now it would take the desktop along.
        s.dnf_removes(&["gimp", "plasma-desktop"]);
        let err = remove(&s.env(), &rpm("gimp"), true).await.unwrap_err();
        assert!(err.contains("not removed"), "{err}");
        assert!(!s.f.calls().iter().any(|c| c.starts_with("pkexec")));
        // Allowed again: one pkexec with dnf remove.
        s.dnf_removes(&["gimp"]);
        remove(&s.env(), &rpm("gimp"), true).await.unwrap();
        let calls = s.f.calls();
        let pk: Vec<_> = calls.iter().filter(|c| c.starts_with("pkexec")).collect();
        assert_eq!(pk.len(), 1);
        assert!(pk[0].ends_with("/dnf -y remove gimp"), "{}", pk[0]);
    }

    #[tokio::test]
    async fn pkexec_cancel_is_reported() {
        let s = Setup::new("lr-cancel");
        s.f.set("pkexec", "exit 126");
        s.dnf_removes(&["gimp"]);
        assert_eq!(
            remove(&s.env(), &rpm("gimp"), true).await.unwrap_err(),
            "cancelled"
        );
    }

    #[tokio::test]
    async fn flatpak_data_is_deleted_only_when_asked() {
        let s = Setup::new("lr-flatpak");
        s.f.set("flatpak", "exit 0");
        let user = app(LocalSource::Flatpak {
            flatpak_id: "org.example.App".into(),
            installations: vec![Installation::User],
            origin: None,
        });
        let p = preview(&s.env(), &user).await;
        assert_eq!(
            p,
            RemovePreview::new("flatpak", vec!["org.example.App".into()], false)
        );
        remove(&s.env(), &user, false).await.unwrap();
        remove(&s.env(), &user, true).await.unwrap();
        let system = app(LocalSource::Flatpak {
            flatpak_id: "org.example.App".into(),
            installations: vec![Installation::System, Installation::User],
            origin: None,
        });
        assert!(preview(&s.env(), &system).await.needs_admin);
        remove(&s.env(), &system, true).await.unwrap();
        assert_eq!(
            s.f.calls(),
            [
                "flatpak uninstall -y --noninteractive --app --user --delete-data org.example.App",
                "flatpak uninstall -y --noninteractive --app --user org.example.App",
                "flatpak uninstall -y --noninteractive --app --system org.example.App",
            ]
        );
    }

    #[tokio::test]
    async fn steam_games_are_removed_by_steam_only() {
        let s = Setup::new("lr-steam");
        s.f.set("xdg-open", "exit 0");
        let game = |id: &str| {
            app(LocalSource::Game {
                store: "steam".into(),
                game: id.into(),
            })
        };
        assert_eq!(preview(&s.env(), &game("620")).await.method, "steam");
        remove(&s.env(), &game("620"), true).await.unwrap();
        for bad in ["62a", "", "1234567890123", "620;rm", "../620"] {
            assert!(
                preview(&s.env(), &game(bad)).await.blocked.is_some(),
                "{bad}"
            );
            assert!(remove(&s.env(), &game(bad), true).await.is_err(), "{bad}");
        }
        assert_eq!(s.f.calls(), ["xdg-open steam://uninstall/620"]);
    }

    #[tokio::test]
    async fn appimages_are_removed_only_inside_home() {
        let s = Setup::new("lr-appimage");
        let file = s.home.join("Apps/x.AppImage");
        let entry = s.home.join(".local/share/applications/x.desktop");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&file, "img").unwrap();
        std::fs::write(&entry, "[Desktop Entry]").unwrap();
        let mut inside = app(LocalSource::Appimage {
            path: file.to_string_lossy().into(),
        });
        inside.desktop_file = entry.clone();
        // Outside: a file elsewhere, and a link out of home.
        let elsewhere = s.f.dir.join("y.AppImage");
        std::fs::write(&elsewhere, "img").unwrap();
        let mut outside = app(LocalSource::Appimage {
            path: elsewhere.to_string_lossy().into(),
        });
        outside.desktop_file = entry.clone();
        let link = s.home.join("Apps/link.AppImage");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        let mut linked = app(LocalSource::Appimage {
            path: link.to_string_lossy().into(),
        });
        linked.desktop_file = entry.clone();
        for a in [&outside, &linked] {
            let p = preview(&s.env(), a).await;
            assert_eq!(p.method, "none");
            assert!(remove(&s.env(), a, true).await.is_err());
        }
        assert!(elsewhere.is_file());
        assert_eq!(preview(&s.env(), &inside).await.method, "appimage");
        remove(&s.env(), &inside, true).await.unwrap();
        assert!(!file.exists() && !entry.exists());
    }

    #[tokio::test]
    async fn scripts_and_unknown_programs_are_not_removed() {
        let s = Setup::new("lr-unknown");
        let a = app(LocalSource::Unknown {
            program: "/home/u/start.sh".into(),
        });
        let p = preview(&s.env(), &a).await;
        assert_eq!(
            p,
            RemovePreview::blocked("none", "not-a-package")
        );
        assert!(remove(&s.env(), &a, true).await.is_err());
        assert!(s.f.calls().is_empty());
    }
}
