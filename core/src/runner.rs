//! Runs one external program at a time (add-ons, Spotlight). Starting a new
//! run kills the previous one, so fast typing never piles up processes.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

const MAX_OUTPUT: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub struct Runner {
    current: Mutex<Option<(u64, Child)>>,
    generation: AtomicU64,
}

fn kill_group(child: &mut Child) {
    // Each run is its own process group, so scripts' children die too.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl Runner {
    /// Stdout of the program, or `None` if it failed to start, timed out,
    /// exited non-zero, or was superseded by a newer run.
    pub fn run(&self, argv: &[String], cwd: Option<&Path>, timeout: Duration) -> Option<String> {
        use std::os::unix::process::CommandExt;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.cancel_current();

        let (prog, args) = argv.split_first()?;
        let mut cmd = Command::new(prog);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).process_group(0);
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        // ETXTBSY: the script was just written (e.g. an editor saving it) — retry briefly.
        let mut child = (0..5)
            .find_map(|_| match cmd.spawn() {
                Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) => {
                    std::thread::sleep(Duration::from_millis(20));
                    None
                }
                r => Some(r),
            })?
            .ok()?;
        let stdout = child.stdout.take()?;
        *self.current.lock().unwrap() = Some((generation, child));

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.take(MAX_OUTPUT).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
        let out = rx.recv_timeout(timeout);

        let status = {
            let mut slot = self.current.lock().unwrap();
            match slot.take() {
                Some((g, mut c)) if g == generation => {
                    let mut st = c.try_wait().ok().flatten();
                    // Stdout closed: give the process a moment to report its exit.
                    for _ in 0..50 {
                        if st.is_some() || out.is_err() {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(1));
                        st = c.try_wait().ok().flatten();
                    }
                    if st.is_none() {
                        kill_group(&mut c);
                    }
                    st
                }
                other => {
                    *slot = other; // a newer run owns the slot
                    None
                }
            }
        };
        let fresh = self.generation.load(Ordering::SeqCst) == generation;
        match (out, status) {
            (Ok(bytes), Some(st)) if fresh && st.success() => Some(String::from_utf8_lossy(&bytes).into_owned()),
            _ => None,
        }
    }

    /// Kill whatever is running (the user cleared or changed the query).
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.cancel_current();
    }

    fn cancel_current(&self) {
        if let Some((_, mut c)) = self.current.lock().unwrap().take() {
            kill_group(&mut c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    fn sh(s: &str) -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), s.into()]
    }

    #[test]
    fn returns_stdout() {
        let r = Runner::default();
        assert_eq!(r.run(&sh("echo hi"), None, Duration::from_secs(2)).as_deref(), Some("hi\n"));
    }

    #[test]
    fn failure_and_timeout_give_none() {
        let r = Runner::default();
        assert_eq!(r.run(&sh("exit 3"), None, Duration::from_secs(2)), None);
        let t = Instant::now();
        assert_eq!(r.run(&sh("sleep 5; echo late"), None, Duration::from_millis(200)), None);
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn new_run_supersedes_old() {
        let r = Arc::new(Runner::default());
        let r2 = r.clone();
        let old = std::thread::spawn(move || r2.run(&sh("sleep 3; echo old"), None, Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(150));
        let t = Instant::now();
        assert_eq!(r.run(&sh("echo new"), None, Duration::from_secs(2)).as_deref(), Some("new\n"));
        assert_eq!(old.join().unwrap(), None);
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
