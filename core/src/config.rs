//! `config.toml` in the data folder. Created with defaults on first run.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT: &str = r#"# Launcher settings. Changes to hotkeys need a restart; everything else
# is picked up the next time the window opens.

hotkey = "alt+space"            # Raycast holds cmd+space; switch once it's gone
clipboard_hotkey = "cmd+shift+v"

# Extra folders to scan for apps (the standard ones are always scanned).
app_dirs = []

# Quicklinks. Put {query} in the url to make a search: type "<keyword> <text>".
# A url can also be a folder or file path.
[[quicklink]]
name = "Google"
keyword = "g"
url = "https://www.google.com/search?q={query}"

[[quicklink]]
name = "GitHub"
keyword = "gh"
url = "https://github.com"

[[quicklink]]
name = "Downloads"
url = "~/Downloads"
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Quicklink {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub keyword: Option<String>,
}

impl Quicklink {
    pub fn is_search(&self) -> bool {
        self.url.contains("{query}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_clip_hotkey")]
    pub clipboard_hotkey: String,
    #[serde(default)]
    pub app_dirs: Vec<String>,
    #[serde(default = "default_appearance")]
    pub appearance: String,
    /// Enabled System Settings deep links (anchors, see engine::SYSTEM_LINKS).
    #[serde(default)]
    pub system_links: Vec<String>,
    /// File search ("f <name>"). Off = nothing on disk is read.
    #[serde(default)]
    pub file_search: bool,
    /// Folders to index for file search; empty = home + iCloud Drive.
    #[serde(default)]
    pub search_dirs: Vec<String>,
    /// The sleeping cat on top of the search box.
    #[serde(default = "default_cat")]
    pub cat: bool,
    #[serde(default, rename = "quicklink")]
    pub quicklinks: Vec<Quicklink>,
}

fn default_hotkey() -> String {
    "alt+space".into()
}
fn default_clip_hotkey() -> String {
    "cmd+shift+v".into()
}
fn default_appearance() -> String {
    "system".into()
}
fn default_cat() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT).expect("default config parses")
    }
}

impl Config {
    /// Load, writing the default file first if there is none. A broken file
    /// falls back to defaults (and reports why) rather than refusing to start.
    pub fn load(path: &Path) -> (Config, Option<String>) {
        if !path.exists() {
            let _ = std::fs::write(path, DEFAULT);
        }
        match std::fs::read_to_string(path) {
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(mut c) => {
                    if !["system", "light", "dark"].contains(&c.appearance.as_str()) {
                        c.appearance = "system".into();
                    }
                    // Migrate configs saved when only four System Settings
                    // panes existed: "all on" used to be those four.
                    const OLD_FOUR: [&str; 4] = ["accessibility", "firewall", "notifications", "login_items"];
                    if c.system_links.iter().filter(|l| OLD_FOUR.contains(&l.as_str())).count() == 4
                        && c.system_links.len() == 4
                    {
                        c.system_links = crate::engine::SYSTEM_LINKS.iter().map(|l| l.0.to_string()).collect();
                    }
                    (c, None)
                }
                Err(e) => (Config::default(), Some(format!("config.toml: {e}"))),
            },
            Err(e) => (Config::default(), Some(format!("config.toml: {e}"))),
        }
    }

    /// Merge the given JSON fields over this config, write it back to
    /// `path`, and return the saved config. Missing keys keep their value.
    pub fn write_fields(&self, path: &Path, fields: &serde_json::Value) -> Result<Config, String> {
        let mut c = self.clone();
        if let Some(h) = fields.get("hotkey").and_then(|v| v.as_str()) {
            c.hotkey = h.to_owned();
        }
        if let Some(h) = fields.get("clipboard_hotkey").and_then(|v| v.as_str()) {
            c.clipboard_hotkey = h.to_owned();
        }
        if let Some(a) = fields.get("appearance").and_then(|v| v.as_str()) {
            if ["system", "light", "dark"].contains(&a) {
                c.appearance = a.to_owned();
            }
        }
        if let Some(dirs) = fields.get("app_dirs").and_then(|v| v.as_array()) {
            c.app_dirs = dirs.iter().filter_map(|d| d.as_str().map(String::from)).collect();
        }
        if let Some(links) = fields.get("system_links").and_then(|v| v.as_array()) {
            c.system_links = links.iter().filter_map(|l| l.as_str().map(String::from)).collect();
        }
        if let Some(b) = fields.get("file_search").and_then(|v| v.as_bool()) {
            c.file_search = b;
        }
        if let Some(dirs) = fields.get("search_dirs").and_then(|v| v.as_array()) {
            c.search_dirs = dirs.iter().filter_map(|d| d.as_str().map(String::from)).collect();
        }
        let body = toml::to_string_pretty(&c).map_err(|e| format!("serialize config: {e}"))?;
        let header = "# Launcher settings. Hotkey changes apply immediately; the rest\n\
                      # is picked up the next time the window opens.\n\n";
        std::fs::write(path, format!("{header}{body}")).map_err(|e| format!("write config: {e}"))?;
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parses_with_quicklinks() {
        let c = Config::default();
        assert_eq!(c.hotkey, "alt+space");
        assert_eq!(c.quicklinks.len(), 3);
        assert!(c.quicklinks[0].is_search());
    }

    #[test]
    fn write_fields_merges_and_keeps_unknown_keys() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let (c, _) = Config::load(&p);
        let fields: serde_json::Value =
            serde_json::json!({"hotkey": "cmd+space", "app_dirs": ["~/Apps"], "appearance": "dark"});
        let saved = c.write_fields(&p, &fields).unwrap();
        assert_eq!(saved.hotkey, "cmd+space");
        assert_eq!(saved.app_dirs, vec!["~/Apps".to_string()]);
        assert_eq!(saved.appearance, "dark");
        assert_eq!(saved.quicklinks.len(), 3);  // untouched
        // Round-trip: what's on disk loads back identically.
        let (reloaded, err) = Config::load(&p);
        assert!(err.is_none());
        assert_eq!(reloaded, saved);
        // Invalid appearance values are ignored on write.
        let bad = saved.write_fields(&p, &serde_json::json!({"appearance": "blue"})).unwrap();
        assert_eq!(bad.appearance, "dark");
    }

    #[test]
    fn old_four_system_links_expand_to_all() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        std::fs::write(&p, "system_links = [\"accessibility\", \"firewall\", \"notifications\", \"login_items\"]\n").unwrap();
        let (c, err) = Config::load(&p);
        assert!(err.is_none());
        assert_eq!(c.system_links.len(), crate::engine::SYSTEM_LINKS.len());
        // A deliberate subset that isn't the old four is left alone.
        std::fs::write(&p, "system_links = [\"firewall\"]\n").unwrap();
        let (c, err) = Config::load(&p);
        assert!(err.is_none());
        assert_eq!(c.system_links, vec!["firewall".to_string()]);
    }

    #[test]
    fn load_writes_default_and_survives_garbage() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let (c, err) = Config::load(&p);
        assert!(err.is_none() && p.exists());
        assert_eq!(c, Config::default());
        std::fs::write(&p, "hotkey = [").unwrap();
        let (c, err) = Config::load(&p);
        assert!(err.is_some());
        assert_eq!(c, Config::default());
    }
}
