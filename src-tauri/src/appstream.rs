//! Reads AppStream metadata (Flathub collections and distro metainfo files)
//! to enrich apps found in the start menu.

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_GZ_BYTES: u64 = 64 * 1024 * 1024;
const MAX_METAINFO_BYTES: u64 = 1024 * 1024;
const COMPONENT_TYPES: [&str; 3] = ["desktop-application", "desktop", "console-application"];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppInfo {
    pub id: String,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub homepage: Option<String>,
    /// Every `<url>` value, including `vcs-browser` and `bugtracker`.
    pub urls: Vec<String>,
}

/// Texts of one element in several languages (`""` = untranslated).
type ByLang = HashMap<String, String>;

#[derive(Default)]
struct Draft {
    id: String,
    name: ByLang,
    summary: ByLang,
    description: ByLang,
    icons: Vec<(String, String)>,
    urls: Vec<(String, String)>,
    launchables: Vec<String>,
}

struct Frame {
    name: String,
    lang: Option<String>,
    attrs: Vec<(String, String)>,
    text: String,
}

/// Language candidates in priority order: `de_DE.UTF-8` gives `de_DE`, then `de`.
fn lang_candidates(lang: &str) -> Vec<String> {
    let base = lang.split(['.', '@']).next().unwrap_or("");
    let full = base.replace('-', "_");
    let short = full.split('_').next().unwrap_or("").to_string();
    let mut out = Vec::new();
    for c in [full, short] {
        if !c.is_empty() && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

fn pick(map: &ByLang, cands: &[String]) -> Option<String> {
    cands
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(""))
        .filter_map(|l| map.get(l))
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map(String::from)
}

fn ensure_gap(buf: &mut String, gap: &str) {
    if buf.is_empty() || buf.ends_with(gap) {
        return;
    }
    let keep = buf.trim_end_matches('\n').len();
    buf.truncate(keep);
    buf.push_str(gap);
}

fn frame_from(e: &BytesStart, parent_lang: Option<&String>) -> Frame {
    let attrs: Vec<(String, String)> = e
        .attributes()
        .flatten()
        .filter_map(|a| {
            Some((
                a.key.as_ref().to_string(),
                a.normalized_value(XmlVersion::Implicit1_0)
                    .ok()?
                    .into_owned(),
            ))
        })
        .collect();
    let own = attrs
        .iter()
        .find(|(k, _)| k == "xml:lang")
        .map(|(_, v)| v.clone());
    Frame {
        name: e.name().as_ref().to_string(),
        lang: own.or_else(|| parent_lang.cloned()),
        attrs,
        text: String::new(),
    }
}

struct Parser {
    stack: Vec<Frame>,
    cur: Option<Draft>,
    /// Stack length while inside the open component (its own frame included).
    comp_depth: usize,
    done: Vec<Draft>,
}

impl Parser {
    fn description_buf(&mut self) -> Option<&mut String> {
        let depth = self.comp_depth;
        let lang = self.stack.last()?.lang.clone().unwrap_or_default();
        if self.stack.get(depth)?.name != "description" {
            return None;
        }
        Some(self.cur.as_mut()?.description.entry(lang).or_default())
    }

    fn start(&mut self, e: &BytesStart) {
        let frame = frame_from(e, self.stack.last().and_then(|f| f.lang.as_ref()));
        if self.cur.is_none() && frame.name == "component" {
            let ty = frame
                .attrs
                .iter()
                .find(|(k, _)| k == "type")
                .map(|(_, v)| v.as_str());
            if ty.is_some_and(|t| COMPONENT_TYPES.contains(&t)) {
                self.cur = Some(Draft::default());
                self.comp_depth = self.stack.len() + 1;
            }
        }
        let name = frame.name.clone();
        self.stack.push(frame);
        if self.cur.is_some() {
            let gap = match name.as_str() {
                "p" | "ul" | "ol" => Some("\n\n"),
                "li" => Some("\n"),
                _ => None,
            };
            if let (Some(gap), Some(buf)) = (gap, self.description_buf()) {
                ensure_gap(buf, gap);
                if name == "li" {
                    buf.push_str("- ");
                }
            }
        }
    }

    fn text(&mut self, t: &str) {
        if self.cur.is_none() {
            return;
        }
        let in_block = self
            .stack
            .get(self.comp_depth..)
            .is_some_and(|s| s.iter().any(|f| f.name == "p" || f.name == "li"));
        if let Some(buf) = self.description_buf() {
            if in_block {
                // Collapse source whitespace but keep single spaces between words.
                let starts = t.starts_with(char::is_whitespace);
                let ends = t.ends_with(char::is_whitespace);
                let words = t.split_whitespace().collect::<Vec<_>>().join(" ");
                if words.is_empty() {
                    if !buf.is_empty() && !buf.ends_with([' ', '\n']) {
                        buf.push(' ');
                    }
                    return;
                }
                if starts && !buf.is_empty() && !buf.ends_with([' ', '\n']) {
                    buf.push(' ');
                }
                buf.push_str(&words);
                if ends {
                    buf.push(' ');
                }
            }
            return;
        }
        if let Some(f) = self.stack.last_mut() {
            f.text.push_str(t);
        }
    }

    fn end(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let Some(d) = self.cur.as_mut() else { return };
        if self.stack.len() + 1 == self.comp_depth && frame.name == "component" {
            if let Some(d) = self.cur.take()
                && !d.id.is_empty()
            {
                self.done.push(d);
            }
            return;
        }
        if self.stack.len() != self.comp_depth {
            return; // only direct children of the component carry data
        }
        let value = frame.text.trim().to_string();
        let lang = frame.lang.clone().unwrap_or_default();
        let attr = |k: &str| {
            frame
                .attrs
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
        };
        match frame.name.as_str() {
            "id" => d.id = value,
            "name" => {
                d.name.entry(lang).or_insert(value);
            }
            "summary" => {
                d.summary.entry(lang).or_insert(value);
            }
            "icon" if !value.is_empty() => d.icons.push((attr("type").unwrap_or_default(), value)),
            "url" if !value.is_empty() => d.urls.push((attr("type").unwrap_or_default(), value)),
            "launchable" if attr("type").as_deref() == Some("desktop-id") && !value.is_empty() => {
                d.launchables.push(value);
            }
            _ => {}
        }
    }
}

fn finish(d: Draft, cands: &[String]) -> (AppInfo, Vec<String>) {
    let icon = d
        .icons
        .iter()
        .find(|(t, _)| t == "stock")
        .or_else(|| d.icons.first())
        .map(|(_, v)| v.clone());
    let homepage = d
        .urls
        .iter()
        .find(|(t, _)| t == "homepage")
        .map(|(_, v)| v.clone());
    let description = d
        .description
        .iter()
        .map(|(l, v)| (l.clone(), v.trim().to_string()))
        .collect::<ByLang>();
    let info = AppInfo {
        id: d.id,
        name: pick(&d.name, cands),
        summary: pick(&d.summary, cands),
        description: pick(&description, cands),
        icon,
        homepage,
        urls: d.urls.into_iter().map(|(_, v)| v).collect(),
    };
    (info, d.launchables)
}

fn parse_full(xml: &[u8], lang: &str) -> Vec<(AppInfo, Vec<String>)> {
    let cands = lang_candidates(lang);
    let mut reader = Reader::from_reader(xml);
    let mut p = Parser {
        stack: Vec::new(),
        cur: None,
        comp_depth: 0,
        done: Vec::new(),
    };
    loop {
        match reader.read_event() {
            Err(_) => return Vec::new(),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => p.start(&e),
            Ok(Event::Empty(e)) => {
                p.start(&e);
                p.end();
            }
            Ok(Event::End(_)) => p.end(),
            Ok(Event::Text(t)) => p.text(&t.xml_content(XmlVersion::Implicit1_0)),
            Ok(Event::CData(t)) => p.text(&t.into_inner()),
            Ok(Event::GeneralRef(r)) => {
                let resolved = if r.is_char_ref() {
                    match r.resolve_char_ref() {
                        Ok(Some(c)) => c.to_string(),
                        _ => return Vec::new(),
                    }
                } else {
                    quick_xml::escape::resolve_predefined_entity(&r)
                        .map(String::from)
                        .unwrap_or_default()
                };
                p.text(&resolved);
            }
            Ok(_) => {}
        }
    }
    if !p.stack.is_empty() {
        return Vec::new(); // truncated document
    }
    p.done.into_iter().map(|d| finish(d, &cands)).collect()
}

/// Parses an AppStream collection (Flathub) or a single metainfo file.
/// Broken or truncated XML yields an empty list.
pub fn parse(xml: &[u8], lang: &str) -> Vec<AppInfo> {
    parse_full(xml, lang).into_iter().map(|(a, _)| a).collect()
}

/// `owner/repo` of the first `https://github.com/<owner>/<repo>[/...]` URL.
pub fn github_repo(urls: &[String]) -> Option<String> {
    urls.iter().find_map(|u| {
        let rest = u.strip_prefix("https://github.com/")?;
        let mut parts = rest.split(['/', '?', '#']);
        let owner = parts.next()?;
        let repo = parts.next()?.trim_end_matches(".git");
        let ok = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        (ok(owner) && ok(repo)).then(|| format!("{owner}/{repo}"))
    })
}

/// Reads at most `max` bytes; `None` when the source is larger or unreadable.
fn read_limited(reader: impl Read, max: u64) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    reader.take(max + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= max).then_some(buf)
}

fn read_file(path: &Path, gz: bool, max_gz: u64, max_meta: u64) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if gz {
        read_limited(flate2::read::GzDecoder::new(file), max_gz)
    } else {
        read_limited(file, max_meta)
    }
}

fn build_index(
    gz: &[PathBuf],
    meta: &[PathBuf],
    max_gz: u64,
    max_meta: u64,
    lang: &str,
) -> HashMap<String, AppInfo> {
    let mut index = HashMap::new();
    let sources = gz
        .iter()
        .map(|p| (p, true))
        .chain(meta.iter().map(|p| (p, false)));
    for (path, is_gz) in sources {
        let Some(xml) = read_file(path, is_gz, max_gz, max_meta) else {
            continue;
        };
        for (info, launchables) in parse_full(&xml, lang) {
            let bare = info
                .id
                .strip_suffix(".desktop")
                .unwrap_or(&info.id)
                .to_string();
            let keys = [bare.clone(), format!("{bare}.desktop"), info.id.clone()]
                .into_iter()
                .chain(launchables);
            for key in keys {
                index.entry(key).or_insert_with(|| info.clone());
            }
        }
    }
    index
}

fn files_in(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == suffix) && p.is_file())
        .collect();
    out.sort();
    out
}

fn system_language() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .unwrap_or_else(|| "en".into())
}

/// Index of installed apps' AppStream data. Keys are component IDs with and
/// without `.desktop` plus launchable IDs. Unreadable or oversized files are skipped.
pub fn load_index() -> HashMap<String, AppInfo> {
    let mut gz = Vec::new();
    let mut roots = vec![PathBuf::from("/var/lib/flatpak/appstream")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/flatpak/appstream"));
    }
    for root in roots {
        let mut remotes: Vec<PathBuf> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        remotes.sort();
        for remote in remotes {
            let f = remote.join("x86_64/active/appstream.xml.gz");
            if f.is_file() {
                gz.push(f);
            }
        }
    }
    let mut meta = files_in(Path::new("/usr/share/metainfo"), "xml");
    meta.extend(files_in(Path::new("/usr/share/appdata"), "xml"));
    build_index(
        &gz,
        &meta,
        MAX_GZ_BYTES,
        MAX_METAINFO_BYTES,
        &system_language(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLLECTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<components version="0.16" origin="flathub">
  <component type="desktop-application">
    <id>com.core447.StreamController</id>
    <name>StreamController</name>
    <summary>Control your Stream Deck</summary>
    <description>
      <p>First paragraph &amp; more.</p>
      <ul><li>One</li><li>Two</li></ul>
      <p>Last.</p>
    </description>
    <icon type="cached" width="64" height="64">com.core447.StreamController.png</icon>
    <url type="homepage">https://example.org/sc</url>
    <url type="vcs-browser">https://github.com/StreamController/StreamController</url>
    <url type="bugtracker">https://github.com/StreamController/StreamController/issues</url>
  </component>
  <component type="desktop-application">
    <id>org.example.Other</id>
    <name>Other</name>
  </component>
</components>"#;

    #[test]
    fn parses_collection() {
        let apps = parse(COLLECTION.as_bytes(), "en");
        assert_eq!(apps.len(), 2);
        let sc = &apps[0];
        assert_eq!(sc.id, "com.core447.StreamController");
        assert_eq!(sc.name.as_deref(), Some("StreamController"));
        assert_eq!(sc.summary.as_deref(), Some("Control your Stream Deck"));
        assert_eq!(
            sc.description.as_deref(),
            Some("First paragraph & more.\n\n- One\n- Two\n\nLast.")
        );
        assert_eq!(sc.icon.as_deref(), Some("com.core447.StreamController.png"));
        assert_eq!(sc.homepage.as_deref(), Some("https://example.org/sc"));
        assert_eq!(sc.urls.len(), 3);
        assert_eq!(
            github_repo(&sc.urls).as_deref(),
            Some("StreamController/StreamController")
        );
        assert_eq!(apps[1].id, "org.example.Other");
        assert!(apps[1].urls.is_empty());
    }

    const METAINFO: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>org.kde.dolphin</id>
  <name>Dolphin</name>
  <developer id="kde.org"><name>KDE</name></developer>
  <launchable type="desktop-id">org.kde.dolphin.desktop</launchable>
</component>"#;

    #[test]
    fn parses_metainfo_and_launchable() {
        let dir = temp("metainfo");
        let f = dir.join("org.kde.dolphin.metainfo.xml");
        std::fs::write(&f, METAINFO).unwrap();
        let apps = parse(METAINFO.as_bytes(), "en");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name.as_deref(), Some("Dolphin"));
        let idx = build_index(&[], &[f], MAX_GZ_BYTES, MAX_METAINFO_BYTES, "en");
        assert_eq!(idx["org.kde.dolphin"].name.as_deref(), Some("Dolphin"));
        assert_eq!(idx["org.kde.dolphin.desktop"].id, "org.kde.dolphin");
    }

    #[test]
    fn prefers_language() {
        let xml = r#"<component type="desktop-application"><id>a.b</id>
<summary>English</summary><summary xml:lang="de">Deutsch</summary></component>"#;
        assert_eq!(
            parse(xml.as_bytes(), "de")[0].summary.as_deref(),
            Some("Deutsch")
        );
        assert_eq!(
            parse(xml.as_bytes(), "de_DE.UTF-8")[0].summary.as_deref(),
            Some("Deutsch")
        );
        assert_eq!(
            parse(xml.as_bytes(), "fr")[0].summary.as_deref(),
            Some("English")
        );
    }

    #[test]
    fn github_repo_rejects_others() {
        let u = |s: &str| vec![s.to_string()];
        assert_eq!(github_repo(&u("https://gitlab.com/a/b")), None);
        assert_eq!(github_repo(&u("https://github.com/a")), None);
        assert_eq!(
            github_repo(&u("https://github.com/a/b/issues")).as_deref(),
            Some("a/b")
        );
        assert_eq!(
            github_repo(&u("https://github.com/a/b.git")).as_deref(),
            Some("a/b")
        );
        let both = vec![
            "https://gitlab.com/a/b".to_string(),
            "https://github.com/c/d".to_string(),
        ];
        assert_eq!(github_repo(&both).as_deref(), Some("c/d"));
    }

    #[test]
    fn broken_xml_is_empty() {
        let cut = &COLLECTION[..COLLECTION.len() - 120];
        assert!(parse(cut.as_bytes(), "en").is_empty());
        assert!(parse(b"<<<not xml", "en").is_empty());
        assert!(parse(b"", "en").is_empty());
    }

    fn temp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("tuxhub-appstream-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn gzip(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn reads_gz_collection() {
        let dir = temp("gz");
        let f = dir.join("appstream.xml.gz");
        std::fs::write(&f, gzip(COLLECTION.as_bytes())).unwrap();
        let idx = build_index(&[f], &[], MAX_GZ_BYTES, MAX_METAINFO_BYTES, "en");
        assert!(idx.contains_key("com.core447.StreamController"));
        assert!(idx.contains_key("org.example.Other.desktop"));
    }

    #[test]
    fn over_limit_files_are_skipped() {
        let dir = temp("limits");
        let gz = dir.join("appstream.xml.gz");
        std::fs::write(&gz, gzip(COLLECTION.as_bytes())).unwrap();
        let meta = dir.join("big.metainfo.xml");
        std::fs::write(&meta, METAINFO).unwrap();
        // limits just below the real sizes
        let gz_len = COLLECTION.len() as u64;
        let meta_len = METAINFO.len() as u64;
        assert!(build_index(std::slice::from_ref(&gz), &[], gz_len - 1, meta_len, "en").is_empty());
        assert!(
            build_index(&[], std::slice::from_ref(&meta), gz_len, meta_len - 1, "en").is_empty()
        );
        // exactly at the limit is still read
        assert!(!build_index(&[gz], &[], gz_len, meta_len, "en").is_empty());
        assert!(!build_index(&[], &[meta], gz_len, meta_len, "en").is_empty());
    }
}
