//! Learning from picks:
//! - picks for this exact query count most,
//! - then picks of the item in the last 7 days for any query,
//! - an item picked for the same query twice in a row is "latched" to the top
//!   until something else is picked twice in a row for that query.
//!
//! All picks live in memory (a few thousand rows at most) and are appended
//! to SQLite so they survive restarts.

use rusqlite::{Connection, params};
use std::collections::HashMap;

pub const WEEK: i64 = 7 * 24 * 3600;
const KEEP: i64 = 90 * 24 * 3600;

#[derive(Debug, Clone)]
struct Pick {
    uid: String,
    query: String,
    ts: i64,
}

#[derive(Default)]
pub struct Learner {
    picks: Vec<Pick>,
}

/// Boosts for one query, computed once per keystroke.
#[derive(Debug, Default)]
pub struct Boosts {
    key: HashMap<String, u32>,
    recent: HashMap<String, u32>,
    latched: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Boost {
    pub latched: bool,
    pub key: u32,
    pub recent: u32,
}

impl Boosts {
    pub fn get(&self, uid: &str) -> Boost {
        Boost {
            latched: self.latched.as_deref() == Some(uid),
            key: self.key.get(uid).copied().unwrap_or(0),
            recent: self.recent.get(uid).copied().unwrap_or(0),
        }
    }
}

pub fn normalize(q: &str) -> String {
    q.trim().to_lowercase()
}

impl Learner {
    pub fn init(conn: &Connection, now: i64) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS picks(uid TEXT NOT NULL, query TEXT NOT NULL, ts INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS picks_ts ON picks(ts);",
        )?;
        conn.execute("DELETE FROM picks WHERE ts < ?1", params![now - KEEP])?;
        let mut stmt = conn.prepare("SELECT uid, query, ts FROM picks ORDER BY ts, rowid")?;
        let picks = stmt
            .query_map([], |r| Ok(Pick { uid: r.get(0)?, query: r.get(1)?, ts: r.get(2)? }))?
            .collect::<Result<_, _>>()?;
        Ok(Learner { picks })
    }

    pub fn record(&mut self, conn: &Connection, query: &str, uid: &str, now: i64) -> rusqlite::Result<()> {
        let query = normalize(query);
        conn.execute("INSERT INTO picks(uid, query, ts) VALUES (?1, ?2, ?3)", params![uid, query, now])?;
        self.picks.push(Pick { uid: uid.to_owned(), query, ts: now });
        Ok(())
    }

    pub fn boosts(&self, query: &str, now: i64) -> Boosts {
        let q = normalize(query);
        let mut b = Boosts::default();
        let mut last_two: [Option<&str>; 2] = [None, None];
        for p in &self.picks {
            if p.query == q {
                *b.key.entry(p.uid.clone()).or_default() += 1;
                last_two = [last_two[1], Some(&p.uid)];
            }
            if p.ts >= now - WEEK {
                *b.recent.entry(p.uid.clone()).or_default() += 1;
            }
        }
        if let [Some(a), Some(c)] = last_two {
            if a == c {
                b.latched = Some(a.to_owned());
            }
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Connection, Learner) {
        let c = Connection::open_in_memory().unwrap();
        let l = Learner::init(&c, 1_000_000).unwrap();
        (c, l)
    }

    #[test]
    fn latch_takes_two_picks_to_set_and_two_to_steal() {
        let (c, mut l) = setup();
        let t = 1_000_000;
        l.record(&c, "s", "safari", t).unwrap();
        assert!(!l.boosts("s", t).get("safari").latched);
        l.record(&c, "S ", "safari", t + 1).unwrap();
        assert!(l.boosts("s", t).get("safari").latched);
        l.record(&c, "s", "slack", t + 2).unwrap();
        assert!(!l.boosts("s", t).get("slack").latched);
        l.record(&c, "s", "slack", t + 3).unwrap();
        let b = l.boosts("s", t);
        assert!(b.get("slack").latched);
        assert!(!b.get("safari").latched);
    }

    #[test]
    fn recent_counts_only_last_week_and_persists() {
        let (c, mut l) = setup();
        let now = 2_000_000;
        l.record(&c, "a", "old", now - WEEK - 10).unwrap();
        l.record(&c, "b", "new", now - 10).unwrap();
        let b = l.boosts("zzz", now);
        assert_eq!(b.get("old").recent, 0);
        assert_eq!(b.get("new").recent, 1);
        assert_eq!(b.get("new").key, 0);
        let reloaded = Learner::init(&c, now).unwrap();
        assert_eq!(reloaded.boosts("b", now).get("new").key, 1);
    }

    #[test]
    fn old_picks_are_pruned() {
        let (c, mut l) = setup();
        l.record(&c, "a", "x", 0).unwrap();
        let reloaded = Learner::init(&c, KEEP + 10).unwrap();
        assert_eq!(reloaded.boosts("a", KEEP + 10).get("x").key, 0);
    }
}
