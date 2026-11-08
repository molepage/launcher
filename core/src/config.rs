//! `config.toml` in the data folder. Created with defaults on first run.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const DEFAULT: &str = r#"# Launcher settings. Hotkey changes apply immediately; the rest
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

/// A user-defined System Settings entry in config.toml. `target` is either an
/// `x-apple.systempreferences:` URL or the AppleScript pane/anchor id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Syslink {
    pub anchor: String,
    pub title: String,
    pub target: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Whether `target` is a URL (true) or an AppleScript pane id (false).
    /// Defaults to URL, matching how the built-in entries are mostly used.
    #[serde(default = "default_true")]
    pub url: bool,
}

fn default_true() -> bool {
    true
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
    /// Enabled System Settings deep links (anchors, see syslinks::links).
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
    /// User-defined System Settings entries (see syslinks.rs). Explicit
    /// intent, so always enabled.
    #[serde(default, rename = "syslink")]
    pub syslinks: Vec<Syslink>,
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
                    (c, None)
                }
                Err(e) => (Config::default(), Some(format!("config.toml: {e}"))),
            },
            Err(e) => (Config::default(), Some(format!("config.toml: {e}"))),
        }
    }

    /// Merge the given JSON fields over the config file, write it back to
    /// `path`, and return the saved config. Missing keys keep their value, as
    /// do comments and keys the `Config` struct doesn't know about (the merge
    /// is format-preserving via `toml_edit`). A field with a type the config
    /// can't represent rejects the whole save, so mistakes surface instead of
    /// being silently dropped.
    pub fn write_fields(path: &Path, fields: &serde_json::Value) -> Result<Config, String> {
        let obj = fields
            .as_object()
            .ok_or_else(|| "config fields must be a JSON object".to_string())?;
        let text = std::fs::read_to_string(path).unwrap_or_else(|_| DEFAULT.to_string());
        let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("parse config: {e}"))?;
        for (k, v) in obj {
            if let Some(item) = json_to_edit(v) {
                doc[k] = item;
            }
        }
        let out = doc.to_string();
        let mut c: Config = toml::from_str(&out).map_err(|e| format!("config field rejected: {e}"))?;
        if !["system", "light", "dark"].contains(&c.appearance.as_str()) {
            c.appearance = "system".into();
        }
        std::fs::write(path, out).map_err(|e| format!("write config: {e}"))?;
        Ok(c)
    }
}

/// Convert a JSON value to a TOML item. Nulls map to None (skipped).
fn json_to_edit(v: &serde_json::Value) -> Option<toml_edit::Item> {
    let value = match v {
        serde_json::Value::Null => return None,
        serde_json::Value::Bool(b) => toml_edit::Value::from(*b),
        serde_json::Value::String(s) => toml_edit::Value::from(s.as_str()),
        serde_json::Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => toml_edit::Value::from(i),
            (None, Some(u)) => toml_edit::Value::from(u as i64),
            _ => toml_edit::Value::from(n.as_f64().unwrap_or(f64::NAN)),
        },
        serde_json::Value::Array(a) => {
            let mut arr = toml_edit::Array::new();
            for v in a {
                match json_to_edit(v) {
                    Some(toml_edit::Item::Value(val)) => arr.push(val),
                    _ => return None,
                }
            }
            toml_edit::Value::from(arr)
        }
        serde_json::Value::Object(o) => {
            let mut t = toml_edit::InlineTable::new();
            for (k, v) in o {
                if let Some(item) = json_to_edit(v) {
                    t.insert(k, item.into_value().ok()?);
                }
            }
            toml_edit::Value::from(t)
        }
    };
    Some(toml_edit::Item::Value(value))
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
        Config::load(&p);  // creates the default file
        // Hand edits the merge must survive: comments and unknown keys.
        let mut text = std::fs::read_to_string(&p).unwrap();
        text = text.replace(
            "app_dirs = []",
            "# my own note\napp_dirs = []\ncustom_key = 42\n",
        );
        std::fs::write(&p, &text).unwrap();
        let fields: serde_json::Value =
            serde_json::json!({"hotkey": "cmd+space", "app_dirs": ["~/Apps"], "appearance": "dark"});
        let saved = Config::write_fields(&p, &fields).unwrap();
        assert_eq!(saved.hotkey, "cmd+space");
        assert_eq!(saved.app_dirs, vec!["~/Apps".to_string()]);
        assert_eq!(saved.appearance, "dark");
        assert_eq!(saved.quicklinks.len(), 3);  // untouched
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.contains("# my own note"));
        assert!(out.contains("custom_key = 42"));
        // Round-trip: what's on disk loads back identically.
        let (reloaded, err) = Config::load(&p);
        assert!(err.is_none());
        assert_eq!(reloaded, saved);
        // Invalid appearance values are normalized like Config::load does.
        let bad = Config::write_fields(&p, &serde_json::json!({"appearance": "blue"})).unwrap();
        assert_eq!(bad.appearance, "system");
    }

    /// Every field the settings UI writes round-trips through write_fields.
    /// Catches new config fields being forgotten in the save path — this test
    /// would have caught the cat toggle never persisting.
    #[test]
    fn write_fields_round_trips_every_ui_field() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        Config::load(&p);  // creates the default file
        let fields: serde_json::Value = serde_json::json!({
            "hotkey": "ctrl+opt+l",
            "clipboard_hotkey": "cmd+shift+c",
            "appearance": "light",
            "app_dirs": ["~/Apps", "/Applications"],
            "system_links": ["firewall", "network"],
            "file_search": true,
            "search_dirs": ["~/Documents"],
            "cat": false,
        });
        let saved = Config::write_fields(&p, &fields).unwrap();
        assert_eq!(saved.hotkey, "ctrl+opt+l");
        assert_eq!(saved.clipboard_hotkey, "cmd+shift+c");
        assert_eq!(saved.appearance, "light");
        assert_eq!(saved.cat, false);
        assert_eq!(saved.file_search, true);
        let (reloaded, err) = Config::load(&p);
        assert!(err.is_none());
        assert_eq!(reloaded, saved);
    }

    /// A JSON field with a type the config can't represent rejects the save
    /// instead of being silently dropped.
    #[test]
    fn write_fields_rejects_bad_types() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        Config::load(&p);  // creates the default file
        let before = std::fs::read_to_string(&p).unwrap();
        assert!(Config::write_fields(&p, &serde_json::json!({"hotkey": 5})).is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), before); // file untouched
    }

    /// No migration: `system_links` is exactly what the user chose, however
    /// stale; the settings toggle rewrites it.
    #[test]
    fn system_links_are_taken_verbatim() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        std::fs::write(&p, "system_links = [\"accessibility\", \"firewall\", \"notifications\", \"login_items\"]\n").unwrap();
        let (c, err) = Config::load(&p);
        assert!(err.is_none());
        assert_eq!(c.system_links.len(), 4);
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
