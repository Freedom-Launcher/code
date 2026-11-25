//! Concurrent downloader with SHA1 verification, retries and atomic writes.
//!
//! A file that already exists with the right hash (or the right size when no
//! hash is known) is skipped, so re-running an install is cheap.

use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use futures_util::{stream, StreamExt};
use sha1::{Digest, Sha1};

use crate::CoreError;

pub const CONCURRENCY: usize = 8;
pub(crate) const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone)]
pub struct Task {
    pub url: String,
    pub path: PathBuf,
    /// Lowercase hex; empty when unknown.
    pub sha1: String,
    pub size: u64,
}

/// Shared counters the UI can poll while an install runs.
pub struct Progress {
    pub total_files: AtomicU64,
    pub done_files: AtomicU64,
    pub total_bytes: AtomicU64,
    pub done_bytes: AtomicU64,
    stage: std::sync::Mutex<&'static str>,
    /// Change hook installed by [`Progress::attach`]; `None` for standalone
    /// counters the UI cannot see (nested installs).
    notify: std::sync::Mutex<Option<Notify>>,
    /// Created at instantiation; counter events are throttled against it.
    epoch: Instant,
    /// Milliseconds since `epoch` of the last counter emission; `u64::MAX`
    /// until the first one, so the first counter change always goes out.
    last_emit_ms: AtomicU64,
}

/// A subscriber of progress snapshots (the core's event bus).
type Notify = Arc<dyn Fn(serde_json::Value) + Send + Sync>;

/// Minimum gap between counter-driven events; stage changes ignore it.
const EMIT_EVERY: u64 = 250;

impl Default for Progress {
    fn default() -> Self {
        Self {
            total_files: AtomicU64::new(0),
            done_files: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            done_bytes: AtomicU64::new(0),
            stage: std::sync::Mutex::new(""),
            notify: std::sync::Mutex::new(None),
            epoch: Instant::now(),
            last_emit_ms: AtomicU64::new(u64::MAX),
        }
    }
}

impl Progress {
    pub fn set_stage(&self, stage: &'static str) {
        *self.stage.lock().expect("stage lock") = stage;
        self.fire(true);
    }

    /// Sends every meaningful change of this counter to the bus under `key`:
    /// stages at once, counter updates at most one per [`EMIT_EVERY`].
    pub(crate) fn attach(&self, events: Arc<crate::events::Events>, key: String) {
        *self.notify.lock().expect("notify lock") = Some(Arc::new(move |v| {
            events.send(crate::events::Event::GameProgress {
                key: key.clone(),
                progress: v,
            });
        }));
    }

    /// One file finished downloading.
    pub(crate) fn file_done(&self, bytes: u64) {
        self.done_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.done_files.fetch_add(1, Ordering::Relaxed);
        self.fire(false);
    }

    /// One file was already valid: it leaves the byte totals.
    pub(crate) fn file_skipped(&self, size: u64) {
        self.total_bytes.fetch_sub(size, Ordering::Relaxed);
        self.done_files.fetch_add(1, Ordering::Relaxed);
        self.fire(false);
    }

    /// The totals are known: one immediate event, so the UI gets `0/N` at once.
    pub(crate) fn totals_ready(&self) {
        self.fire(true);
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "stage": *self.stage.lock().expect("stage lock"),
            "total_files": self.total_files.load(Ordering::Relaxed),
            "done_files": self.done_files.load(Ordering::Relaxed),
            "total_bytes": self.total_bytes.load(Ordering::Relaxed),
            "done_bytes": self.done_bytes.load(Ordering::Relaxed),
        })
    }

    /// Sends a snapshot through the hook. Counter updates race through
    /// `compare_exchange`, so among concurrent downloaders exactly one wins
    /// per window; `force` (stages, first totals) bypasses the throttle.
    fn fire(&self, force: bool) {
        let hook = self.notify.lock().expect("notify lock").clone();
        let Some(hook) = hook else { return };
        let now = self.epoch.elapsed().as_millis() as u64;
        if force {
            self.last_emit_ms.store(now, Ordering::Relaxed);
        } else {
            let mut seen = self.last_emit_ms.load(Ordering::Relaxed);
            loop {
                if seen != u64::MAX && now.saturating_sub(seen) < EMIT_EVERY {
                    return;
                }
                match self.last_emit_ms.compare_exchange(
                    seen,
                    now,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => seen = actual,
                }
            }
        }
        hook(self.snapshot());
    }
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    let d = Sha1::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha1_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// True when `path` already holds the expected content.
pub fn is_valid(path: &Path, sha1: &str, size: u64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if size > 0 && meta.len() != size {
        return false;
    }
    if sha1.is_empty() {
        return true;
    }
    sha1_file(path).is_ok_and(|h| h.eq_ignore_ascii_case(sha1))
}

/// Downloads one file. `Ok(None)` means it was already valid and skipped.
/// The last attempt may go to a mirror, per the rule (see `mirror`).
async fn fetch_one(
    http: &reqwest::Client,
    t: &Task,
    mirror: &crate::mirror::Rule,
) -> Result<Option<u64>, CoreError> {
    if is_valid(&t.path, &t.sha1, t.size) {
        return Ok(None);
    }
    let urls = crate::mirror::attempts(mirror, &t.url);
    let mut last = None;
    for (attempt, url) in urls.iter().enumerate() {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt as u32))).await;
        }
        match try_fetch(http, url, t).await {
            Ok(n) => return Ok(Some(n)),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| CoreError::msg("download failed")))
}

async fn try_fetch(http: &reqwest::Client, url: &str, t: &Task) -> Result<u64, CoreError> {
    let bytes = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !t.sha1.is_empty() && !sha1_hex(&bytes).eq_ignore_ascii_case(&t.sha1) {
        return Err(CoreError::msg(format!("checksum mismatch: {url}")));
    }
    write_atomic(&t.path, &bytes)?;
    Ok(bytes.len() as u64)
}

/// Write to `<file>.part`, then rename, so a crash never leaves a half file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Downloads all tasks, at most [`CONCURRENCY`] at a time. Fails on the first
/// file that still fails after retries. `mirror` decides whether a failing
/// download retries against a mirror (see `crate::mirror`).
pub async fn fetch_all(
    http: &reqwest::Client,
    tasks: Vec<Task>,
    progress: Arc<Progress>,
    mirror: &crate::mirror::Rule,
) -> Result<(), CoreError> {
    progress
        .total_files
        .fetch_add(tasks.len() as u64, Ordering::Relaxed);
    progress
        .total_bytes
        .fetch_add(tasks.iter().map(|t| t.size).sum(), Ordering::Relaxed);
    progress.totals_ready();
    let mut results = stream::iter(tasks)
        .map(|t| {
            let progress = Arc::clone(&progress);
            async move {
                // Skipped files leave the byte totals, so the bar shows only
                // what is really downloaded (not a fake "re-download").
                match fetch_one(http, &t, mirror).await? {
                    Some(n) => progress.file_done(n),
                    None => progress.file_skipped(t.size),
                }
                Ok::<_, CoreError>(())
            }
        })
        .buffer_unordered(CONCURRENCY);
    while let Some(r) = results.next().await {
        r?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_hook_emits_stages_now_and_throttles_counters() {
        let events = Arc::new(crate::events::Events::default());
        let mut rx = events.subscribe();
        let p = Progress::default();
        p.attach(Arc::clone(&events), "k".into());

        // Two fast counter updates collapse into one event: the first goes
        // out (nothing emitted before), the second is throttled.
        p.file_done(1);
        p.file_done(1);
        match rx.blocking_recv().unwrap() {
            crate::events::Event::GameProgress { key, progress } => {
                assert_eq!(key, "k");
                // The first update wins the throttle race, so the one event
                // carries the state right after that single file.
                assert_eq!(progress["done_files"].as_u64(), Some(1));
            }
            ev => panic!("unexpected event: {ev:?}"),
        }
        assert!(rx.try_recv().is_err(), "the fast pair emitted one event");

        // After the throttle window the next counter update goes out.
        std::thread::sleep(std::time::Duration::from_millis(EMIT_EVERY + 60));
        p.file_done(1);
        assert!(matches!(
            rx.blocking_recv().unwrap(),
            crate::events::Event::GameProgress { .. }
        ));

        // Stages and the first totals bypass the throttle.
        p.set_stage("done");
        assert!(matches!(
            rx.blocking_recv().unwrap(),
            crate::events::Event::GameProgress { .. }
        ));
        p.totals_ready();
        assert!(matches!(
            rx.blocking_recv().unwrap(),
            crate::events::Event::GameProgress { .. }
        ));
    }

    #[test]
    fn progress_without_a_hook_stays_silent_and_correct() {
        let p = Progress::default();
        p.set_stage("game");
        p.total_bytes.store(20, Ordering::Relaxed);
        p.file_done(10);
        p.file_skipped(5);
        p.totals_ready();
        assert_eq!(p.snapshot()["stage"], "game");
        assert_eq!(p.snapshot()["done_files"].as_u64(), Some(2));
        assert_eq!(p.snapshot()["done_bytes"].as_u64(), Some(10));
        assert_eq!(p.snapshot()["total_bytes"].as_u64(), Some(15));
    }

    #[test]
    fn sha1_known_value() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn existing_file_is_validated() {
        let dir = std::env::temp_dir().join(format!("fl-dl-{}", uuid::Uuid::new_v4()));
        let p = dir.join("a/b.txt");
        write_atomic(&p, b"abc").unwrap();
        assert!(is_valid(&p, "a9993e364706816aba3e25717850c26c9cd0d89d", 3));
        assert!(!is_valid(&p, "0000000000000000000000000000000000000000", 3));
        assert!(!is_valid(&p, "", 4));
        assert!(is_valid(&p, "", 0));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn valid_files_are_not_downloaded_again() {
        let dir = std::env::temp_dir().join(format!("fl-dl-{}", uuid::Uuid::new_v4()));
        let p = dir.join("x.txt");
        write_atomic(&p, b"abc").unwrap();
        let progress = Arc::new(Progress::default());
        let task = Task {
            // Unreachable on purpose: a skipped file must not touch the network.
            url: "http://127.0.0.1:9/never".into(),
            path: p,
            sha1: "a9993e364706816aba3e25717850c26c9cd0d89d".into(),
            size: 3,
        };
        fetch_all(
            &reqwest::Client::new(),
            vec![task],
            Arc::clone(&progress),
            &crate::mirror::Rule::Never,
        )
            .await
            .unwrap();
        assert_eq!(progress.done_files.load(Ordering::Relaxed), 1);
        assert_eq!(progress.total_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(progress.done_bytes.load(Ordering::Relaxed), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
