//! Ties the sources together: parses the query, ranks, and learns.

use crate::addons::{self, Addon};
use crate::calc;
use crate::clips;
use crate::config::{Config, Quicklink};
use crate::fileindex::{self, FileIndex};
use crate::item::{Action, Icon, Item};
use crate::learn::{Boost, Learner};
use crate::matcher::NameMatcher;
use crate::runner::Runner;
use crate::sources::{self, ago, expand_tilde, now};
use crate::syslinks;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub const MAX_RESULTS: usize = 30;

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    pub data_dir: PathBuf,
    pub home: PathBuf,
    /// Where screenshots are saved; defaults to ~/Desktop.
    #[serde(default)]
    pub screenshot_dir: Option<PathBuf>,
    /// macOS major version, for gating System Settings entries that only
    /// exist in some releases. None = unknown, show everything.
    #[serde(default)]
    pub os_major: Option<u32>,
    /// App folders to scan. None = the standard system folders plus
    /// ~/Applications and config app_dirs. Tests set this so they never
    /// touch the real machine's /Applications.
    #[serde(default)]
    pub app_roots: Option<Vec<PathBuf>>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct QueryResult {
    pub items: Vec<Item>,
    /// True when `query_slow` will add more (files, add-ons).
    pub slow: bool,
}

struct State {
    config: Config,
    apps: Vec<Item>,
    addons: Vec<Addon>,
    errors: Vec<String>,
    refreshed: Instant,
}

pub struct Engine {
    opts: Options,
    state: RwLock<State>,
    db: Mutex<Connection>,
    learner: Mutex<Learner>,
    runner: Runner,
    files: Arc<RwLock<Option<Arc<FileIndex>>>>,
    indexing: Arc<AtomicBool>,
}

/// Rebuild the file index when it's older than this and the window opens.
const INDEX_MAX_AGE: Duration = Duration::from_secs(15 * 60);

fn hint(text: impl Into<String>) -> Item {
    Item::new("hint:", text, Action::None).icon(Icon::Symbol("info.circle".into()))
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// System Settings panes live in syslinks.rs (data/system_links.toml).

impl Engine {
    pub fn open(opts: Options) -> Result<Engine, String> {
        std::fs::create_dir_all(opts.data_dir.join("addons")).map_err(|e| e.to_string())?;
        let conn = Connection::open(opts.data_dir.join("launcher.db")).map_err(|e| e.to_string())?;
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");
        let t = now();
        let learner = Learner::init(&conn, t).map_err(|e| e.to_string())?;
        clips::init(&conn, t).map_err(|e| e.to_string())?;
        let state = Self::load_state(&opts);
        let e = Engine {
            opts,
            state: RwLock::new(state),
            db: Mutex::new(conn),
            learner: Mutex::new(learner),
            runner: Runner::default(),
            files: Arc::default(),
            indexing: Arc::default(),
        };
        Ok(e)
    }

    fn load_state(opts: &Options) -> State {
        let (config, cfg_err) = Config::load(&opts.data_dir.join("config.toml"));
        let dirs = match &opts.app_roots {
            Some(v) => v.clone(),
            None => sources::app_dirs(&opts.home, &config.app_dirs),
        };
        let apps = sources::scan_apps(&dirs, &opts.home);
        let (addons, mut errors) = addons::discover(&opts.data_dir.join("addons"));
        errors.extend(cfg_err);
        State { config, apps, addons, errors, refreshed: Instant::now() }
    }

    /// Rescan apps, config and add-ons. Cheap; the shell calls it whenever
    /// the window opens. Skipped if done in the last few seconds unless
    /// `force` (the user just saved settings).
    pub fn refresh(&self, force: bool) {
        if !force && self.state.read().unwrap().refreshed.elapsed() < Duration::from_secs(5) {
            return;
        }
        let fresh = Self::load_state(&self.opts);
        *self.state.write().unwrap() = fresh;
    }

    /// Start a background rebuild if there's no index, it's stale, or the
    /// configured roots changed. Only called when the user actually searches
    /// files, so nothing on disk is read unless file search is enabled.
    fn ensure_file_index(&self) {
        let roots = self.search_roots();
        let stale = match &*self.files.read().unwrap() {
            None => true,
            Some(ix) => ix.built.elapsed() > INDEX_MAX_AGE || ix.roots != roots,
        };
        if !stale || self.indexing.swap(true, Ordering::SeqCst) {
            return;
        }
        let (slot, flag) = (self.files.clone(), self.indexing.clone());
        std::thread::spawn(move || {
            let ix = FileIndex::build(&roots);
            *slot.write().unwrap() = Some(Arc::new(ix));
            flag.store(false, Ordering::SeqCst);
        });
    }

    /// Folders to index: the user's choice, or home + iCloud by default.
    fn search_roots(&self) -> Vec<PathBuf> {
        let (enabled, dirs) = {
            let st = self.state.read().unwrap();
            (st.config.file_search, st.config.search_dirs.clone())
        };
        if !enabled {
            return vec![];
        }
        if dirs.is_empty() {
            return fileindex::default_roots(&self.opts.home);
        }
        dirs.iter().map(|d| expand_tilde(d, &self.opts.home).to_path_buf()).collect()
    }

    /// Search file names. `None` while the first index is still being built.
    pub fn search_files(&self, query: &str) -> Option<Vec<Item>> {
        // Runs off the UI thread, so wait (bounded) for a first build in progress.
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.files.read().unwrap().is_none() && self.indexing.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let ix = self.files.read().unwrap().clone()?;
        let paths = ix.search(query, MAX_RESULTS * 3);
        let home_s = self.opts.home.to_string_lossy();
        let strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        Some(sources::rank_files(strs.iter().map(String::as_str), &home_s, &self.opts.home, query, now(), MAX_RESULTS))
    }

    pub fn settings(&self) -> serde_json::Value {
        let st = self.state.read().unwrap();
        serde_json::json!({
            "hotkey": st.config.hotkey,
            "clipboard_hotkey": st.config.clipboard_hotkey,
            "app_dirs": st.config.app_dirs,
            "appearance": st.config.appearance,
            "system_links": st.config.system_links,
            "file_search": st.config.file_search,
            "search_dirs": st.config.search_dirs,
            "cat": st.config.cat,
            "errors": st.errors,
            "data_dir": self.opts.data_dir,
            "indexed_files": self.files.read().unwrap().as_ref().map(|ix| ix.len()),
        })
    }

    /// Merge `fields` into config.toml, reload state, and return the new
    /// settings JSON (or an error string under "error").
    pub fn write_config(&self, fields: &serde_json::Value) -> serde_json::Value {
        let path = self.opts.data_dir.join("config.toml");
        match Config::write_fields(&path, fields) {
            Ok(_) => {
                self.refresh(true);
                self.settings()
            }
            Err(e) => serde_json::json!({ "error": e }),
        }
    }

    /// Built-in System Settings anchors visible on this OS, for the settings
    /// window's enable-all toggle.
    pub fn system_link_anchors(&self) -> Vec<(String, String)> {
        syslinks::anchors(self.opts.os_major)
    }

    fn screenshot_dir(&self) -> PathBuf {
        self.opts.screenshot_dir.clone().unwrap_or_else(|| self.opts.home.join("Desktop"))
    }

    fn quicklink_action(&self, url: &str) -> Action {
        if url.contains("://") {
            Action::OpenUrl { url: url.to_owned() }
        } else {
            Action::Open { path: expand_tilde(url, &self.opts.home).to_string_lossy().into_owned() }
        }
    }

    /// Fixed commands, quicklinks and add-on entries, with their aliases.
    fn commands(&self, st: &State) -> Vec<(Item, Vec<String>)> {
        let sym = |s: &str| Icon::Symbol(s.into());
        let complete = |t: &str| Action::Complete { text: t.into() };
        let data = self.opts.data_dir.to_string_lossy().into_owned();
        let mut v = vec![
            (
                Item::new("cmd:clip", "Clipboard History", complete("clip "))
                    .subtitle("clip <text>")
                    .icon(sym("doc.on.clipboard")),
                vec!["clip".into(), "clipboard".into()],
            ),
            (
                Item::new("cmd:ss", "Search Screenshots", complete("ss "))
                    .subtitle("ss <name>")
                    .icon(sym("camera.viewfinder")),
                vec!["ss".into(), "screenshots".into()],
            ),
            (
                Item::new("cmd:f", "Search Files", complete("f ")).subtitle("f <name>").icon(sym("doc.text.magnifyingglass")),
                vec!["f".into(), "find".into()],
            ),
            (
                Item::new("cmd:config", "Launcher Settings", Action::Open { path: format!("{data}/config.toml") })
                    .subtitle("Edit config.toml")
                    .icon(sym("gearshape")),
                vec!["settings".into(), "preferences".into(), "config".into()],
            ),
            (
                Item::new("cmd:addons", "Launcher Add-ons Folder", Action::Open { path: format!("{data}/addons") })
                    .subtitle(format!("{} installed", st.addons.len()))
                    .icon(sym("puzzlepiece.extension")),
                vec!["addons".into(), "extensions".into()],
            ),
            (
                Item::new("cmd:quit", "Quit Launcher", Action::Internal { name: "quit".into() }).icon(sym("power")),
                vec!["exit".into()],
            ),
        ];
        for e in syslinks::entries(&st.config, self.opts.os_major) {
            let mut aliases: Vec<String> = e.keywords.clone();
            aliases.push(e.title.to_lowercase());
            let action = if e.url {
                Action::OpenUrl { url: e.target.clone() }
            } else {
                // "System Preferences" doesn't exist on macOS 13+; targeting
                // by bundle id resolves to System Preferences on 12 and
                // System Settings on 13 and later.
                Action::Run {
                    argv: vec![
                        "osascript".into(),
                        "-e".into(),
                        format!(
                            "tell application id \"com.apple.systempreferences\"\nactivate\nreveal pane id \"{}\"\nend tell",
                            e.target
                        ),
                    ],
                }
            };
            v.push((
                Item::new(format!("sys:{}", e.anchor), &e.title, action)
                    .subtitle(format!("System Settings > {}", e.title))
                    .icon(sym("gearshape.fill")),
                aliases,
            ));
        }
        for ql in &st.config.quicklinks {
            let aliases = ql.keyword.iter().cloned().collect();
            let item = match (&ql.keyword, ql.is_search()) {
                (Some(k), true) => Item::new(format!("ql:{}", ql.name), &ql.name, complete(&format!("{k} ")))
                    .subtitle(format!("{k} <search>")),
                (None, true) => continue,
                (_, false) => Item::new(format!("ql:{}", ql.name), &ql.name, self.quicklink_action(&ql.url)).subtitle(&ql.url),
            };
            v.push((item.icon(sym("link")), aliases));
        }
        for a in &st.addons {
            let m = &a.manifest;
            v.push((
                Item::new(format!("addon:{}", a.id), &m.name, complete(&format!("{} ", m.keyword)))
                    .subtitle(m.description.clone().unwrap_or_else(|| format!("{} <query>", m.keyword)))
                    .icon(sym("puzzlepiece")),
                vec![m.keyword.clone()],
            ));
        }
        v
    }

    fn search_quicklink<'a>(st: &'a State, kw: &str) -> Option<&'a Quicklink> {
        st.config.quicklinks.iter().find(|q| q.is_search() && q.keyword.as_deref().is_some_and(|k| k.eq_ignore_ascii_case(kw)))
    }

    pub fn query(&self, raw: &str) -> QueryResult {
        let q = raw.trim_start();
        let done = |items| QueryResult { items, slow: false };
        if q.trim().is_empty() {
            return done(vec![]);
        }
        let st = self.state.read().unwrap();
        let t = now();

        if let Some((kw, rest)) = q.split_once(' ') {
            let rest = rest.trim();
            match kw.to_lowercase().as_str() {
                "clip" => return done(self.clip_items(rest, t)),
                "ss" => {
                    let dir = self.screenshot_dir();
                    let items = sources::screenshots(&dir, &self.opts.home, rest, t, MAX_RESULTS);
                    if items.is_empty() {
                        return done(vec![hint(format!("No screenshots in {}", sources::pretty(&dir, &self.opts.home)))]);
                    }
                    return done(self.apply_learning(q, items));
                }
                "f" if rest.is_empty() => return done(vec![hint("Type to search file names in your home folder")]),
                "f" => return QueryResult { items: vec![], slow: true },
                k => {
                    if let Some(ql) = Self::search_quicklink(&st, k) {
                        if rest.is_empty() {
                            return done(vec![hint(format!("Type to search {}", ql.name))]);
                        }
                        let url = ql.url.replace("{query}", &url_encode(rest));
                        return done(vec![
                            Item::new(format!("ql:{}", ql.name), format!("Search {} for “{rest}”", ql.name), Action::OpenUrl { url })
                                .icon(Icon::Symbol("magnifyingglass".into())),
                        ]);
                    }
                    if st.addons.iter().any(|a| a.manifest.keyword.eq_ignore_ascii_case(k)) {
                        return QueryResult { items: vec![], slow: true };
                    }
                }
            }
        }

        let mut items = Vec::new();
        if let Some(v) = calc::eval(q) {
            let s = calc::format(v);
            items.push(
                Item::new("calc", format!("= {s}"), Action::Copy { text: s.clone() })
                    .subtitle("↩ copy answer")
                    .icon(Icon::Symbol("equal.circle".into())),
            );
        }

        let boosts = self.learner.lock().unwrap().boosts(q, t);
        let mut m = NameMatcher::new(q);
        let cmds = self.commands(&st);
        let mut scored: Vec<((Boost, u8, u32), &Item)> = Vec::new();
        let rank = |b: Boost, (class, score): (u8, u32)| {
            // Latch and exact-query picks first, then match class, then recent use.
            (Boost { latched: b.latched, key: b.key, recent: 0 }, class, b.recent.min(50) * 1000 + score.min(999))
        };
        for it in &st.apps {
            if let Some(s) = m.score(&it.title) {
                scored.push((rank(boosts.get(&it.uid), s), it));
            }
        }
        for (it, aliases) in &cmds {
            if let Some(s) = m.score_any(std::iter::once(it.title.as_str()).chain(aliases.iter().map(String::as_str))) {
                scored.push((rank(boosts.get(&it.uid), s), it));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        items.extend(scored.into_iter().take(MAX_RESULTS).map(|(_, it)| it.clone()));
        done(items)
    }

    fn clip_items(&self, rest: &str, t: i64) -> Vec<Item> {
        let found = clips::search(&self.db.lock().unwrap(), rest, MAX_RESULTS).unwrap_or_default();
        if found.is_empty() {
            return vec![hint(if rest.is_empty() { "Clipboard history is empty" } else { "No matching clips" })];
        }
        found
            .into_iter()
            .map(|c| {
                let app = if c.app.is_empty() { String::new() } else { format!("{} · ", c.app) };
                Item::new(format!("clip:{}", c.id), clips::preview(&c.text), Action::Paste { text: c.text.clone() })
                    .subtitle(format!("{app}{} · {} chars · ↩ paste  ⌘↩ copy", ago(c.ts, t), c.text.chars().count()))
                    .icon(Icon::Symbol("doc.on.clipboard".into()))
                    .alt(Action::Copy { text: c.text })
            })
            .collect()
    }

    /// Learned picks re-rank results from sources that order themselves.
    fn apply_learning(&self, q: &str, mut items: Vec<Item>) -> Vec<Item> {
        let b = self.learner.lock().unwrap().boosts(q, now());
        items.sort_by_key(|it| {
            let x = b.get(&it.uid);
            std::cmp::Reverse((x.latched, x.key))
        });
        items
    }

    /// The slow part of a query: Spotlight and add-ons. Blocks; call it off
    /// the UI thread. A newer call cancels an older one.
    pub fn query_slow(&self, raw: &str) -> Vec<Item> {
        let q = raw.trim_start();
        let Some((kw, rest)) = q.split_once(' ') else { return vec![] };
        let rest = rest.trim();
        let kw = kw.to_lowercase();
        if kw == "f" {
            if !self.state.read().unwrap().config.file_search {
                return vec![hint("File search is off — enable it in Launcher Settings")];
            }
            self.ensure_file_index();
            return match self.search_files(rest) {
                None => vec![hint("Still indexing files — try again in a moment")],
                Some(items) if items.is_empty() => vec![hint("No matching files")],
                Some(items) => self.apply_learning(q, items),
            };
        }
        let addon = self.state.read().unwrap().addons.iter().find(|a| a.manifest.keyword.eq_ignore_ascii_case(&kw)).cloned();
        let Some(a) = addon else { return vec![] };
        let Some(out) = self.runner.run(&a.argv(rest), Some(&a.dir), a.timeout()) else { return vec![] };
        match addons::parse_output(&a.id, &out) {
            Ok(items) if items.is_empty() => vec![hint("No results")],
            Ok(items) => self.apply_learning(q, items),
            Err(e) => vec![hint(format!("{} returned bad output: {e}", a.manifest.name))],
        }
    }

    pub fn cancel_slow(&self) {
        self.runner.cancel();
    }

    pub fn picked(&self, raw: &str, uid: &str) {
        if uid == "calc" || uid.starts_with("hint:") {
            return;
        }
        let db = self.db.lock().unwrap();
        let _ = self.learner.lock().unwrap().record(&db, raw, uid, now());
    }

    pub fn clipboard_add(&self, text: &str, app: &str) {
        let _ = clips::add(&self.db.lock().unwrap(), text, app, now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (tempfile::TempDir, Engine) {
        let d = tempfile::tempdir().unwrap();
        let home = d.path().join("home");
        for app in ["Safari", "Slack", "Terminal", "Visual Studio Code", "Simulator"] {
            std::fs::create_dir_all(home.join(format!("Applications/{app}.app"))).unwrap();
        }
        std::fs::create_dir_all(home.join("Desktop")).unwrap();
        let e = Engine::open(Options {
            data_dir: d.path().join("data"),
            home: home.clone(),
            screenshot_dir: None,
            os_major: None,
            app_roots: Some(vec![home.join("Applications")]),
        })
        .unwrap();
        (d, e)
    }

    fn titles(r: &QueryResult) -> Vec<&str> {
        r.items.iter().map(|i| i.title.as_str()).collect()
    }

    #[test]
    fn empty_query_is_empty() {
        let (_d, e) = engine();
        assert_eq!(e.query("  "), QueryResult { items: vec![], slow: false });
    }

    #[test]
    fn prefix_beats_fuzzy_and_learning_beats_both() {
        let (_d, e) = engine();
        let r = e.query("s");
        assert!(titles(&r).iter().position(|t| *t == "Safari") < titles(&r).iter().position(|t| *t == "Visual Studio Code"));
        e.picked("s", "cmd:ss");
        let r = e.query("s");
        assert_eq!(r.items[0].uid, "cmd:ss");
        assert_eq!(e.query("vsc").items[0].title, "Visual Studio Code");
    }

    #[test]
    fn latch_holds_against_one_other_pick() {
        let (_d, e) = engine();
        let simulator = e.query("si").items.iter().find(|i| i.title == "Simulator").unwrap().uid.clone();
        let slack = e.query("si").items.iter().find(|i| i.title == "Slack").map(|i| i.uid.clone());
        e.picked("si", &simulator);
        e.picked("si", &simulator);
        if let Some(slack) = slack {
            e.picked("si", &slack);
        }
        assert_eq!(e.query("si").items[0].uid, simulator);
    }

    #[test]
    fn calculator_shows_first() {
        let (_d, e) = engine();
        let r = e.query("12*3");
        assert_eq!(r.items[0].title, "= 36");
        assert_eq!(r.items[0].action, Action::Copy { text: "36".into() });
    }

    #[test]
    fn clipboard_history_round_trip() {
        let (_d, e) = engine();
        assert_eq!(e.query("clip ").items[0].uid, "hint:");
        e.clipboard_add("first thing", "Notes");
        e.clipboard_add("second thing", "Safari");
        let r = e.query("clip ");
        assert_eq!(titles(&r), ["second thing", "first thing"]);
        assert_eq!(r.items[0].action, Action::Paste { text: "second thing".into() });
        assert_eq!(titles(&e.query("clip fir")), ["first thing"]);
    }

    #[test]
    fn search_quicklink_and_plain_quicklink() {
        let (_d, e) = engine();
        let r = e.query("g rust lang");
        assert_eq!(r.items[0].action, Action::OpenUrl { url: "https://www.google.com/search?q=rust%20lang".into() });
        let r = e.query("gh");
        assert_eq!(r.items[0].action, Action::OpenUrl { url: "https://github.com".into() });
        let r = e.query("downloads");
        assert!(matches!(&r.items[0].action, Action::Open { path } if path.ends_with("home/Downloads")));
    }

    #[test]
    fn keywords_route_to_slow_sources() {
        let (_d, e) = engine();
        assert!(e.query("f readme").slow);
        assert!(!e.query("f ").slow);
        assert!(!e.query("unknown words").slow);
        assert_eq!(e.query("ss ").items[0].uid, "hint:");
        // "f" alone is a normal search that offers the files command.
        assert!(e.query("f").items.iter().any(|i| i.uid == "cmd:f"));
    }

    #[test]
    fn addon_query_runs_the_program() {
        let (d, e) = engine();
        let dir = d.path().join("data/addons/echo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("addon.toml"), "name='Echo'\nkeyword='echo'\nexec='run.sh'\n").unwrap();
        let script = dir.join("run.sh");
        std::fs::write(&script, "#!/bin/sh\necho '{\"items\":[{\"uid\":\"a\",\"title\":\"got '\"$1\"'\",\"action\":{\"type\":\"none\"}}]}'\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Force a rescan past the throttle.
        *e.state.write().unwrap() = Engine::load_state(&e.opts);
        assert!(e.query("echo hi").slow);
        assert_eq!(e.query_slow("echo hi")[0].title, "got hi");
        assert!(e.query("ech").items.iter().any(|i| i.title == "Echo"));
    }
}
