//! Minimaler GitHub-Client für Releases und READMEs.
//!
//! Mit Token (Umgebungsvariable `GITHUB_TOKEN`/`GH_TOKEN` oder `gh auth token`)
//! funktionieren auch private Repos, und das Rate-Limit ist höher.

use serde::Deserialize;
use std::path::Path;
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub size: u64,
}

fn token() -> Option<&'static str> {
    static TOKEN: OnceLock<Option<String>> = OnceLock::new();
    TOKEN
        .get_or_init(|| {
            for var in ["GITHUB_TOKEN", "GH_TOKEN"] {
                if let Ok(t) = std::env::var(var) {
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
            let out = std::process::Command::new("gh").args(["auth", "token"]).output().ok()?;
            let t = String::from_utf8(out.stdout).ok()?.trim().to_string();
            (out.status.success() && !t.is_empty()).then_some(t)
        })
        .as_deref()
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("tuxhub/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP-Client")
    })
}

fn get(url: &str) -> reqwest::RequestBuilder {
    let req = client()
        .get(url)
        .header("X-GitHub-Api-Version", "2022-11-28");
    match token() {
        Some(t) => req.bearer_auth(t),
        None => req,
    }
}

fn check_repo(repo: &str) -> Result<(), String> {
    let ok = repo.split('/').count() == 2
        && repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if ok { Ok(()) } else { Err(format!("ungültiges Repo {repo:?}")) }
}

async fn send(req: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
    let resp = req.send().await.map_err(|e| format!("GitHub nicht erreichbar: {e}"))?;
    match resp.status().as_u16() {
        200..=299 => Ok(resp),
        404 => Err("Nicht gefunden – gibt es ein Release, und hast du Zugriff auf das Repo?".into()),
        403 | 429 => Err("GitHub-Limit erreicht. Mit `gh auth login` anmelden hilft.".into()),
        s => Err(format!("GitHub antwortet mit {s}")),
    }
}

pub async fn latest_release(repo: &str) -> Result<Release, String> {
    check_repo(repo)?;
    send(get(&format!("https://api.github.com/repos/{repo}/releases/latest")).header("Accept", "application/vnd.github+json"))
        .await?
        .json()
        .await
        .map_err(|e| format!("GitHub-Antwort unlesbar: {e}"))
}

pub async fn readme(repo: &str) -> Result<String, String> {
    check_repo(repo)?;
    send(get(&format!("https://api.github.com/repos/{repo}/readme")).header("Accept", "application/vnd.github.raw"))
        .await?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// Eine Datei aus dem Standard-Branch eines Repos.
pub async fn raw_file(repo: &str, path: &str) -> Result<String, String> {
    check_repo(repo)?;
    send(get(&format!("https://api.github.com/repos/{repo}/contents/{path}")).header("Accept", "application/vnd.github.raw"))
        .await?
        .text()
        .await
        .map_err(|e| e.to_string())
}

/// Lädt ein Release-Asset über die API (funktioniert auch bei privaten Repos).
pub async fn download(
    repo: &str,
    asset: &Asset,
    dest: &Path,
    mut on_progress: impl FnMut(u64),
) -> Result<(), String> {
    check_repo(repo)?;
    let mut resp = send(
        get(&format!("https://api.github.com/repos/{repo}/releases/assets/{}", asset.id))
            .header("Accept", "application/octet-stream"),
    )
    .await?;
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| e.to_string())?;
    let mut done = 0u64;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("Download abgebrochen: {e}"))? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        done += chunk.len() as u64;
        on_progress(done);
    }
    file.flush().await.map_err(|e| e.to_string())
}

/// Einfache Wildcard-Suche: `*` passt auf beliebig viele Zeichen.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(pos) => rest = &rest[pos + part.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn globs() {
        assert!(glob_match("*.AppImage", "Heroic-2.18.1-linux-x86_64.AppImage"));
        assert!(glob_match("Heroic-*-x86_64.AppImage", "Heroic-2.18.1-linux-x86_64.AppImage"));
        assert!(!glob_match("*.AppImage", "Heroic-2.18.1.AppImage.blockmap"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "exactly"));
        assert!(!glob_match("a*a", "a"));
        assert!(glob_match("*", "anything"));
    }
}
