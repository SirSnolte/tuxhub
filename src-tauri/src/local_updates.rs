//! Updates of every app of the start menu: one `dnf check-update` for all
//! packages and one `flatpak remote-ls --updates` per installation find them;
//! one `pkexec dnf upgrade` (one password) and one `flatpak update` per
//! installation install them. Games, AppImages and other programs are
//! updated elsewhere.

use crate::local_apps::{LocalApp, LocalSource};
use crate::tools::{self, Installation, Programs};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// The result of updating one app.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateResult {
    pub desktop_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

pub const ELSEWHERE: &str = "updated elsewhere";
pub const CANCELLED: &str = "cancelled";

/// Packages and their new version from the output of `dnf -q check-update`.
/// A long name puts the other columns on the next line; the list ends at
/// "Obsoleting packages", whose lines name packages that are replaced.
pub fn parse_check_update(out: &str) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut pending: Option<String> = None;
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.to_ascii_lowercase().starts_with("obsoleting") {
            break;
        }
        let mut cols = line.split_whitespace();
        let starts_indented = line.starts_with(char::is_whitespace);
        let (name, version) = match (starts_indented, pending.take()) {
            // The rest of a long line: version and repository.
            (true, Some(name)) => (name, cols.next()),
            (true, None) => continue,
            (false, _) => {
                let Some(first) = cols.next() else { continue };
                match cols.next() {
                    Some(v) => (first.to_string(), Some(v)),
                    None => {
                        pending = Some(first.to_string());
                        continue;
                    }
                }
            }
        };
        // `name.arch`: the architecture follows the last dot.
        let Some((package, _arch)) = name.rsplit_once('.') else {
            continue;
        };
        if !tools::valid_package_name(package) {
            continue;
        }
        if let Some(Ok(v)) = version.map(tools::clean_version) {
            found.insert(package.to_string(), v);
        }
    }
    found
}

async fn output(
    programs: &Programs,
    program: &Path,
    args: &[&str],
) -> Result<std::process::Output, String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    tokio::time::timeout(programs.network_timeout, out)
        .await
        .map_err(|_| format!("{} did not answer in time", program.display()))?
        .map_err(|e| format!("could not run {}: {e}", program.display()))
}

/// Every package with a pending update (exit code 100), none (0).
pub async fn rpm_updates(programs: &Programs) -> Result<BTreeMap<String, String>, String> {
    let out = output(programs, &programs.dnf, &["-q", "check-update"]).await?;
    match out.status.code() {
        Some(0) => Ok(BTreeMap::new()),
        Some(100) => Ok(parse_check_update(&String::from_utf8_lossy(&out.stdout))),
        _ => Err(tools::failure_text(&out)),
    }
}

/// Every app of one Flatpak installation with a pending update: its version,
/// or its short commit when the update names no version.
pub async fn flatpak_updates(
    programs: &Programs,
    installation: Installation,
) -> Result<BTreeMap<String, String>, String> {
    let out = output(
        programs,
        &programs.flatpak,
        &[
            "remote-ls",
            "--updates",
            "--app",
            installation.flag(),
            "--columns=application,version,commit",
        ],
    )
    .await?;
    if !out.status.success() {
        return Err(tools::failure_text(&out));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut cols = line.split('\t').map(str::trim);
            let id = cols.next().filter(|i| tools::valid_flatpak_name(i))?;
            let version = cols.next().unwrap_or_default();
            let commit: String = cols.next().unwrap_or_default().chars().take(12).collect();
            let shown = if version.is_empty() {
                commit
            } else {
                version.to_string()
            };
            (!shown.is_empty()).then(|| (id.to_string(), shown))
        })
        .collect())
}

/// The installation tuxhub uses for a Flatpak: the first one it is in.
fn flatpak_of(app: &LocalApp) -> Option<(&str, Installation)> {
    match &app.source {
        LocalSource::Flatpak {
            flatpak_id,
            installations,
            ..
        } => Some((flatpak_id, *installations.first()?)),
        _ => None,
    }
}

/// Pending updates of `apps` by desktop id. A source that cannot be asked
/// (program missing, no network) is left out and logged; the others stay.
pub async fn check(programs: &Programs, apps: &[LocalApp]) -> BTreeMap<String, String> {
    let has_rpm = apps
        .iter()
        .any(|a| matches!(a.source, LocalSource::Rpm { .. }));
    let wanted = |inst| {
        apps.iter()
            .any(|a| flatpak_of(a).is_some_and(|(_, i)| i == inst))
    };
    let rpm = async {
        if has_rpm {
            rpm_updates(programs).await
        } else {
            Ok(BTreeMap::new())
        }
    };
    let flatpak = |inst| async move {
        if wanted(inst) {
            flatpak_updates(programs, inst).await
        } else {
            Ok(BTreeMap::new())
        }
    };
    let (rpm, system, user) = futures_util::future::join3(
        rpm,
        flatpak(Installation::System),
        flatpak(Installation::User),
    )
    .await;
    let take = |r: Result<BTreeMap<String, String>, String>, what: &str| {
        r.unwrap_or_else(|e| {
            eprintln!("update check of {what} failed: {e}");
            BTreeMap::new()
        })
    };
    let rpm = take(rpm, "packages");
    let system = take(system, "system Flatpaks");
    let user = take(user, "user Flatpaks");
    apps.iter()
        .filter_map(|a| {
            let version = match &a.source {
                LocalSource::Rpm { package } => rpm.get(package),
                LocalSource::Flatpak { .. } => {
                    let (id, inst) = flatpak_of(a)?;
                    match inst {
                        Installation::System => system.get(id),
                        Installation::User => user.get(id),
                    }
                }
                _ => None,
            }?;
            Some((a.desktop_id.clone(), version.clone()))
        })
        .collect()
}

/// Pending updates of the start menu apps, as the hub keeps them.
#[derive(Debug, Clone, Default)]
pub struct Pending {
    /// Desktop id -> new version.
    pub versions: BTreeMap<String, String>,
    /// Desktop id -> what is updated (package or Flatpak id) and the id of
    /// the tool it is in tuxhub, if it is one.
    pub what: BTreeMap<String, (String, Option<String>)>,
}

impl Pending {
    /// `versions` of `apps`; `tool_of` names the tool an app is in tuxhub.
    pub fn new(
        versions: BTreeMap<String, String>,
        apps: &[LocalApp],
        tool_of: impl Fn(&LocalApp) -> Option<String>,
    ) -> Self {
        let what = apps
            .iter()
            .filter(|a| versions.contains_key(&a.desktop_id))
            .filter_map(|a| {
                let key = match &a.source {
                    LocalSource::Rpm { package } => format!("rpm:{package}"),
                    LocalSource::Flatpak { flatpak_id, .. } => format!("flatpak:{flatpak_id}"),
                    _ => return None,
                };
                Some((a.desktop_id.clone(), (key, tool_of(a))))
            })
            .collect();
        Self { versions, what }
    }

    /// Updates to show in the tray: the tools' ones, and each package or
    /// Flatpak once that is not one of those tools.
    pub fn count_with(&self, tool_updates: &BTreeMap<String, String>) -> usize {
        let mut keys: Vec<&String> = self
            .what
            .values()
            .filter(|(_, tool)| !tool.as_ref().is_some_and(|t| tool_updates.contains_key(t)))
            .map(|(key, _)| key)
            .collect();
        keys.sort();
        keys.dedup();
        tool_updates.len() + keys.len()
    }
}

/// A program pkexec runs: only by its full path.
fn absolute(program: &Path) -> Result<PathBuf, String> {
    if program.is_absolute() {
        return Ok(program.to_path_buf());
    }
    crate::embed::which(&program.to_string_lossy())
        .ok_or_else(|| format!("{} is missing", program.display()))
}

fn result(desktop_id: &str, outcome: &Result<(), String>) -> UpdateResult {
    UpdateResult {
        desktop_id: desktop_id.to_string(),
        ok: outcome.is_ok(),
        error: outcome.as_ref().err().cloned(),
    }
}

/// Installs the updates of the apps `desktop_ids` names, found in `apps`:
/// all packages with one `pkexec dnf -y upgrade`, Flatpaks with one
/// `flatpak update` per installation. `progress(desktop_id, stage)` reports
/// each step. Apps tuxhub does not update get "updated elsewhere"; a
/// cancelled password prompt gives every package "cancelled".
pub async fn update(
    programs: &Programs,
    apps: &[LocalApp],
    desktop_ids: &[String],
    progress: impl Fn(&str, &str),
) -> Vec<UpdateResult> {
    let mut results: BTreeMap<String, UpdateResult> = BTreeMap::new();
    let mut packages: Vec<(String, String)> = Vec::new();
    let mut flatpaks: BTreeMap<&'static str, Vec<(String, String)>> = BTreeMap::new();
    for id in desktop_ids {
        if results.contains_key(id) || packages.iter().any(|(d, _)| d == id) {
            continue;
        }
        let app = apps.iter().find(|a| a.desktop_id == *id);
        match app.map(|a| &a.source) {
            Some(LocalSource::Rpm { package }) => {
                if tools::valid_package_name(package) {
                    packages.push((id.clone(), package.clone()));
                } else {
                    results.insert(
                        id.clone(),
                        result(id, &Err(format!("invalid package name {package:?}"))),
                    );
                }
            }
            Some(LocalSource::Flatpak { .. }) => {
                let (fid, inst) = flatpak_of(app.unwrap()).unwrap_or(("", Installation::User));
                if tools::valid_flatpak_name(fid) {
                    flatpaks
                        .entry(inst.flag())
                        .or_default()
                        .push((id.clone(), fid.to_string()));
                } else {
                    results.insert(
                        id.clone(),
                        result(id, &Err(format!("invalid Flatpak id {fid:?}"))),
                    );
                }
            }
            _ => {
                results.insert(id.clone(), result(id, &Err(ELSEWHERE.into())));
            }
        }
    }
    if !packages.is_empty() {
        for (d, _) in &packages {
            progress(d, "dnf");
        }
        let outcome = upgrade_packages(programs, &packages).await;
        for (d, _) in &packages {
            results.insert(d.clone(), result(d, &outcome));
        }
    }
    for (flag, list) in flatpaks {
        for (d, _) in &list {
            progress(d, "flatpak");
        }
        let mut args = vec!["update", flag, "-y", "--noninteractive"];
        let mut ids: Vec<&str> = list.iter().map(|(_, f)| f.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        args.extend(ids);
        let outcome = match output(programs, &programs.flatpak, &args).await {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => Err(tools::failure_text(&out)),
            Err(e) => Err(e),
        };
        for (d, _) in &list {
            results.insert(d.clone(), result(d, &outcome));
        }
    }
    for id in desktop_ids {
        if let Some(r) = results.get(id) {
            progress(id, if r.ok { "done" } else { "failed" });
        }
    }
    desktop_ids
        .iter()
        .filter_map(|id| results.remove(id))
        .collect()
}

/// One `pkexec dnf -y upgrade p1 p2 …`, each package its own argument.
async fn upgrade_packages(
    programs: &Programs,
    packages: &[(String, String)],
) -> Result<(), String> {
    let dnf = absolute(&programs.dnf)?;
    let mut names: Vec<&str> = packages.iter().map(|(_, p)| p.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    let out = Command::new(&programs.pkexec)
        .arg(&dnf)
        .args(["-y", "upgrade"])
        .args(&names)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("could not run pkexec: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        // The password prompt was closed or the user may not do this.
        Some(126) | Some(127) => Err(CANCELLED.into()),
        _ => Err(tools::failure_text(&out)),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::tools::Fakes;

    fn app(desktop_id: &str, source: LocalSource) -> LocalApp {
        LocalApp {
            desktop_id: desktop_id.into(),
            name: desktop_id.into(),
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

    fn rpm(desktop_id: &str, package: &str) -> LocalApp {
        app(
            desktop_id,
            LocalSource::Rpm {
                package: package.into(),
            },
        )
    }

    fn flatpak(desktop_id: &str, id: &str, inst: Installation) -> LocalApp {
        app(
            desktop_id,
            LocalSource::Flatpak {
                flatpak_id: id.into(),
                installations: vec![inst],
                origin: Some("flathub".into()),
            },
        )
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    const CHECK_UPDATE: &str = "\
firefox.x86_64                     146.0-1.fc44              updates
a-very-long-package-name-for-the-columns.noarch
                                   2:3.4.5-6.fc44            updates
kernel.x86_64                      6.18.0-1.fc44             updates

Obsoleting packages
grub2-tools.x86_64                 1:2.12-30.fc44            updates
    grub2-tools-efi.x86_64         1:2.12-29.fc44            @updates
";

    #[test]
    fn parses_long_lines_and_stops_at_obsoleting() {
        let found = parse_check_update(CHECK_UPDATE);
        assert_eq!(
            found,
            BTreeMap::from([
                (
                    "a-very-long-package-name-for-the-columns".into(),
                    "3.4.5".into()
                ),
                ("firefox".into(), "146.0".into()),
                ("kernel".into(), "6.18.0".into()),
            ])
        );
    }

    fn fake_dnf(f: &Fakes, code: i32) {
        let out = f.dir.join("dnf.out");
        std::fs::write(&out, CHECK_UPDATE).unwrap();
        f.set("dnf", &format!("cat '{}'; exit {code}", out.display()));
    }

    fn fake_flatpak(f: &Fakes) {
        f.set(
            "flatpak",
            "case \"$*\" in *--user*) printf 'org.example.Chat\\t2.0\\tabcdef0123456789\\norg.other.App\\t\\t0123456789abcdef\\n';; esac",
        );
    }

    #[tokio::test]
    async fn check_reads_each_source_once() {
        let f = Fakes::new("lu-check");
        fake_dnf(&f, 100);
        fake_flatpak(&f);
        let apps = [
            rpm("firefox.desktop", "firefox"),
            rpm("firefox-private.desktop", "firefox"),
            rpm("dolphin.desktop", "dolphin"),
            flatpak("chat.desktop", "org.example.Chat", Installation::User),
            flatpak("other.desktop", "org.other.App", Installation::User),
        ];
        let found = check(&f.programs, &apps).await;
        assert_eq!(
            found,
            BTreeMap::from([
                ("chat.desktop".into(), "2.0".into()),
                ("firefox-private.desktop".into(), "146.0".into()),
                ("firefox.desktop".into(), "146.0".into()),
                ("other.desktop".into(), "0123456789ab".into()),
            ])
        );
        // kernel is not an app of the start menu.
        assert!(!found.values().any(|v| v.starts_with("6.18")));
        let calls = f.calls();
        assert_eq!(
            calls.iter().filter(|c| c.starts_with("dnf ")).count(),
            1,
            "{calls:?}"
        );
        assert!(calls.contains(&"dnf -q check-update".to_string()));
        // Only the installation that has apps is asked.
        assert_eq!(
            calls.iter().filter(|c| c.starts_with("flatpak ")).count(),
            1
        );
    }

    #[tokio::test]
    async fn check_exit_codes_and_failures() {
        let f = Fakes::new("lu-codes");
        fake_flatpak(&f);
        let apps = [
            rpm("firefox.desktop", "firefox"),
            flatpak("chat.desktop", "org.example.Chat", Installation::User),
        ];
        fake_dnf(&f, 0);
        assert_eq!(
            check(&f.programs, &apps).await.keys().collect::<Vec<_>>(),
            ["chat.desktop"]
        );
        fake_dnf(&f, 100);
        assert_eq!(check(&f.programs, &apps).await.len(), 2);
        // dnf fails: the Flatpak result stays.
        fake_dnf(&f, 1);
        assert_eq!(
            check(&f.programs, &apps).await.keys().collect::<Vec<_>>(),
            ["chat.desktop"]
        );
        // dnf is missing altogether.
        std::fs::remove_file(f.dir.join("dnf")).unwrap();
        assert_eq!(check(&f.programs, &apps).await.len(), 1);
    }

    #[tokio::test]
    async fn one_pkexec_for_all_packages_each_its_own_argument() {
        let f = Fakes::new("lu-upgrade");
        f.set("pkexec", "exit 0");
        f.set("dnf", "exit 0");
        f.set("flatpak", "exit 0");
        let apps = [
            rpm("firefox.desktop", "firefox"),
            rpm("firefox-private.desktop", "firefox"),
            rpm("gimp.desktop", "gimp"),
            flatpak("chat.desktop", "org.example.Chat", Installation::User),
            flatpak("sys.desktop", "org.example.Sys", Installation::System),
        ];
        let stages = std::sync::Mutex::new(Vec::new());
        let results = update(
            &f.programs,
            &apps,
            &ids(&[
                "firefox.desktop",
                "gimp.desktop",
                "firefox-private.desktop",
                "chat.desktop",
                "sys.desktop",
            ]),
            |d, s| stages.lock().unwrap().push(format!("{d} {s}")),
        )
        .await;
        assert!(results.iter().all(|r| r.ok), "{results:?}");
        assert_eq!(results.len(), 5);
        let calls = f.calls();
        let pkexec: Vec<_> = calls.iter().filter(|c| c.starts_with("pkexec ")).collect();
        assert_eq!(pkexec.len(), 1, "{calls:?}");
        assert!(
            pkexec[0].ends_with("/dnf -y upgrade firefox gimp"),
            "{}",
            pkexec[0]
        );
        assert!(
            calls.contains(&"flatpak update --user -y --noninteractive org.example.Chat".into())
        );
        assert!(
            calls.contains(&"flatpak update --system -y --noninteractive org.example.Sys".into())
        );
        let stages = stages.into_inner().unwrap();
        assert!(stages.contains(&"gimp.desktop dnf".into()));
        assert!(stages.contains(&"chat.desktop done".into()));
    }

    #[tokio::test]
    async fn bad_package_names_are_refused_without_a_call() {
        let f = Fakes::new("lu-bad-names");
        f.set("pkexec", "exit 0");
        f.set("dnf", "exit 0");
        let apps = [
            rpm("a.desktop", "two words"),
            rpm("b.desktop", "x;rm"),
            rpm("c.desktop", "-y"),
        ];
        let results = update(
            &f.programs,
            &apps,
            &ids(&["a.desktop", "b.desktop", "c.desktop"]),
            |_, _| {},
        )
        .await;
        assert!(
            results
                .iter()
                .all(|r| !r.ok && r.error.as_deref().unwrap().contains("invalid"))
        );
        assert!(f.calls().is_empty());
    }

    #[tokio::test]
    async fn a_cancelled_password_prompt_cancels_the_packages_only() {
        let f = Fakes::new("lu-cancel");
        f.set("pkexec", "exit 126");
        f.set("dnf", "exit 0");
        f.set("flatpak", "exit 0");
        let apps = [
            rpm("firefox.desktop", "firefox"),
            rpm("gimp.desktop", "gimp"),
            flatpak("chat.desktop", "org.example.Chat", Installation::User),
        ];
        let results = update(
            &f.programs,
            &apps,
            &ids(&["firefox.desktop", "gimp.desktop", "chat.desktop"]),
            |_, _| {},
        )
        .await;
        let by = |d: &str| results.iter().find(|r| r.desktop_id == d).unwrap().clone();
        assert_eq!(by("firefox.desktop").error.as_deref(), Some(CANCELLED));
        assert_eq!(by("gimp.desktop").error.as_deref(), Some(CANCELLED));
        assert!(by("chat.desktop").ok);
        assert!(f.calls().iter().any(|c| c.starts_with("flatpak update")));
    }

    #[test]
    fn tray_counts_each_update_once() {
        let apps = [
            rpm("firefox.desktop", "firefox"),
            rpm("firefox-private.desktop", "firefox"),
            flatpak("chat.desktop", "org.example.Chat", Installation::User),
            rpm("gimp.desktop", "gimp"),
        ];
        let versions: BTreeMap<String, String> = apps
            .iter()
            .map(|a| (a.desktop_id.clone(), "2".to_string()))
            .collect();
        // Chat is a tool in tuxhub that has its own update.
        let pending = Pending::new(versions, &apps, |a| {
            (a.desktop_id == "chat.desktop").then(|| "chat".to_string())
        });
        let tools = BTreeMap::from([
            ("chat".to_string(), "2".to_string()),
            ("autoclicker".to_string(), "1.1".to_string()),
        ]);
        // autoclicker + chat (tools) + firefox + gimp.
        assert_eq!(pending.count_with(&tools), 4);
        assert_eq!(pending.count_with(&BTreeMap::new()), 3);
    }

    #[tokio::test]
    async fn games_appimages_and_unknown_are_updated_elsewhere() {
        let f = Fakes::new("lu-elsewhere");
        let apps = [
            app(
                "game.desktop",
                LocalSource::Game {
                    store: "steam".into(),
                    game: "620".into(),
                },
            ),
            app(
                "img.desktop",
                LocalSource::Appimage {
                    path: "/home/u/x.AppImage".into(),
                },
            ),
            app(
                "sh.desktop",
                LocalSource::Unknown {
                    program: "/bin/sh".into(),
                },
            ),
            app(
                "deb.desktop",
                LocalSource::Deb {
                    package: "foo".into(),
                },
            ),
        ];
        let wanted = ids(&[
            "game.desktop",
            "img.desktop",
            "sh.desktop",
            "deb.desktop",
            "gone.desktop",
        ]);
        let results = update(&f.programs, &apps, &wanted, |_, _| {}).await;
        assert_eq!(results.len(), 5);
        assert!(
            results
                .iter()
                .all(|r| !r.ok && r.error.as_deref() == Some(ELSEWHERE))
        );
        assert!(f.calls().is_empty());
    }
}
