//! Game processes per instance, so the launcher knows what is running and
//! does not start the same instance twice.

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Default)]
pub struct Running {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    children: HashMap<String, (Child, Instant)>,
    /// Sessions that ended and are not yet added to playtime: (id, seconds).
    finished: Vec<(String, u64)>,
    /// Instances between «Играть» and a spawned process (install, Java).
    starting: HashSet<String>,
}

/// Drops processes that have exited.
fn reap(g: &mut Inner) {
    let mut done = Vec::new();
    g.children.retain(|id, (c, t)| {
        let alive = matches!(c.try_wait(), Ok(None));
        if !alive {
            done.push((id.clone(), t.elapsed().as_secs()));
        }
        alive
    });
    g.finished.extend(done);
}

impl Running {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("running lock")
    }

    /// Reserves the instance for a launch; `false` if it is starting or running.
    pub fn begin(&self, id: &str) -> bool {
        let mut g = self.lock();
        reap(&mut g);
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

    pub fn started(&self, id: &str, child: Child) {
        let mut g = self.lock();
        g.starting.remove(id);
        g.children.insert(id.to_owned(), (child, Instant::now()));
    }

    /// Ended sessions since the last call, for playtime bookkeeping.
    pub fn take_finished(&self) -> Vec<(String, u64)> {
        let mut g = self.lock();
        reap(&mut g);
        std::mem::take(&mut g.finished)
    }

    pub fn is_busy(&self, id: &str) -> bool {
        let mut g = self.lock();
        reap(&mut g);
        g.children.contains_key(id) || g.starting.contains(id)
    }

    /// `(running, starting)` instance ids.
    pub fn list(&self) -> (Vec<String>, Vec<String>) {
        let mut g = self.lock();
        reap(&mut g);
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
                g.finished.push((id.to_owned(), t.elapsed().as_secs()));
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
    fn exited_process_is_reaped() {
        let r = Running::default();
        assert!(r.begin("i2"));
        let mut c = Command::new("true").spawn().unwrap();
        let _ = c.wait();
        r.started("i2", c);
        assert_eq!(r.list().0, Vec::<String>::new());
    }
}
