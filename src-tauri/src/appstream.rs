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
    /// One document per `<description>` element (its own `xml:lang`, blocks).
    descriptions: Vec<(Option<String>, Vec<Block>)>,
    icons: Vec<(String, String)>,
    urls: Vec<(String, String)>,
    launchables: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum BlockKind {
    Para,
    Item,
}

/// A paragraph or list item with its translations (`""` = untranslated).
struct Block {
    kind: BlockKind,
    list: usize,
    texts: ByLang,
}

struct Frame {
    name: String,
    lang: Option<String>,
    own_lang: Option<String>,
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

/// Renders the blocks of one description in the wanted language. A block
/// without a translation falls back to its untranslated text.
fn render_blocks(blocks: &[Block], cands: &[String]) -> String {
    let mut out = String::new();
    let mut prev: Option<(BlockKind, usize)> = None;
    for b in blocks {
        let Some(text) = pick(&b.texts, cands) else {
            continue;
        };
        match prev {
            None => {}
            Some((BlockKind::Item, l)) if b.kind == BlockKind::Item && l == b.list => {
                out.push('\n')
            }
            Some(_) => out.push_str("\n\n"),
        }
        if b.kind == BlockKind::Item {
            out.push_str("- ");
        }
        out.push_str(&text);
        prev = Some((b.kind, b.list));
    }
    out
}

/// Description document for the language: a whole `<description xml:lang>`
/// wins over the untranslated one.
fn pick_description(docs: &[(Option<String>, Vec<Block>)], cands: &[String]) -> Option<String> {
    let doc = cands
        .iter()
        .find_map(|c| docs.iter().find(|(l, _)| l.as_deref() == Some(c.as_str())))
        .or_else(|| docs.iter().find(|(l, _)| l.is_none()))?;
    let text = render_blocks(&doc.1, cands);
    (!text.is_empty()).then_some(text)
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
        lang: own.clone().or_else(|| parent_lang.cloned()),
        own_lang: own,
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
    /// Number of lists seen in the open component (tells list items apart).
    lists: usize,
}

impl Parser {
    /// The open `<description>` document, when the stack is inside one.
    fn description_doc(&mut self) -> Option<&mut Vec<Block>> {
        if self.stack.get(self.comp_depth)?.name != "description" {
            return None;
        }
        self.cur.as_mut()?.descriptions.last_mut().map(|(_, b)| b)
    }

    fn open_block(&mut self, kind: BlockKind, own_lang: Option<String>) {
        let list = self.lists;
        let Some(blocks) = self.description_doc() else {
            return;
        };
        let lang = own_lang.clone().unwrap_or_default();
        // A block with its own language translates the previous block of its kind.
        if own_lang.is_some()
            && let Some(last) = blocks.last_mut()
            && last.kind == kind
            && !last.texts.contains_key(&lang)
        {
            last.texts.insert(lang, String::new());
            return;
        }
        blocks.push(Block {
            kind,
            list,
            texts: ByLang::from([(lang, String::new())]),
        });
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
                self.lists = 0;
            }
        }
        let name = frame.name.clone();
        self.stack.push(frame);
        if self.cur.is_some() && self.stack.len() == self.comp_depth + 1 && name == "description" {
            let own = self.stack.last().and_then(|f| f.own_lang.clone());
            if let Some(d) = self.cur.as_mut() {
                d.descriptions.push((own, Vec::new()));
            }
        } else if self.cur.is_some() && self.stack.len() > self.comp_depth + 1 {
            let own = self.stack.last().and_then(|f| f.own_lang.clone());
            let in_block = self.stack[self.comp_depth + 1..self.stack.len() - 1]
                .iter()
                .any(|f| f.name == "p" || f.name == "li");
            match name.as_str() {
                "ul" | "ol" => self.lists += 1,
                "p" if !in_block => self.open_block(BlockKind::Para, own),
                "li" if !in_block => self.open_block(BlockKind::Item, own),
                _ => {}
            }
        }
    }

    fn text(&mut self, t: &str) {
        if self.cur.is_none() {
            return;
        }
        if self
            .stack
            .get(self.comp_depth)
            .is_some_and(|f| f.name == "description")
        {
            // Text belongs to the innermost p/li; its language is the block's own one.
            let Some(idx) = self.stack[self.comp_depth..]
                .iter()
                .position(|f| f.name == "p" || f.name == "li")
            else {
                return;
            };
            let lang = self.stack[self.comp_depth + idx]
                .own_lang
                .clone()
                .unwrap_or_default();
            let Some(buf) = self
                .description_doc()
                .and_then(|b| b.last_mut())
                .and_then(|b| b.texts.get_mut(&lang))
            else {
                return;
            };
            // Collapse source whitespace but keep single spaces between words.
            let starts = t.starts_with(char::is_whitespace);
            let ends = t.ends_with(char::is_whitespace);
            let words = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if words.is_empty() {
                if !buf.is_empty() && !buf.ends_with(' ') {
                    buf.push(' ');
                }
                return;
            }
            if starts && !buf.is_empty() && !buf.ends_with(' ') {
                buf.push(' ');
            }
            buf.push_str(&words);
            if ends {
                buf.push(' ');
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
    // Never a remote icon: nothing is fetched when the workshop opens.
    let icon = ["stock", "cached", "local"].iter().find_map(|ty| {
        d.icons
            .iter()
            .find(|(t, v)| t == ty && !v.contains("://"))
            .map(|(_, v)| v.clone())
    });
    let homepage = d
        .urls
        .iter()
        .find(|(t, _)| t == "homepage")
        .map(|(_, v)| v.clone());
    let description = pick_description(&d.descriptions, cands);
    let info = AppInfo {
        id: d.id,
        name: pick(&d.name, cands),
        summary: pick(&d.summary, cands),
        description,
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
        lists: 0,
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
/// Broken or truncated XML yields an empty list. The index uses
/// [`parse_full`], which also keeps the launchables.
#[cfg(test)]
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

    const INLINE_TRANSLATIONS: &str = r#"<component type="desktop-application"><id>a.b</id>
<description>
  <p>English one</p>
  <p xml:lang="de">Deutsch eins</p>
  <p>English two</p>
  <ul>
    <li>Item A</li>
    <li xml:lang="de">Punkt A</li>
    <li>Item B</li>
  </ul>
  <p>Closing</p>
</description></component>"#;

    #[test]
    fn inline_translations_keep_untranslated_blocks() {
        let de = parse(INLINE_TRANSLATIONS.as_bytes(), "de")[0]
            .description
            .clone();
        assert_eq!(
            de.as_deref(),
            Some("Deutsch eins\n\nEnglish two\n\n- Punkt A\n- Item B\n\nClosing")
        );
        let en = parse(INLINE_TRANSLATIONS.as_bytes(), "en")[0]
            .description
            .clone();
        assert_eq!(
            en.as_deref(),
            Some("English one\n\nEnglish two\n\n- Item A\n- Item B\n\nClosing")
        );
    }

    #[test]
    fn whole_translated_description_wins() {
        let xml = r#"<component type="desktop-application"><id>a.b</id>
<description><p>English</p></description>
<description xml:lang="de"><p>Deutsch</p><ul><li>Eins</li></ul></description></component>"#;
        assert_eq!(
            parse(xml.as_bytes(), "de")[0].description.as_deref(),
            Some("Deutsch\n\n- Eins")
        );
        assert_eq!(
            parse(xml.as_bytes(), "en")[0].description.as_deref(),
            Some("English")
        );
    }

    #[test]
    fn never_uses_remote_icons() {
        let xml = |icons: &str| {
            format!(r#"<component type="desktop-application"><id>a.b</id>{icons}</component>"#)
        };
        let remote = r#"<icon type="remote">https://example.org/i.png</icon>"#;
        assert_eq!(parse(xml(remote).as_bytes(), "en")[0].icon, None);
        let all = format!(
            r#"{remote}<icon type="local">/x/l.png</icon><icon type="cached">c.png</icon><icon type="stock">st</icon>"#
        );
        assert_eq!(
            parse(xml(&all).as_bytes(), "en")[0].icon.as_deref(),
            Some("st")
        );
        let no_stock = format!(
            r#"{remote}<icon type="local">/x/l.png</icon><icon type="cached">c.png</icon>"#
        );
        assert_eq!(
            parse(xml(&no_stock).as_bytes(), "en")[0].icon.as_deref(),
            Some("c.png")
        );
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
