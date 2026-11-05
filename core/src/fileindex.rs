//! In-memory index of file and folder names under home (plus iCloud Drive).
//!
//! Spotlight takes ~2 s per query on a real machine, so we keep our own:
//! a compact tree (parent pointer + name in one shared string), about
//! 25 bytes per entry. A walk of ~500k entries takes about a second and
//! runs on a background thread; searching is a parallel fuzzy pass over
//! the names.

use crate::matcher::NameMatcher;
use std::path::{Path, PathBuf};
use std::time::Instant;

const ROOT: u32 = u32::MAX;

/// Folders that are noise for a person looking for a file.
const SKIP_DIRS: &[&str] =
    &["Library", "node_modules", "target", "DerivedData", "Pods", "build", "__pycache__", "venv", "bower_components"];

/// Folders macOS shows as single files; index the package, not its insides.
const PACKAGES: &[&str] = &[
    "app", "photoslibrary", "musiclibrary", "tvlibrary", "imovielibrary", "fcpbundle", "framework", "bundle",
    "xcodeproj", "xcworkspace", "playground", "pkg", "rtfd", "key", "pages", "numbers", "logicx", "band",
    "plugin", "kext", "appex", "xcarchive", "lrlibrary", "lrdata",
];

#[derive(Clone, Copy)]
struct Node {
    parent: u32,
    start: u32,
    len: u16,
}

pub struct FileIndex {
    names: String,
    nodes: Vec<Node>,
    pub built: Instant,
    /// Roots the index was built from (to detect config changes).
    pub roots: Vec<PathBuf>,
}

fn is_package(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| PACKAGES.contains(&ext.to_ascii_lowercase().as_str()))
}

impl FileIndex {
    pub fn build(roots: &[PathBuf]) -> FileIndex {
        let mut ix = FileIndex { names: String::new(), nodes: Vec::new(), built: Instant::now(), roots: roots.to_vec() };
        for root in roots {
            if !root.is_dir() {
                continue;
            }
            let r = ix.push(ROOT, &root.to_string_lossy());
            let mut stack = vec![(root.clone(), r)];
            while let Some((dir, idx)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&dir) else { continue };
                for e in rd.flatten() {
                    let name = e.file_name();
                    let Some(name) = name.to_str() else { continue };
                    if name.starts_with('.') || name.len() > u16::MAX as usize {
                        continue;
                    }
                    let Ok(ft) = e.file_type() else { continue };
                    if ft.is_dir() && SKIP_DIRS.contains(&name) {
                        continue;
                    }
                    let child = ix.push(idx, name);
                    if ft.is_dir() && !is_package(name) {
                        stack.push((e.path(), child));
                    }
                }
            }
        }
        ix.names.shrink_to_fit();
        ix.nodes.shrink_to_fit();
        ix.built = Instant::now();
        ix
    }

    fn push(&mut self, parent: u32, name: &str) -> u32 {
        let start = self.names.len() as u32;
        self.names.push_str(name);
        self.nodes.push(Node { parent, start, len: name.len() as u16 });
        (self.nodes.len() - 1) as u32
    }

    fn name(&self, i: usize) -> &str {
        let n = self.nodes[i];
        &self.names[n.start as usize..n.start as usize + n.len as usize]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn path(&self, i: usize) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = i as u32;
        while cur != ROOT {
            parts.push(self.name(cur as usize));
            cur = self.nodes[cur as usize].parent;
        }
        let mut p = PathBuf::new();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }

    fn depth(&self, i: usize) -> u32 {
        let mut d = 0;
        let mut cur = self.nodes[i].parent;
        while cur != ROOT {
            d += 1;
            cur = self.nodes[cur as usize].parent;
        }
        d
    }

    /// The best `limit` matches by name, as paths. Ties go to shallower paths.
    pub fn search(&self, query: &str, limit: usize) -> Vec<PathBuf> {
        if query.trim().is_empty() || self.nodes.is_empty() {
            return vec![];
        }
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let chunk = self.nodes.len().div_ceil(threads);
        let mut hits: Vec<((u8, u32), usize)> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    s.spawn(move || {
                        let mut m = NameMatcher::new(query);
                        let lo = t * chunk;
                        let hi = ((t + 1) * chunk).min(self.nodes.len());
                        let mut v = Vec::new();
                        for i in lo..hi {
                            if self.nodes[i].parent == ROOT {
                                continue;
                            }
                            if let Some(sc) = m.score(self.name(i)) {
                                v.push((sc, i));
                            }
                        }
                        v
                    })
                })
                .collect();
            handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
        });
        let keep = (limit * 4).min(hits.len());
        if keep == 0 {
            return vec![];
        }
        hits.select_nth_unstable_by(keep - 1, |a, b| b.0.cmp(&a.0));
        hits.truncate(keep);
        let mut ranked: Vec<_> = hits.into_iter().map(|(sc, i)| (sc, self.depth(i), i)).collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        ranked.into_iter().take(limit).map(|(_, _, i)| self.path(i)).collect()
    }
}

/// Home plus iCloud Drive (which lives under ~/Library, otherwise skipped).
pub fn default_roots(home: &Path) -> Vec<PathBuf> {
    vec![home.to_path_buf(), home.join("Library/Mobile Documents/com~apple~CloudDocs")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_and_skips_noise() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for f in [
            "Documents/report.pdf",
            "Documents/old/my-report.docx",
            "code/proj/node_modules/report.js",
            "code/proj/.git/report",
            "Library/Caches/report.txt",
            "Photos.photoslibrary/inner/report.jpg",
        ] {
            let p = r.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"").unwrap();
        }
        let ix = FileIndex::build(&[r.to_path_buf()]);
        let found = ix.search("report", 10);
        let names: Vec<_> = found.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["report.pdf", "my-report.docx"]);
        assert_eq!(found[0], r.join("Documents/report.pdf"));
        assert!(ix.search("photos", 5).iter().any(|p| p.ends_with("Photos.photoslibrary")));
        assert!(ix.search("", 5).is_empty());
    }
}
