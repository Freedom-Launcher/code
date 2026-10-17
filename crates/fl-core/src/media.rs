//! Screenshots and the game log of an instance (the «Скриншоты» and «Логи» tabs).

use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::{instances::InstanceStore, store, CoreError};

/// Largest log chunk returned by one `instances.log` call.
pub const LOG_CHUNK: u64 = 256 * 1024;

fn is_image(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".png") || n.ends_with(".jpg") || n.ends_with(".jpeg")
}

fn valid_file(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && is_image(name)
}

fn mtime(m: &std::fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

/// Screenshots in `screenshots/`, newest first.
pub fn list_shots(game_dir: &Path) -> Vec<Value> {
    let Ok(rd) = std::fs::read_dir(game_dir.join("screenshots")) else {
        return Vec::new();
    };
    let mut out: Vec<(u64, String, u64)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok().filter(|n| valid_file(n))?;
            let m = e.metadata().ok().filter(|m| m.is_file())?;
            Some((mtime(&m), name, m.len()))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    out.into_iter()
        .map(|(t, file, size)| json!({ "file": file, "taken": t, "size": size }))
        .collect()
}

/// Path of one screenshot, if the name is safe and the file exists.
pub fn shot_path(game_dir: &Path, file: &str) -> Option<PathBuf> {
    if !valid_file(file) {
        return None;
    }
    let p = game_dir.join("screenshots").join(file);
    p.is_file().then_some(p)
}

/// For the `shot:` protocol: `<instance id>/<file>` → bytes and MIME type.
pub fn read_shot(data_dir: &Path, instance: &str, file: &str) -> Option<(Vec<u8>, &'static str)> {
    let s: InstanceStore = store::load(&data_dir.join("instances.json")).ok()?;
    let i = s.instances.into_iter().find(|i| i.id == instance)?;
    let p = shot_path(&i.game_dir(data_dir), file)?;
    let bytes = std::fs::read(p).ok()?;
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else {
        return None;
    };
    Some((bytes, mime))
}

/// A piece of the log. `from` = continue after this offset (live tail); `before` =
/// the chunk that ends here (scroll up); neither = the last chunk. Cuts start on a
/// line boundary. If the file shrank (new launch), it starts over with `reset: true`.
pub fn read_log(path: &Path, from: Option<u64>, before: Option<u64>) -> Result<Value, CoreError> {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(
                json!({ "exists": false, "text": "", "start": 0, "end": 0, "size": 0, "reset": false }),
            );
        }
        Err(e) => return Err(e.into()),
    };
    let size = f.metadata()?.len();
    let mut reset = false;
    let (mut start, end) = match (from, before) {
        (Some(o), _) if o <= size => (o, size.min(o + LOG_CHUNK)),
        (Some(_), _) => {
            reset = true;
            (size.saturating_sub(LOG_CHUNK), size)
        }
        (None, Some(b)) => {
            let b = b.min(size);
            (b.saturating_sub(LOG_CHUNK), b)
        }
        (None, None) => (size.saturating_sub(LOG_CHUNK), size),
    };
    f.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; (end - start) as usize];
    f.read_exact(&mut buf)?;
    let cut_head = start > 0 && from.is_none() || reset && start > 0;
    if cut_head {
        if let Some(n) = buf.iter().position(|&b| b == b'\n') {
            buf.drain(..=n);
            start += n as u64 + 1;
        }
    }
    // A live tail stops at the last full line; the rest comes next time.
    let mut end = end;
    if from.is_some() && !reset {
        match buf.iter().rposition(|&b| b == b'\n') {
            Some(n) => {
                end = start + n as u64 + 1;
                buf.truncate(n + 1);
            }
            None => {
                end = start;
                buf.clear();
            }
        }
    }
    Ok(json!({
        "exists": true,
        "text": String::from_utf8_lossy(&buf),
        "start": start,
        "end": end,
        "size": size,
        "reset": reset,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-media-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn shots_are_filtered_and_safe() {
        let d = tmp("shots");
        let s = d.join("screenshots");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("a.png"), b"\x89PNG").unwrap();
        std::fs::write(s.join("notes.txt"), b"x").unwrap();
        std::fs::write(s.join(".hidden.png"), b"x").unwrap();
        let l = list_shots(&d);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0]["file"], "a.png");
        assert!(shot_path(&d, "a.png").is_some());
        assert!(shot_path(&d, "../a.png").is_none());
        assert!(shot_path(&d, "notes.txt").is_none());
    }

    #[test]
    fn log_tail_and_follow() {
        let d = tmp("log");
        let p = d.join("l.log");
        assert_eq!(read_log(&p, None, None).unwrap()["exists"], false);
        std::fs::write(&p, "one\ntwo\nthr").unwrap();
        let v = read_log(&p, None, None).unwrap();
        assert_eq!(v["text"], "one\ntwo\nthr");
        let v = read_log(&p, Some(0), None).unwrap();
        assert_eq!(v["text"], "one\ntwo\n");
        assert_eq!(v["end"], 8);
        let v = read_log(&p, Some(8), None).unwrap();
        assert_eq!(v["text"], "");
        assert_eq!(v["end"], 8);
        let v = read_log(&p, Some(999), None).unwrap();
        assert_eq!(v["reset"], true);
    }
}
