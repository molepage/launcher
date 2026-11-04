//! Built-in sources that read the filesystem: apps, screenshots, files.

use crate::item::{Action, Icon, Item};
use crate::matcher::NameMatcher;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn ago(ts: i64, now: i64) -> String {
    let d = (now - ts).max(0);
    match d {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", d / 60),
        3600..86400 => format!("{} h ago", d / 3600),
        86400..172800 => "yesterday".into(),
        _ => format!("{} days ago", d / 86400),
    }
}

pub fn expand_tilde(p: &str, home: &Path) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if p == "~" => home.to_path_buf(),
        None => PathBuf::from(p),
    }
}

/// Shorten a path under home to ~/… (and iCloud Drive to its name).
pub fn pretty(p: &Path, home: &Path) -> String {
    if let Ok(rest) = p.strip_prefix(home.join("Library/Mobile Documents/com~apple~CloudDocs")) {
        return Path::new("iCloud Drive").join(rest).display().to_string();
    }
    match p.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

fn mtime(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------- apps ----------

pub fn app_dirs(home: &Path, extra: &[String]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        "/Applications",
        "/System/Applications",
        "/System/Applications/Utilities",
        "/System/Library/CoreServices/Applications",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    v.push(home.join("Applications"));
    v.extend(extra.iter().map(|e| expand_tilde(e, home)));
    v
}

/// Find `.app` bundles in the given folders, looking one folder deep
/// (for things like /Applications/Utilities or vendor folders).
pub fn scan_apps(dirs: &[PathBuf], home: &Path) -> Vec<Item> {
    let mut found: Vec<PathBuf> = Vec::new();
    for d in dirs {
        collect_apps(d, 2, &mut found);
    }
    found.push(PathBuf::from("/System/Library/CoreServices/Finder.app"));
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|p| p.exists())
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            if !seen.insert(name.to_lowercase()) {
                return None;
            }
            let path = p.to_string_lossy().into_owned();
            Some(
                Item::new(format!("app:{path}"), name, Action::Open { path: path.clone() })
                    .subtitle(pretty(p.parent().unwrap_or(Path::new("/")), home))
                    .icon(Icon::File(path.clone()))
                    .alt(Action::Reveal { path }),
            )
        })
        .collect()
}

fn collect_apps(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if name.ends_with(".app") {
            out.push(p);
        } else if depth > 1 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            collect_apps(&p, depth - 1, out);
        }
    }
}

// ---------- screenshots ----------

const MEDIA: &[&str] = &["png", "jpg", "jpeg", "heic", "tiff", "gif", "mov", "mp4"];
const SHOT_PREFIXES: &[&str] = &["screenshot", "screen shot", "screen recording", "cleanshot"];

pub fn screenshots(dir: &Path, home: &Path, query: &str, now: i64, limit: usize) -> Vec<Item> {
    let only_named = dir == home.join("Desktop");
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<(PathBuf, String, i64, u64)> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_string_lossy().to_lowercase();
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            if !MEDIA.contains(&ext.as_str()) || lower.starts_with('.') {
                return None;
            }
            if only_named && !SHOT_PREFIXES.iter().any(|s| lower.starts_with(s)) {
                return None;
            }
            let md = e.metadata().ok()?;
            let ts = md.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
            Some((p, name, ts, md.len()))
        })
        .collect();
    files.sort_by(|a, b| b.2.cmp(&a.2));
    let mut m = NameMatcher::new(query);
    files
        .into_iter()
        .filter(|(_, name, _, _)| query.trim().is_empty() || m.score(name).is_some())
        .take(limit)
        .map(|(p, name, ts, size)| {
            let path = p.to_string_lossy().into_owned();
            Item::new(format!("shot:{path}"), name, Action::CopyFile { path: path.clone() })
                .subtitle(format!("{} · {} · ↩ copy  ⌘↩ show in Finder", ago(ts, now), human_size(size)))
                .icon(Icon::Thumb(path.clone()))
                .alt(Action::Reveal { path })
        })
        .collect()
}

fn human_size(b: u64) -> String {
    match b {
        0..1024 => format!("{b} B"),
        1024..1_048_576 => format!("{:.0} KB", b as f64 / 1024.0),
        _ => format!("{:.1} MB", b as f64 / 1_048_576.0),
    }
}

// ---------- files ----------

fn skip_path(p: &str, home: &str) -> bool {
    let rel = p.strip_prefix(home).unwrap_or(p);
    rel.starts_with("/Library/")
        || rel.split('/').any(|c| c.starts_with('.') || c == "node_modules" || c == "target" || c == "DerivedData")
}

pub fn rank_files<'a>(
    paths: impl Iterator<Item = &'a str>,
    home_s: &str,
    home: &Path,
    q: &str,
    now: i64,
    limit: usize,
) -> Vec<Item> {
    let mut m = NameMatcher::new(q);
    let mut scored: Vec<((u8, u32), &str)> = paths
        .filter(|p| !p.is_empty() && !skip_path(p, home_s))
        .filter_map(|p| {
            let name = p.rsplit('/').next()?;
            Some((m.score(name)?, p))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.len().cmp(&b.1.len())));
    scored.truncate(limit * 3);
    // Within the shortlist: match class, then shallow paths (not buried in
    // a project tree), then most recently modified.
    let depth = |p: &str| p.strip_prefix(home_s).unwrap_or(p).matches('/').count();
    let mut with_time: Vec<_> =
        scored.into_iter().map(|(s, p)| (s.0, depth(p) <= 5, mtime(Path::new(p)), s.1, p)).collect();
    with_time.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)).then(b.3.cmp(&a.3)));
    with_time
        .into_iter()
        .take(limit)
        .map(|(_, _, ts, _, p)| {
            let path = PathBuf::from(p);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let parent = pretty(path.parent().unwrap_or(Path::new("/")), home);
            Item::new(format!("file:{p}"), name, Action::Open { path: p.into() })
                .subtitle(format!("{parent} · {}", ago(ts, now)))
                .icon(Icon::File(p.into()))
                .alt(Action::Reveal { path: p.into() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_found_one_level_deep_and_deduped() {
        let d = tempfile::tempdir().unwrap();
        for p in ["A/Safari.app", "A/Vendor/Tool.app", "A/Vendor/Deep/Nope/X.app", "B/Safari.app", "A/.Hidden.app"] {
            std::fs::create_dir_all(d.path().join(p)).unwrap();
        }
        let items = scan_apps(&[d.path().join("A"), d.path().join("B")], d.path());
        let names: HashSet<_> = items.iter().map(|i| i.title.as_str()).collect();
        assert!(names.contains("Safari") && names.contains("Tool"));
        assert!(!names.contains("X") && !names.contains(".Hidden"));
        assert_eq!(items.iter().filter(|i| i.title == "Safari").count(), 1);
    }

    #[test]
    fn screenshots_on_desktop_need_screenshot_names() {
        let home = tempfile::tempdir().unwrap();
        let desk = home.path().join("Desktop");
        std::fs::create_dir_all(&desk).unwrap();
        std::fs::write(desk.join("Screenshot 2026-11-04 at 10.00.00.png"), b"x").unwrap();
        std::fs::write(desk.join("holiday.png"), b"x").unwrap();
        std::fs::write(desk.join("notes.txt"), b"x").unwrap();
        let s = screenshots(&desk, home.path(), "", now(), 10);
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0].action, Action::CopyFile { .. }));
        let other = home.path().join("Shots");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("holiday.png"), b"x").unwrap();
        assert_eq!(screenshots(&other, home.path(), "hol", now(), 10).len(), 1);
        assert_eq!(screenshots(&other, home.path(), "zzz", now(), 10).len(), 0);
    }

    #[test]
    fn file_ranking_prefers_name_matches_and_skips_noise() {
        let home = Path::new("/Users/me");
        let paths = [
            "/Users/me/Documents/my-report-old.pdf",
            "/Users/me/Documents/report.pdf",
            "/Users/me/Library/Caches/report.pdf",
            "/Users/me/code/x/node_modules/report.js",
            "/Users/me/.config/report",
        ];
        let items = rank_files(paths.into_iter(), "/Users/me", home, "report", 0, 10);
        let titles: Vec<_> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["report.pdf", "my-report-old.pdf"]);
        assert_eq!(items[0].subtitle.split(" · ").next(), Some("~/Documents"));
    }

    #[test]
    fn ago_reads_naturally() {
        assert_eq!(ago(100, 110), "just now");
        assert_eq!(ago(0, 600), "10 min ago");
        assert_eq!(ago(0, 100_000), "yesterday");
    }
}
