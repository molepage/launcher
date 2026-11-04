//! Add-ons: a folder with `addon.toml` and a program that prints JSON items.

use crate::item::Item;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub keyword: String,
    pub exec: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Addon {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl Addon {
    pub fn argv(&self, query: &str) -> Vec<String> {
        vec![self.dir.join(&self.manifest.exec).to_string_lossy().into_owned(), query.to_owned()]
    }
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.manifest.timeout_ms.unwrap_or(1000).clamp(50, 10_000))
    }
}

/// Every valid add-on in `dir`, sorted by folder name. Broken ones are
/// returned as errors so the shell can say why one isn't showing.
pub fn discover(dir: &Path) -> (Vec<Addon>, Vec<String>) {
    let mut addons = Vec::new();
    let mut errors = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return (addons, errors) };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        let mf = d.join("addon.toml");
        let Ok(text) = std::fs::read_to_string(&mf) else { continue };
        let id = d.file_name().unwrap_or_default().to_string_lossy().into_owned();
        match toml::from_str::<Manifest>(&text) {
            Ok(m) if m.keyword.trim().is_empty() || m.keyword.contains(' ') => {
                errors.push(format!("{id}: keyword must be one word"))
            }
            Ok(m) => addons.push(Addon { id, dir: d, manifest: m }),
            Err(e) => errors.push(format!("{id}: {e}")),
        }
    }
    (addons, errors)
}

#[derive(Deserialize)]
struct Output {
    items: Vec<Item>,
}

/// Parse an add-on's stdout. Uids are namespaced by add-on so two add-ons
/// can't collide in the learning data.
pub fn parse_output(addon_id: &str, stdout: &str) -> Result<Vec<Item>, String> {
    let out: Output = serde_json::from_str(stdout.trim()).map_err(|e| e.to_string())?;
    Ok(out
        .items
        .into_iter()
        .take(200)
        .map(|mut it| {
            it.uid = format!("addon:{addon_id}:{}", it.uid);
            it
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Action;
    use crate::runner::Runner;

    #[test]
    fn discovers_valid_and_reports_broken() {
        let d = tempfile::tempdir().unwrap();
        let good = d.path().join("quit");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join("addon.toml"), "name='Quit'\nkeyword='quit'\nexec='run.sh'\n").unwrap();
        let bad = d.path().join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join("addon.toml"), "name='x'").unwrap();
        std::fs::create_dir_all(d.path().join("not-an-addon")).unwrap();
        let (a, e) = discover(d.path());
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].manifest.keyword, "quit");
        assert_eq!(e.len(), 1);
        assert!(e[0].starts_with("bad:"));
    }

    #[test]
    fn runs_a_script_end_to_end() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("echo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("addon.toml"), "name='Echo'\nkeyword='echo'\nexec='run.sh'\n").unwrap();
        let script = dir.join("run.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"items\":[{\"uid\":\"1\",\"title\":\"%s\",\"action\":{\"type\":\"copy\",\"text\":\"%s\"}}]}' \"$1\" \"$1\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (a, _) = discover(d.path());
        let out = Runner::default().run(&a[0].argv("hello"), Some(&a[0].dir), a[0].timeout()).unwrap();
        let items = parse_output(&a[0].id, &out).unwrap();
        assert_eq!(items[0].uid, "addon:echo:1");
        assert_eq!(items[0].title, "hello");
        assert_eq!(items[0].action, Action::Copy { text: "hello".into() });
    }

    #[test]
    fn bad_output_is_an_error() {
        assert!(parse_output("x", "not json").is_err());
        assert!(parse_output("x", r#"{"items":[{"title":"no uid"}]}"#).is_err());
    }
}
