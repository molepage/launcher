//! Try the core from a terminal: `lq [--data DIR] <query>...`
//! Uses a throwaway data folder unless --data is given.
use launcher_core::engine::{Engine, Options};
use std::time::Instant;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
    let data = match args.iter().position(|a| a == "--data") {
        Some(i) => { args.remove(i); std::path::PathBuf::from(args.remove(i)) }
        None => std::env::temp_dir().join("lq-data"),
    };
    let t = Instant::now();
    let shots = home.join("Library/Mobile Documents/com~apple~CloudDocs/Screenshots");
    let e = Engine::open(Options { data_dir: data, home, screenshot_dir: Some(shots) }).expect("open");
    println!("open: {:?}", t.elapsed());
    while e.settings()["indexed_files"].is_null() {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    println!("file index ready: {:?}, {} entries", t.elapsed(), e.settings()["indexed_files"]);
    for q in &args {
        let t = Instant::now();
        let r = e.query(q);
        let fast = t.elapsed();
        let mut items = r.items;
        let mut slow_t = None;
        if r.slow {
            let t = Instant::now();
            items = e.query_slow(q);
            slow_t = Some(t.elapsed());
        }
        println!("\n{q:?}: fast {fast:?}{}", slow_t.map(|d| format!(", slow {d:?}")).unwrap_or_default());
        for it in items.iter().take(6) {
            println!("  {:<40} {}", it.title, it.subtitle);
        }
    }
}
