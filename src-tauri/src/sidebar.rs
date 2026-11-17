//! Order of the sidebar: `<config>/sidebar.json` holds the ids of the pinned
//! tools in the order the user gave them. Which tools are pinned is told by
//! their records (`Installed::pinned`); tools that announce themselves and
//! have no record of tuxhub are taken out through `hidden`.

use crate::manifest::valid_id;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Largest `sidebar.json` that is read.
const MAX_SIZE: u64 = 64 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sidebar {
    /// Pinned tool ids, top first.
    #[serde(default)]
    pub order: Vec<String>,
    /// Announced tools (no record of tuxhub) the user took out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<String>,
}

pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join("sidebar.json")
}

/// The saved sidebar, its ids checked; `None` when the file is missing, too
/// large or broken (it is then replaced on the next save).
pub fn load(path: &Path) -> Option<Sidebar> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_SIZE {
        eprintln!("{}: too large, ignored", path.display());
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let mut sidebar: Sidebar = match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return None;
        }
    };
    sidebar.order.retain(|id| valid_id(id));
    sidebar.hidden.retain(|id| valid_id(id));
    Some(sidebar)
}

/// Writes the sidebar to a temporary file and moves it over the old one, so
/// a crash never leaves half a file.
pub fn save(path: &Path, sidebar: &Sidebar) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(sidebar).unwrap())
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// The order the sidebar shows `pinned` (id, name) in: the saved order
/// without ids that are no longer pinned, then the pinned ones it does not
/// name yet by name. Without a saved order all are sorted by name.
pub fn arrange(saved: Option<&[String]>, pinned: &[(String, String)]) -> Vec<String> {
    let known = |id: &String| pinned.iter().any(|(p, _)| p == id);
    let mut order: Vec<String> = Vec::new();
    for id in saved.unwrap_or_default() {
        if known(id) && !order.contains(id) {
            order.push(id.clone());
        }
    }
    let mut rest: Vec<&(String, String)> = pinned
        .iter()
        .filter(|(id, _)| !order.contains(id))
        .collect();
    rest.sort_by_cached_key(|(id, name)| (name.to_lowercase(), id.clone()));
    for (id, _) in rest {
        if !order.contains(id) {
            order.push(id.clone());
        }
    }
    order
}

/// Accepts `new` only as a reordering of `current`: the same ids, each once.
pub fn check_order(current: &[String], new: &[String]) -> Result<(), String> {
    let mut a = current.to_vec();
    let mut b = new.to_vec();
    a.sort();
    b.sort();
    let unique = b.windows(2).all(|w| w[0] != w[1]);
    if a == b && unique {
        Ok(())
    } else {
        Err("order does not match the sidebar".into())
    }
}

/// Inserts the ids that are not in `order` yet at `at` (the end when it is
/// past it or none), in the given order. Ids already there stay where they are.
pub fn insert(order: &mut Vec<String>, ids: &[String], at: Option<usize>) {
    let mut at = at.unwrap_or(order.len()).min(order.len());
    for id in ids {
        if !order.contains(id) {
            order.insert(at, id.clone());
            at += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn pinned(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(i, n)| (i.to_string(), n.to_string()))
            .collect()
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tuxhub-sidebar-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        path(&dir)
    }

    #[test]
    fn without_a_file_the_sidebar_is_sorted_by_name() {
        let p = temp("missing");
        assert_eq!(load(&p), None);
        let tools = pinned(&[("zed", "Zed"), ("b", "beta"), ("a", "Alpha")]);
        assert_eq!(arrange(None, &tools), ids(&["a", "b", "zed"]));
    }

    #[test]
    fn unknown_ids_go_and_missing_pinned_ones_follow_by_name() {
        let tools = pinned(&[
            ("a", "Alpha"),
            ("c", "Charlie"),
            ("b", "Bravo"),
            ("d", "Delta"),
        ]);
        let saved = ids(&["c", "gone", "a", "c"]);
        assert_eq!(arrange(Some(&saved), &tools), ids(&["c", "a", "b", "d"]));
    }

    #[test]
    fn a_broken_or_oversized_file_counts_as_missing_and_is_replaced() {
        let p = temp("broken");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(load(&p), None);
        let s = Sidebar {
            order: ids(&["a"]),
            hidden: vec![],
        };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), Some(s));
        assert!(!p.with_extension("json.tmp").exists());
        std::fs::write(&p, " ".repeat(MAX_SIZE as usize + 1)).unwrap();
        assert_eq!(load(&p), None);
    }

    #[test]
    fn invalid_ids_in_the_file_are_dropped() {
        let p = temp("invalid");
        std::fs::write(
            &p,
            r#"{"order":["ok","../etc","UPPER",""],"hidden":["x/y","h"]}"#,
        )
        .unwrap();
        let s = load(&p).unwrap();
        assert_eq!(s.order, ids(&["ok"]));
        assert_eq!(s.hidden, ids(&["h"]));
    }

    #[test]
    fn set_order_must_be_a_permutation() {
        let cur = ids(&["a", "b", "c"]);
        assert!(check_order(&cur, &ids(&["c", "a", "b"])).is_ok());
        for bad in [
            ids(&["a", "b"]),
            ids(&["a", "b", "c", "d"]),
            ids(&["a", "a", "b", "c"]),
            ids(&["a", "a", "b"]),
        ] {
            assert_eq!(
                check_order(&cur, &bad),
                Err("order does not match the sidebar".into()),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn insert_clamps_and_never_duplicates() {
        let mut o = ids(&["a", "b"]);
        insert(&mut o, &ids(&["x", "y"]), Some(1));
        assert_eq!(o, ids(&["a", "x", "y", "b"]));
        insert(&mut o, &ids(&["z"]), Some(99));
        assert_eq!(o, ids(&["a", "x", "y", "b", "z"]));
        // Already pinned: stays where it is.
        insert(&mut o, &ids(&["b", "n"]), Some(0));
        assert_eq!(o, ids(&["n", "a", "x", "y", "b", "z"]));
        insert(&mut o, &ids(&["e"]), None);
        assert_eq!(o.last().unwrap(), "e");
    }
}
