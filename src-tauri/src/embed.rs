//! „Im Hub anzeigen“: native Apps auf einem eigenen virtuellen Bildschirm.
//!
//! Wayland erlaubt kein Einbetten fremder Fenster. Deshalb startet der Hub
//! pro Tool eine xpra-Sitzung (eigenes X-Display, eigener D-Bus), lässt die App
//! darin laufen und zeigt sie über den xpra-HTML5-Client im iframe an.
//! Pro Sitzung laufen eigene Portal-Dienste, damit Datei-Dialoge im virtuellen
//! Bildschirm erscheinen, und ein Wächter maximiert das Hauptfenster, damit es
//! den Bereich im Hub ausfüllt. Alles lauscht nur auf 127.0.0.1.

use crate::{Hub, tools};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const WATCHER: &str = include_str!("xpra_watch.py");
const PORTALS_CONF: &str = "[preferred]\ndefault=gtk\n";

struct Session {
    display: u32,
    port: u16,
    xpra: Child,
    watcher: Option<Child>,
}

#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Session>>);

#[derive(Clone, Serialize)]
pub struct EmbedInfo {
    pub url: String,
}

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(cmd))
            .find(|p| p.is_file())
    })
}

/// Erster vorhandener Pfad – Distributionen legen die Portale unterschiedlich ab.
fn first_existing(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|p| Path::new(p).is_file())
        .map(|p| p.to_string())
}

/// Fehlt etwas für „Im Hub anzeigen“? Dann eine Erklärung, sonst `None`.
pub fn missing() -> Option<String> {
    if which("xpra").is_none() {
        return Some(
            "Für „Im Hub anzeigen“ wird xpra gebraucht. Installieren mit:\n\
             Fedora: sudo dnf install xpra\n\
             Ubuntu/Debian: sudo apt install xpra\n\
             Arch: sudo pacman -S xpra"
                .into(),
        );
    }
    if which("python3").is_none() {
        return Some("python3 fehlt".into());
    }
    None
}

fn html5_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = if cfg!(debug_assertions) {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/xpra-html5"))
    } else {
        app.path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("resources/xpra-html5")
    };
    if dir.join("index.html").is_file() {
        Ok(dir)
    } else {
        Err(format!("xpra-Web-Client fehlt in {}", dir.display()))
    }
}

fn free_display(taken: &[u32]) -> Option<u32> {
    (110..400).find(|n| {
        !taken.contains(n)
            && !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists()
            && !Path::new(&format!("/tmp/.X{n}-lock")).exists()
    })
}

fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Startskript, das innerhalb der xpra-Sitzung läuft.
fn run_script(lc: &tools::LaunchCmd, portal_conf_dir: &Path) -> String {
    let mut s = String::from(
        "#!/bin/sh\n# Von tuxhub erzeugt – läuft in der xpra-Sitzung (eigenes DISPLAY und D-Bus).\n\
         unset WAYLAND_DISPLAY\n\
         export GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb SDL_VIDEODRIVER=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1\n",
    );
    let gtk = first_existing(&[
        "/usr/libexec/xdg-desktop-portal-gtk",
        "/usr/lib/xdg-desktop-portal-gtk",
        "/usr/lib/x86_64-linux-gnu/xdg-desktop-portal-gtk",
    ]);
    let portal = first_existing(&[
        "/usr/libexec/xdg-desktop-portal",
        "/usr/lib/xdg-desktop-portal",
        "/usr/lib/x86_64-linux-gnu/xdg-desktop-portal",
    ]);
    if let (Some(gtk), Some(portal)) = (gtk, portal) {
        s += &format!(
            "{gtk} >/dev/null 2>&1 &\nsleep 0.5\n\
             XDG_CONFIG_DIRS={}:\"${{XDG_CONFIG_DIRS:-/etc/xdg}}\" {portal} --replace >/dev/null 2>&1 &\nsleep 1\n",
            sh_quote(&portal_conf_dir.to_string_lossy())
        );
    }
    if let Some(cwd) = &lc.cwd {
        s += &format!("cd {}\n", sh_quote(&cwd.to_string_lossy()));
    }
    let mut args: Vec<String> = lc.args.clone();
    if lc.flatpak {
        // Flatpak-Apps auf X11 zwingen, sonst öffnen sie sich auf dem echten Wayland-Desktop.
        let id = args.pop().unwrap_or_default();
        args.extend(
            [
                "--nosocket=wayland",
                "--socket=x11",
                "--env=GDK_BACKEND=x11",
                "--env=QT_QPA_PLATFORM=xcb",
            ]
            .map(String::from),
        );
        args.push(id);
    }
    s += "exec ";
    s += &sh_quote(&lc.program.to_string_lossy());
    for a in &args {
        s += " ";
        s += &sh_quote(a);
    }
    s += "\n";
    s
}

fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/?floating_menu=false&sound=false")
}

pub async fn start<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<EmbedInfo, String> {
    if let Some(m) = missing() {
        return Err(m);
    }
    let hub = app.state::<Hub>();
    let mut sessions = hub.embeds.0.lock().await;
    if let Some(s) = sessions.get_mut(id) {
        if matches!(s.xpra.try_wait(), Ok(None)) {
            return Ok(EmbedInfo { url: url(s.port) });
        }
        sessions.remove(id);
    }

    let inst = tools::load(&hub.tools_dir, id)?;
    let lc = tools::launch_cmd(&hub.tools_dir, &inst)?;
    let base = hub
        .tools_dir
        .parent()
        .unwrap_or(&hub.tools_dir)
        .join("embed");
    let dir = base.join(id);
    let conf = base.join("portalconf");
    std::fs::create_dir_all(conf.join("xdg-desktop-portal")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(conf.join("xdg-desktop-portal/portals.conf"), PORTALS_CONF)
        .map_err(|e| e.to_string())?;
    let watcher_py = base.join("xpra_watch.py");
    std::fs::write(&watcher_py, WATCHER).map_err(|e| e.to_string())?;
    let run_sh = dir.join("run.sh");
    std::fs::write(&run_sh, run_script(&lc, &conf)).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&run_sh, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;

    let taken: Vec<u32> = sessions.values().map(|s| s.display).collect();
    let display = free_display(&taken).ok_or("Kein freies X-Display gefunden")?;
    let port = free_port()?;
    let html5 = html5_dir(app)?;
    let log = std::fs::File::create(dir.join("xpra.log")).map_err(|e| e.to_string())?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;

    let xpra = Command::new("xpra")
        .arg("start")
        .arg(format!(":{display}"))
        .arg("--daemon=no")
        .arg(format!("--bind-tcp=127.0.0.1:{port}"))
        .arg(format!("--html={}", html5.display()))
        .arg(format!("--start-child={}", run_sh.display()))
        .args([
            "--exit-with-children=yes",
            "--dbus=yes",
            "--mdns=no",
            "--pulseaudio=no",
            "--notifications=no",
            "--systemd-run=no",
            "--webcam=no",
            "--printing=no",
            "--file-transfer=no",
            "--tray=no",
            "--splash=no",
        ])
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log2)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("xpra konnte nicht gestartet werden: {e}"))?;
    let mut session = Session {
        display,
        port,
        xpra,
        watcher: None,
    };

    // Warten, bis der Web-Client erreichbar ist.
    let mut ready = false;
    for _ in 0..80 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            ready = true;
            break;
        }
        if !matches!(session.xpra.try_wait(), Ok(None)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !ready {
        let _ = session.xpra.kill().await;
        return Err(format!(
            "Die Sitzung ist nicht gestartet. Details: {}",
            dir.join("xpra.log").display()
        ));
    }

    session.watcher = Command::new("python3")
        .arg(&watcher_py)
        .arg(format!(":{display}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok();
    sessions.insert(id.to_string(), session);
    Ok(EmbedInfo { url: url(port) })
}

pub async fn status<R: Runtime>(app: &AppHandle<R>, id: &str) -> Option<EmbedInfo> {
    let hub = app.state::<Hub>();
    let mut sessions = hub.embeds.0.lock().await;
    let s = sessions.get_mut(id)?;
    if matches!(s.xpra.try_wait(), Ok(None)) {
        Some(EmbedInfo { url: url(s.port) })
    } else {
        // App wurde geschlossen → Sitzung ist von selbst zu Ende.
        sessions.remove(id);
        None
    }
}

pub async fn stop<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let session = app.state::<Hub>().embeds.0.lock().await.remove(id);
    if let Some(s) = session {
        shutdown(s).await;
    }
}

pub async fn stop_all<R: Runtime>(app: &AppHandle<R>) {
    let all: Vec<Session> = app
        .state::<Hub>()
        .embeds
        .0
        .lock()
        .await
        .drain()
        .map(|(_, s)| s)
        .collect();
    for s in all {
        shutdown(s).await;
    }
}

async fn shutdown(mut s: Session) {
    // Sauber beenden lassen (räumt Display und Kinder auf), sonst hart.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new("xpra")
            .arg("stop")
            .arg(format!(":{}", s.display))
            .output(),
    )
    .await;
    if tokio::time::timeout(Duration::from_secs(3), s.xpra.wait())
        .await
        .is_err()
    {
        let _ = s.xpra.kill().await;
    }
    if let Some(mut w) = s.watcher {
        let _ = w.kill().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lc(flatpak: bool) -> tools::LaunchCmd {
        if flatpak {
            tools::LaunchCmd {
                program: "flatpak".into(),
                args: vec!["run".into(), "net.lutris.Lutris".into()],
                cwd: None,
                flatpak: true,
            }
        } else {
            tools::LaunchCmd {
                program: "/opt/my app/it's.AppImage".into(),
                args: vec!["--flag".into()],
                cwd: Some("/opt/my app".into()),
                flatpak: false,
            }
        }
    }

    #[test]
    fn script_quotes_and_forces_x11() {
        let s = run_script(&lc(false), Path::new("/tmp/conf"));
        assert!(s.contains("unset WAYLAND_DISPLAY"));
        assert!(s.contains("cd '/opt/my app'"));
        assert!(s.contains(r"exec '/opt/my app/it'\''s.AppImage' '--flag'"));

        let f = run_script(&lc(true), Path::new("/tmp/conf"));
        assert!(f.contains("'run' '--nosocket=wayland' '--socket=x11'"));
        assert!(f.trim_end().ends_with("'net.lutris.Lutris'"));
    }

    /// Startet den installierten GIF Color Changer wirklich „im Hub“.
    /// Braucht xpra und das installierte Tool: `cargo test -- --ignored embed_real`
    #[tokio::test]
    #[ignore]
    async fn embed_real_app() {
        let home = std::env::var("HOME").unwrap();
        let real_tools = PathBuf::from(home).join(".local/share/dev.tuxhub.app/tools");
        let app = tauri::test::mock_app();
        let base = std::env::temp_dir().join(format!("tuxhub-embed-{}", std::process::id()));
        let tools_dir = base.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        // Nur den tool.json-Eintrag kopieren, die Dateien bleiben am Original.
        let inst =
            tools::load(&real_tools, "gif-color-changer").expect("GIF Color Changer installieren");
        let root = inst.root_dir(&real_tools).unwrap();
        let link = tools_dir.join("gif-color-changer");
        std::fs::create_dir_all(&link).unwrap();
        std::os::unix::fs::symlink(
            real_tools.join("gif-color-changer/files"),
            link.join("files"),
        )
        .unwrap();
        std::fs::copy(
            real_tools.join("gif-color-changer/tool.json"),
            link.join("tool.json"),
        )
        .unwrap();
        assert!(root.exists());
        app.manage(crate::Hub::for_test(tools_dir, base.join("settings")));

        let info = start(app.handle(), "gif-color-changer")
            .await
            .expect("Sitzung startet");
        println!("URL: {}", info.url);
        let body = reqwest::get(&info.url).await.unwrap().text().await.unwrap();
        assert!(
            body.to_lowercase().contains("xpra"),
            "Web-Client wird ausgeliefert"
        );
        assert!(status(app.handle(), "gif-color-changer").await.is_some());
        if std::env::var("TUXHUB_KEEP").is_ok() {
            println!("Sitzung bleibt 60 s offen …");
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
        stop(app.handle(), "gif-color-changer").await;
        assert!(status(app.handle(), "gif-color-changer").await.is_none());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn display_and_port() {
        let d = free_display(&[110, 111]).unwrap();
        assert!(d >= 110 && d != 110 && d != 111);
        assert!(free_port().unwrap() > 0);
    }
}
