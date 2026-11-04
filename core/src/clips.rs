//! Clipboard history (text). The shell reports copies; the core stores them.

use rusqlite::{Connection, params};

pub const MAX_ENTRIES: i64 = 1000;
pub const MAX_AGE: i64 = 90 * 24 * 3600;
pub const MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub id: i64,
    pub text: String,
    pub app: String,
    pub ts: i64,
}

pub fn init(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clips(id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE,
                                          app TEXT NOT NULL DEFAULT '', ts INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS clips_ts ON clips(ts);",
    )?;
    prune(conn, now)
}

fn prune(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM clips WHERE ts < ?1", params![now - MAX_AGE])?;
    conn.execute(
        "DELETE FROM clips WHERE id NOT IN (SELECT id FROM clips ORDER BY ts DESC, id DESC LIMIT ?1)",
        params![MAX_ENTRIES],
    )?;
    Ok(())
}

/// Store a copy. Copying something already in history moves it to the top.
pub fn add(conn: &Connection, text: &str, app: &str, now: i64) -> rusqlite::Result<bool> {
    if text.trim().is_empty() || text.len() > MAX_BYTES {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO clips(text, app, ts) VALUES (?1, ?2, ?3)
         ON CONFLICT(text) DO UPDATE SET ts = excluded.ts, app = excluded.app",
        params![text, app, now],
    )?;
    prune(conn, now)?;
    Ok(true)
}

pub fn search(conn: &Connection, query: &str, limit: usize) -> rusqlite::Result<Vec<Clip>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, text, app, ts FROM clips
         WHERE ?1 = '' OR instr(lower(text), lower(?1)) > 0
         ORDER BY ts DESC, id DESC LIMIT ?2",
    )?;
    stmt.query_map(params![query, limit as i64], |r| {
        Ok(Clip { id: r.get(0)?, text: r.get(1)?, app: r.get(2)?, ts: r.get(3)? })
    })?
    .collect()
}

/// One-line preview of a clip for the result title.
pub fn preview(text: &str) -> String {
    let line = text.trim().lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut s: String = line.chars().take(100).collect();
    if line.chars().count() > 100 || text.trim().lines().count() > 1 {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init(&c, 0).unwrap();
        c
    }

    #[test]
    fn dedupes_and_moves_to_top() {
        let c = db();
        add(&c, "hello", "A", 10).unwrap();
        add(&c, "world", "B", 20).unwrap();
        add(&c, "hello", "C", 30).unwrap();
        let all = search(&c, "", 10).unwrap();
        assert_eq!(all.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["hello", "world"]);
        assert_eq!(all[0].app, "C");
    }

    #[test]
    fn search_is_case_insensitive_and_literal() {
        let c = db();
        add(&c, "Hello World", "", 1).unwrap();
        add(&c, "100% done_now", "", 2).unwrap();
        assert_eq!(search(&c, "world", 10).unwrap().len(), 1);
        assert_eq!(search(&c, "0%", 10).unwrap().len(), 1);
        assert_eq!(search(&c, "e_n", 10).unwrap().len(), 1);
        assert_eq!(search(&c, "nope", 10).unwrap().len(), 0);
    }

    #[test]
    fn skips_blank_and_caps_entries() {
        let c = db();
        assert!(!add(&c, "  \n", "", 1).unwrap());
        for i in 0..(MAX_ENTRIES + 5) {
            add(&c, &format!("t{i}"), "", i).unwrap();
        }
        let n: i64 = c.query_row("SELECT count(*) FROM clips", [], |r| r.get(0)).unwrap();
        assert_eq!(n, MAX_ENTRIES);
    }

    #[test]
    fn preview_takes_first_line() {
        assert_eq!(preview("\n  first\nsecond"), "first…");
        assert_eq!(preview("short"), "short");
    }
}
