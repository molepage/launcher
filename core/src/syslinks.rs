//! Deep links into System Settings panes. The built-in list lives in
//! `data/system_links.toml` (compiled in with `include_str!`), so entries
//! aren't hardcoded in engine logic. Users can extend or override entries
//! with `[[syslink]]` tables in config.toml — e.g. to wire up a pane that a
//! new macOS release added, without recompiling.

use crate::config::{Config, Syslink};
use serde::Deserialize;
use std::sync::OnceLock;

const DATA: &str = include_str!("../data/system_links.toml");

/// One row of the built-in table.
#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    pub anchor: String,
    pub title: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub target: String,
    pub url: bool,
    #[serde(default)]
    pub min_os: Option<u32>,
    #[serde(default)]
    pub max_os: Option<u32>,
}

fn builtin() -> &'static [Link] {
    static LINKS: OnceLock<Vec<Link>> = OnceLock::new();
    LINKS.get_or_init(|| {
        #[derive(Deserialize)]
        struct File {
            #[serde(rename = "entry")]
            entries: Vec<Link>,
        }
        toml::from_str::<File>(DATA).expect("system_links.toml parses").entries
    })
}

/// A link ready to become a launcher entry: built-ins that pass the OS gate
/// and the user's `system_links` selection, plus every user `[[syslink]]`
/// (explicit intent, always enabled). A user entry with the same anchor
/// replaces the built-in.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub anchor: String,
    pub title: String,
    pub keywords: Vec<String>,
    pub target: String,
    pub url: bool,
}

fn visible(link: &Link, os_major: Option<u32>) -> bool {
    match os_major {
        None => true,  // unknown OS (lq, tests): show everything
        Some(v) => link.min_os.map_or(true, |m| v >= m) && link.max_os.map_or(true, |m| v <= m),
    }
}

fn entry_for(link: &Link) -> Entry {
    Entry {
        anchor: link.anchor.clone(),
        title: link.title.clone(),
        keywords: link.keywords.clone(),
        target: link.target.clone(),
        url: link.url,
    }
}

fn entry_for_user(sl: &Syslink) -> Entry {
    Entry {
        anchor: sl.anchor.clone(),
        title: sl.title.clone(),
        keywords: sl.keywords.clone(),
        target: sl.target.clone(),
        url: sl.url,
    }
}

/// All entries for this OS and config, in table order with user entries
/// appended (after replacing same-anchor built-ins).
pub fn entries(config: &Config, os_major: Option<u32>) -> Vec<Entry> {
    let mut v: Vec<Entry> = builtin()
        .iter()
        .filter(|l| visible(l, os_major) && config.system_links.iter().any(|a| a == &l.anchor))
        .map(entry_for)
        .collect();
    for sl in &config.syslinks {
        if let Some(e) = v.iter_mut().find(|e| e.anchor == sl.anchor) {
            *e = entry_for_user(sl);
        } else {
            v.push(entry_for_user(sl));
        }
    }
    v
}

/// Anchors and titles of all built-ins visible on this OS, regardless of the
/// user's selection — the settings window's "all on" toggle needs the full
/// list. User entries are not included: they're always on.
pub fn anchors(os_major: Option<u32>) -> Vec<(String, String)> {
    builtin()
        .iter()
        .filter(|l| visible(l, os_major))
        .map(|l| (l.anchor.clone(), l.title.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(system_links: &[&str], syslinks: Vec<Syslink>) -> Config {
        Config {
            system_links: system_links.iter().map(|s| s.to_string()).collect(),
            syslinks,
            ..Config::default()
        }
    }

    #[test]
    fn data_parses_and_is_sane() {
        let links = builtin();
        assert!(links.len() >= 30);
        let mut anchors: Vec<&str> = links.iter().map(|l| l.anchor.as_str()).collect();
        anchors.sort();
        anchors.dedup();
        assert_eq!(anchors.len(), links.len(), "duplicate anchor in system_links.toml");
        for l in links {
            assert!(!l.title.is_empty());
            assert!(!l.target.is_empty());
            if l.url {
                assert!(l.target.starts_with("x-apple.systempreferences:"), "{}", l.anchor);
            }
        }
    }

    #[test]
    fn os_gates_filter_rows() {
        let anchors12: Vec<String> = anchors(Some(12)).into_iter().map(|(a, _)| a).collect();
        let anchors13: Vec<String> = anchors(Some(13)).into_iter().map(|(a, _)| a).collect();
        assert!(!anchors12.contains(&"passwords".to_string()));
        assert!(!anchors12.contains(&"login_items".to_string()));
        assert!(anchors13.contains(&"passwords".to_string()));
        assert!(anchors13.contains(&"login_items".to_string()));
        // Unknown OS: everything is visible.
        assert_eq!(anchors(None).len(), anchors13.len());
    }

    #[test]
    fn selection_gates_entries() {
        let c = cfg(&["firewall"], vec![]);
        let e = entries(&c, Some(12));
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].anchor, "firewall");
        assert_eq!(e[0].title, "Firewall");
    }

    #[test]
    fn user_syslinks_are_always_enabled_and_can_override() {
        let sl = Syslink {
            anchor: "wallpaper".into(),
            title: "Wallpaper".into(),
            target: "x-apple.systempreferences:com.apple.Wallpaper-Settings.extension".into(),
            keywords: vec!["desktop".into()],
            url: true,
        };
        // Unknown anchor: appended even though nothing is selected.
        let c = cfg(&[], vec![sl.clone()]);
        let e = entries(&c, None);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].anchor, "wallpaper");

        // Same anchor as a built-in: replaces it.
        let mut sl2 = sl.clone();
        sl2.anchor = "firewall".into();
        sl2.title = "Custom Firewall".into();
        let c = cfg(&["firewall"], vec![sl2]);
        let e = entries(&c, None);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].title, "Custom Firewall");
    }
}
