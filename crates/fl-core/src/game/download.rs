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
    time::Duration,
};

use futures_util::{stream, StreamExt};
use sha1::{Digest, Sha1};

use crate::CoreError;

pub const CONCURRENCY: usize = 8;
const ATTEMPTS: u32 = 3;

#[derive(Debug, Clone)]
pub struct Task {
    pub url: String,
    pub path: PathBuf,
    /// Lowercase hex; empty when unknown.
    pub sha1: String,
    pub size: u64,
}

/// Shared counters the UI can poll while an install runs.
#[derive(Debug, Default)]
pub struct Progress {
    pub total_files: AtomicU64,
    pub done_files: AtomicU64,
    pub total_bytes: AtomicU64,
    pub done_bytes: AtomicU64,
    stage: std::sync::Mutex<&'static str>,
}

impl Progress {
    pub fn set_stage(&self, stage: &'static str) {
        *self.stage.lock().expect("stage lock") = stage;
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

async fn fetch_one(http: &reqwest::Client, t: &Task) -> Result<u64, CoreError> {
    if is_valid(&t.path, &t.sha1, t.size) {
        return Ok(t.size);
    }
    let mut last = None;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
        }
        match try_fetch(http, t).await {
            Ok(n) => return Ok(n),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| CoreError::msg("download failed")))
}

async fn try_fetch(http: &reqwest::Client, t: &Task) -> Result<u64, CoreError> {
    let bytes = http
        .get(&t.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !t.sha1.is_empty() && !sha1_hex(&bytes).eq_ignore_ascii_case(&t.sha1) {
        return Err(CoreError::msg(format!("checksum mismatch: {}", t.url)));
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
/// file that still fails after retries.
pub async fn fetch_all(
    http: &reqwest::Client,
    tasks: Vec<Task>,
    progress: Arc<Progress>,
) -> Result<(), CoreError> {
    progress
        .total_files
        .fetch_add(tasks.len() as u64, Ordering::Relaxed);
    progress
        .total_bytes
        .fetch_add(tasks.iter().map(|t| t.size).sum(), Ordering::Relaxed);
    let mut results = stream::iter(tasks)
        .map(|t| {
            let progress = Arc::clone(&progress);
            async move {
                let n = fetch_one(http, &t).await?;
                progress.done_files.fetch_add(1, Ordering::Relaxed);
                progress.done_bytes.fetch_add(n, Ordering::Relaxed);
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
}
