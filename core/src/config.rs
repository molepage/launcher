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
    #[serde(default, rename = "quicklink")]
    pub quicklinks: Vec<Quicklink>,
}

fn default_hotkey() -> String {
    "alt+space".into()
}
fn default_clip_hotkey() -> String {
    "cmd+shift+v".into()
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
            Ok(s) => match toml::from_str(&s) {
                Ok(c) => (c, None),
                Err(e) => (Config::default(), Some(format!("config.toml: {e}"))),
            },
            Err(e) => (Config::default(), Some(format!("config.toml: {e}"))),
        }
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
