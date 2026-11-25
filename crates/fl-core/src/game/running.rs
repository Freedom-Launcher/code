//! Game processes per instance, so the launcher knows what is running and
//! does not start the same instance twice.

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::events::{Event, Events};
use crate::Core;

pub struct Running {
    inner: Mutex<Inner>,
    /// Wakes the supervisor thread after any state mutation.
    kick: std::sync::mpsc::Sender<()>,
}

impl Default for Running {
    fn default() -> Self {
        // No supervisor attached: the receiver is dropped at once, so kicks
        // are harmless no-ops.
        let (r, _) = Self::new();
        r
    }
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
        Exit {
            code,
            stopped,
            secs,
            at: crate::unix_now(),
            crashed: !stopped && code != Some(0),
        }
    }
}

/// Drops processes that have exited; returns their `(id, exit)` pairs. The
/// existing callers ignore the return value — the supervisor derives its
/// events from the `exits` map instead, so a `stop` is reported the same way.
fn reap(g: &mut Inner) -> Vec<(String, Exit)> {
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
    g.exits.extend(ended.clone());
    for (id, _) in &done {
        if let Some(mut c) = g.after_exit.remove(id) {
            let _ = c.spawn();
        }
    }
    g.finished.extend(done);
    ended
}

impl Running {
    /// A fresh registry. Returns the wake-up receiver for [`supervise`]:
    /// one supervisor thread per registry drains it.
    pub(crate) fn new() -> (Self, std::sync::mpsc::Receiver<()>) {
        let (kick, rx) = std::sync::mpsc::channel();
        (
            Self {
                inner: Mutex::new(Inner::default()),
                kick,
            },
            rx,
        )
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wakes the supervisor; a dropped receiver (tests, plain registries)
    /// makes this a harmless no-op.
    fn kick(&self) {
        let _ = self.kick.send(());
    }

    /// Reserves the instance for a launch; `false` if it is starting or running.
    pub fn begin(&self, id: &str) -> bool {
        let mut g = self.lock();
        reap(&mut g);
        if g.children.contains_key(id) || g.starting.contains(id) {
            return false;
        }
        g.starting.insert(id.to_owned());
        drop(g);
        self.kick();
        true
    }

    /// The launch failed before a process existed.
    pub fn cancel(&self, id: &str) {
        self.lock().starting.remove(id);
        self.kick();
    }

    pub fn started(&self, id: &str, child: Child) {
        let mut g = self.lock();
        g.starting.remove(id);
        g.children.insert(id.to_owned(), (child, Instant::now()));
        g.exits.remove(id);
        drop(g);
        self.kick();
    }

    /// How each instance's last session ended (only instances that ran since the launcher started).
    pub fn exits(&self) -> HashMap<String, Exit> {
        let mut g = self.lock();
        reap(&mut g);
        g.exits.clone()
    }

    /// Runs `cmd` after this instance's game exits.
    pub fn on_exit(&self, id: &str, cmd: std::process::Command) {
        self.lock().after_exit.insert(id.to_owned(), cmd);
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
                let secs = t.elapsed().as_secs();
                g.finished.push((id.to_owned(), secs));
                g.exits.insert(id.to_owned(), Exit::new(None, true, secs));
                if let Some(mut c) = g.after_exit.remove(id) {
                    let _ = c.spawn();
                }
                drop(g);
                self.kick();
                true
            }
            None => false,
        }
    }
}

/// Body of the supervisor thread (one per core, spawned by `Core::new`).
///
/// Reaps exited game processes, flushes playtime and turns every state change
/// into an event — the single place that publishes, so the UI sees one ordered
/// stream (`game_exit` before the `game_running` it belongs to). With no game
/// up the thread parks on the channel with no timer at all; while a game runs
/// it wakes on mutations or at least once a second, the only way a natural
/// exit can surface (`try_wait`).
pub(crate) fn supervise(core: Weak<Core>, rx: std::sync::mpsc::Receiver<()>, events: Arc<Events>) {
    // Playtime bookkeeping stays core-internal: `record_playtime` only awaits
    // the state lock and does sync fs work under it, so a tiny current-thread
    // runtime driven by `block_on` is enough here.
    let rt = match tokio::runtime::Builder::new_current_thread().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("fl-game-watch: {e}");
            return;
        }
    };
    // The kick senders live inside the core, so the first kick implies the
    // core is built; parking here first also closes the `new_cyclic` race.
    if rx.recv().is_err() {
        return; // the registry is gone before anything ran: nothing to watch
    }
    // Last published `(running, starting)`: a fresh core has none.
    let mut last: Option<(Vec<String>, Vec<String>)> = Some((Vec::new(), Vec::new()));
    // Exits already published. Both `reap` (natural exits) and `stop` write
    // into `exits`; `Exit` is `PartialEq`, so a re-exit of the same instance
    // shows up as a change.
    let mut seen: HashMap<String, Exit> = HashMap::new();
    loop {
        // 1. Snapshot under the lock only; the guard is dropped before any
        //    blocking work (event delivery, playtime, parking).
        let Some(core) = core.upgrade() else { return };
        let (running, starting, ended, has_children) = {
            let mut g = core.game_running.lock();
            reap(&mut g);
            let mut run: Vec<String> = g.children.keys().cloned().collect();
            let mut st: Vec<String> = g.starting.iter().cloned().collect();
            run.sort();
            st.sort();
            let mut ended = Vec::new();
            for (id, exit) in &g.exits {
                if seen.get(id) != Some(exit) {
                    ended.push((id.clone(), exit.clone()));
                }
            }
            seen = g.exits.clone();
            let has_children = !g.children.is_empty();
            (run, st, ended, has_children)
        };
        // 2. Emit outside the lock; delivery can never fail the core.
        for (id, exit) in &ended {
            events.send(Event::GameExit {
                instance: id.clone(),
                exit: exit.clone(),
            });
        }
        let changed = match &last {
            Some((r, s)) => r != &running || s != &starting,
            None => false,
        };
        if changed {
            events.send(Event::GameRunning {
                running: running.clone(),
                starting: starting.clone(),
            });
        }
        last = Some((running, starting));
        if !ended.is_empty() {
            // Sessions ended: hand them to the playtime bookkeeping.
            let _ = rt.block_on(super::service::record_playtime(&core));
        }
        drop(core); // the strong handle must not keep the core alive while parked
                    // 3. Park. Idle: block indefinitely — zero wake-ups. A game running:
                    // wake on kick or at least once a second to catch natural exits.
        match if has_children {
            rx.recv_timeout(Duration::from_secs(1))
        } else {
            rx.recv()
                .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
        } {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
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

    /// Waits for a matching event with a deadline (the supervisor ticks at 1 Hz).
    fn wait_event(
        rx: &mut tokio::sync::broadcast::Receiver<Event>,
        ms: u64,
        f: &dyn Fn(&Event) -> bool,
    ) -> Option<Event> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            match rx.try_recv() {
                Ok(ev) => {
                    if f(&ev) {
                        return Some(ev);
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return None,
            }
        }
    }

    /// A natural exit reaches the bus: `game_exit` first, then an empty
    /// `game_running`, and the supervisor flushes playtime itself.
    #[test]
    fn supervisor_reports_a_natural_exit() {
        let dir = std::env::temp_dir().join(format!("fl-sup-{}", uuid::Uuid::new_v4()));
        let c = Core::new(dir.clone(), Instant::now(), false).unwrap();
        let mut rx = c.subscribe();
        assert!(c.game_running.begin("i1"));
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameRunning { running, starting }
                if running.is_empty() && starting.len() == 1 && starting[0] == "i1"
            ))
            .is_some(),
            "no starting snapshot"
        );
        let mut child = Command::new("true").spawn().unwrap();
        let _ = child.wait();
        c.game_running.started("i1", child);
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameRunning { running, .. } if running.len() == 1 && running[0] == "i1"
            ))
            .is_some(),
            "no running snapshot"
        );
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameExit { instance, .. } if instance == "i1"
            ))
            .is_some(),
            "no exit event"
        );
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameRunning { running, starting }
                if running.is_empty() && starting.is_empty()
            ))
            .is_some(),
            "no empty snapshot after the exit"
        );
        // The supervisor's playtime flush writes instances.json; afterwards
        // nothing is left for the pollers.
        for _ in 0..100 {
            if dir.join("instances.json").exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(dir.join("instances.json").exists(), "playtime not flushed");
        assert!(c.game_running.take_finished().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `stop` writes its exit directly into the registry; the supervisor's
    /// per-key diff reports it (stopped, not a crash) and the list empties.
    #[test]
    fn supervisor_reports_stop() {
        let dir = std::env::temp_dir().join(format!("fl-sup2-{}", uuid::Uuid::new_v4()));
        let c = Core::new(dir.clone(), Instant::now(), false).unwrap();
        let mut rx = c.subscribe();
        assert!(c.game_running.begin("i1"));
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        c.game_running.started("i1", child);
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameRunning { running, .. } if running.len() == 1 && running[0] == "i1"
            ))
            .is_some(),
            "no running snapshot"
        );
        assert!(c.game_running.stop("i1"));
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameExit { instance, exit } if instance == "i1" && exit.stopped
            ))
            .is_some(),
            "no stopped exit event"
        );
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameRunning { running, .. } if running.is_empty()
            ))
            .is_some(),
            "running list did not empty"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A second run of the same instance ends with a different exit: the
    /// per-key diff must see it even though the key existed before.
    #[test]
    fn supervisor_sees_a_reexit_of_the_same_instance() {
        let dir = std::env::temp_dir().join(format!("fl-sup3-{}", uuid::Uuid::new_v4()));
        let c = Core::new(dir.clone(), Instant::now(), false).unwrap();
        let mut rx = c.subscribe();
        assert!(c.game_running.begin("i1"));
        let mut child = Command::new("true").spawn().unwrap();
        let _ = child.wait();
        c.game_running.started("i1", child);
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameExit { instance, .. } if instance == "i1"
            ))
            .is_some(),
            "no first exit event"
        );
        assert!(c.game_running.begin("i1"));
        let mut child = Command::new("false").spawn().unwrap();
        let _ = child.wait();
        c.game_running.started("i1", child);
        assert!(
            wait_event(&mut rx, 5000, &|ev| matches!(ev,
                Event::GameExit { instance, exit }
                if instance == "i1" && exit.crashed && exit.code == Some(1)
            ))
            .is_some(),
            "no second exit event"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
