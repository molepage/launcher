//! The result row shared by every source, and the actions the shell performs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Icon {
    /// The icon of a file or app at this path.
    File(String),
    /// A thumbnail of the image or video at this path.
    Thumb(String),
    /// A named system symbol (SF Symbols on macOS).
    Symbol(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Open { path: String },
    OpenUrl { url: String },
    Copy { text: String },
    Paste { text: String },
    CopyFile { path: String },
    Reveal { path: String },
    Run { argv: Vec<String> },
    /// Replace the search text (used by commands like "Clipboard History").
    Complete { text: String },
    /// Something the shell handles itself ("quit").
    Internal { name: String },
    /// A hint row that does nothing.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub uid: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub icon: Option<Icon>,
    pub action: Action,
    /// Performed on ⌘↩.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_action: Option<Action>,
}

impl Item {
    pub fn new(uid: impl Into<String>, title: impl Into<String>, action: Action) -> Self {
        Item {
            uid: uid.into(),
            title: title.into(),
            subtitle: String::new(),
            icon: None,
            action,
            alt_action: None,
        }
    }
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = s.into();
        self
    }
    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn alt(mut self, a: Action) -> Self {
        self.alt_action = Some(a);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_matches_addon_protocol() {
        let it = Item::new("x", "Safari", Action::Run { argv: vec!["a".into()] })
            .icon(Icon::File("/Applications/Safari.app".into()));
        let j = serde_json::to_value(&it).unwrap();
        assert_eq!(j["icon"]["kind"], "file");
        assert_eq!(j["action"]["type"], "run");
        let back: Item = serde_json::from_value(j).unwrap();
        assert_eq!(back, it);
    }

    #[test]
    fn minimal_addon_item_parses() {
        let it: Item =
            serde_json::from_str(r#"{"uid":"a","title":"T","action":{"type":"none"}}"#).unwrap();
        assert_eq!(it.action, Action::None);
        assert!(it.icon.is_none());
    }
}
