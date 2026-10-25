//! Game processes per instance, so the launcher knows what is running and
//! does not start the same instance twice.

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Default)]
pub struct Running {
    inner: Mutex<Inner>,
    /// `<data>/logs/games.log`: one line per game start and end, kept across launcher
    /// restarts so a diagnostics archive shows how the last sessions ended.
    journal: std::sync::OnceLock<std::path::PathBuf>,
}

fn journal_write(p: &std::path::Path, line: &str) {
    let mut log = std::fs::read_to_string(p).unwrap_or_default();
    if log.len() > 64 * 1024 {
        let cut = log.len() - 32 * 1024;
        let cut = (cut..log.len()).find(|&i| log.is_char_boundary(i)).unwrap_or(log.len());
        log = log[cut..].to_owned();
    }
    log.push_str(&format!("{} {line}\n", crate::unix_now()));
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, log);
}

fn exit_line(id: &str, e: &Exit) -> String {
    let how = if e.stopped {
        "stopped".to_owned()
    } else {
        match e.code {
            Some(c) => format!("code {c} (0x{:08x})", c as u32),
            None => "killed".to_owned(),
        }
    };
    format!("exit {id} {how} secs {}", e.secs)
}

#[derive(Default)]
struct Inner {
    children: HashMap<String, (Child, Instant)>,
    /// Sessions that ended and are not yet added to playtime: (id, seconds).
    finished: Vec<(String, u64)>,
    /// Instances between «Играть» and a spawned process (install, Java).
    starting: HashSet<String>,
    /// Commands to run once a game exits (post-exit hook).
    after_exit: HashMap<String, std::process::Command>,
    /// How each instance's last game session ended, for the crash report.
    exits: HashMap<String, Exit>,
}

/// How a game session ended.
#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct Exit {
    /// Process exit code; `None` when it was killed by a signal.
    pub code: Option<i32>,
    /// Closed from the launcher («Закрыть игру»), so never a crash.
    pub stopped: bool,
    /// Session length.
    pub secs: u64,
    /// When it ended (unix seconds).
    pub at: u64,
    pub crashed: bool,
}

impl Exit {
    fn new(code: Option<i32>, stopped: bool, secs: u64) -> Self {
        Exit { code, stopped, secs, at: crate::unix_now(), crashed: !stopped && code != Some(0) }
    }
}

/// Drops processes that have exited.
fn reap(g: &mut Inner, journal: Option<&std::path::Path>) {
    let mut done = Vec::new();
    let mut ended = Vec::new();
    g.children.retain(|id, (c, t)| {
        let code = match c.try_wait() {
            Ok(None) => return true,
            Ok(Some(st)) => st.code(),
            Err(_) => None,
        };
        let secs = t.elapsed().as_secs();
        done.push((id.clone(), secs));
        ended.push((id.clone(), Exit::new(code, false, secs)));
        false
    });
    if let Some(p) = journal {
        for (id, e) in &ended {
            journal_write(p, &exit_line(id, e));
        }
    }
    g.exits.extend(ended);
    for (id, _) in &done {
        if let Some(mut c) = g.after_exit.remove(id) {
            let _ = c.spawn();
        }
    }
    g.finished.extend(done);
}

impl Running {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Reserves the instance for a launch; `false` if it is starting or running.
    pub fn begin(&self, id: &str) -> bool {
        let mut g = self.lock();
        reap(&mut g, self.journal.get().map(|p| p.as_path()));
        if g.children.contains_key(id) || g.starting.contains(id) {
            return false;
        }
        g.starting.insert(id.to_owned());
        true
    }

    /// The launch failed before a process existed.
    pub fn cancel(&self, id: &str) {
        self.lock().starting.remove(id);
    }

    /// Keeps the start/end journal in `path` (`logs/games.log`).
    pub fn set_journal(&self, path: std::path::PathBuf) {
        let _ = self.journal.set(path);
    }

    pub fn started(&self, id: &str, child: Child) {
        if let Some(p) = self.journal.get() {
            journal_write(p, &format!("start {id} pid {} launcher {}", child.id(), crate::VERSION));
        }
        let mut g = self.lock();
        g.starting.remove(id);
        g.children.insert(id.to_owned(), (child, Instant::now()));
        g.exits.remove(id);
    }

    /// How each instance's last session ended (only instances that ran since the launcher started).
    pub fn exits(&self) -> HashMap<String, Exit> {
        let mut g = self.lock();
        reap(&mut g, self.journal.get().map(|p| p.as_path()));
        g.exits.clone()
    }

    /// Runs `cmd` after this instance's game exits.
    pub fn on_exit(&self, id: &str, cmd: std::process::Command) {
        self.lock().after_exit.insert(id.to_owned(), cmd);
    }

    /// Ended sessions since the last call, for playtime bookkeeping.
    pub fn take_finished(&self) -> Vec<(String, u64)> {
        let mut g = self.lock();
        reap(&mut g, self.journal.get().map(|p| p.as_path()));
        std::mem::take(&mut g.finished)
    }

    pub fn is_busy(&self, id: &str) -> bool {
        let mut g = self.lock();
        reap(&mut g, self.journal.get().map(|p| p.as_path()));
        g.children.contains_key(id) || g.starting.contains(id)
    }

    /// `(running, starting)` instance ids.
    pub fn list(&self) -> (Vec<String>, Vec<String>) {
        let mut g = self.lock();
        reap(&mut g, self.journal.get().map(|p| p.as_path()));
        let mut run: Vec<String> = g.children.keys().cloned().collect();
        let mut st: Vec<String> = g.starting.iter().cloned().collect();
        run.sort();
        st.sort();
        (run, st)
    }

    /// Kills the instance's game; `false` if it was not running.
    pub fn stop(&self, id: &str) -> bool {
        let mut g = self.lock();
        match g.children.remove(id) {
            Some((mut c, t)) => {
                let _ = c.kill();
                let _ = c.wait();
                let secs = t.elapsed().as_secs();
                g.finished.push((id.to_owned(), secs));
                let e = Exit::new(None, true, secs);
                if let Some(p) = self.journal.get() {
                    journal_write(p, &exit_line(id, &e));
                }
                g.exits.insert(id.to_owned(), e);
                if let Some(mut c) = g.after_exit.remove(id) {
                    let _ = c.spawn();
                }
                true
            }
            None => false,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn tracks_one_process_per_instance() {
        let r = Running::default();
        assert!(r.begin("i1"));
        assert!(!r.begin("i1"), "second press while starting");
        assert_eq!(r.list().1, vec!["i1".to_string()]);
        r.started("i1", Command::new("sleep").arg("5").spawn().unwrap());
        assert!(!r.begin("i1"), "second press while running");
        assert_eq!(r.list(), (vec!["i1".to_string()], vec![]));
        assert!(r.stop("i1"));
        assert_eq!(r.list(), (vec![], vec![]));
        assert_eq!(r.take_finished().len(), 1);
        assert!(r.take_finished().is_empty());
        assert!(r.begin("i1"));
        r.cancel("i1");
        assert!(r.begin("i1"));
    }

    #[test]
    fn journal_keeps_starts_and_ends() {
        let p = std::env::temp_dir().join(format!("fl-games-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&p);
        let r = Running::default();
        r.set_journal(p.clone());
        assert!(r.begin("j1"));
        let mut c = Command::new("false").spawn().unwrap();
        let _ = c.wait();
        r.started("j1", c);
        let _ = r.list();
        assert!(r.begin("j2"));
        r.started("j2", Command::new("sleep").arg("5").spawn().unwrap());
        assert!(r.stop("j2"));
        let log = std::fs::read_to_string(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert!(log.contains("start j1 pid "), "{log}");
        assert!(log.contains("exit j1 code 1 (0x00000001) secs "), "{log}");
        assert!(log.contains("exit j2 stopped secs "), "{log}");
        assert!(log.contains(&format!("launcher {}", crate::VERSION)), "{log}");
    }

    #[test]
    fn exited_process_is_reaped() {
        let r = Running::default();
        assert!(r.begin("i2"));
        let mut c = Command::new("true").spawn().unwrap();
        let _ = c.wait();
        r.started("i2", c);
        assert_eq!(r.list().0, Vec::<String>::new());
        let e = r.exits().remove("i2").unwrap();
        assert_eq!(e.code, Some(0));
        assert!(!e.crashed);
    }

    #[test]
    fn failed_exit_is_a_crash_but_stop_is_not() {
        let r = Running::default();
        assert!(r.begin("i3"));
        let mut c = Command::new("false").spawn().unwrap();
        let _ = c.wait();
        r.started("i3", c);
        let e = r.exits().remove("i3").unwrap();
        assert!(e.crashed && !e.stopped && e.code == Some(1));
        assert!(r.begin("i4"));
        r.started("i4", Command::new("sleep").arg("5").spawn().unwrap());
        assert!(r.stop("i4"));
        let e = r.exits().remove("i4").unwrap();
        assert!(e.stopped && !e.crashed);
    }
}
