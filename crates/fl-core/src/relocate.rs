//! Moving the instances folder to a place the user picks (Settings → Игра → «Папка сборок»).
//!
//! Folders are renamed when the target is on the same drive; otherwise they are copied
//! with progress and the originals removed. Any failure moves back what was already moved.

use std::path::Path;
use std::sync::atomic::Ordering;

use crate::{game::download::Progress, CoreError};

/// Key in `game_progress` while a move runs (polled with `game.progress`).
pub const PROGRESS_KEY: &str = "move:instances";

fn bad(s: &str) -> CoreError {
    CoreError::BadArgs(s.into())
}

/// The target must be a real, writable folder outside the current root and the launcher data.
pub fn check_target(
    old: &Path,
    new: &Path,
    data_dir: &Path,
    dirs: &[String],
) -> Result<(), CoreError> {
    if !new.is_absolute() {
        return Err(bad("Нужен полный путь к папке"));
    }
    if new.starts_with(old) {
        return Err(bad("Эта папка внутри текущей папки сборок"));
    }
    if data_dir.starts_with(new) {
        return Err(bad("В этой папке лежат данные лаунчера — выберите отдельную"));
    }
    if new.exists() && !new.is_dir() {
        return Err(bad("По этому пути находится файл, а не папка"));
    }
    if let Some(d) = dirs.iter().find(|d| new.join(d).exists()) {
        return Err(CoreError::msg(format!("В новой папке уже есть «{d}»")));
    }
    std::fs::create_dir_all(new).map_err(|e| CoreError::msg(format!("Не удалось создать папку: {e}")))?;
    let probe = new.join(".freedom-write-test");
    std::fs::write(&probe, b"ok").map_err(|_| bad("В эту папку нельзя записывать"))?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

fn size_of(p: &Path) -> u64 {
    let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
    if !m.is_dir() {
        return m.len();
    }
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum())
        .unwrap_or(0)
}

fn count_files(p: &Path) -> u64 {
    match std::fs::read_dir(p) {
        Ok(rd) => rd
            .flatten()
            .map(|e| if e.path().is_dir() { count_files(&e.path()) } else { 1 })
            .sum(),
        Err(_) => 1,
    }
}

/// Recursive copy that reports bytes and files to `p`.
pub fn copy_tree(from: &Path, to: &Path, p: &Progress) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        let t = e.file_type()?;
        if t.is_dir() {
            copy_tree(&e.path(), &dest, p)?;
        } else if t.is_file() {
            let n = std::fs::copy(e.path(), &dest)?;
            p.done_bytes.fetch_add(n, Ordering::Relaxed);
            p.done_files.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

/// Moves one folder; a rename when possible, otherwise copy + remove.
fn move_one(from: &Path, to: &Path, p: &Progress) -> std::io::Result<()> {
    let size = size_of(from);
    let files = count_files(from);
    if std::fs::rename(from, to).is_ok() {
        p.done_bytes.fetch_add(size, Ordering::Relaxed);
        p.done_files.fetch_add(files, Ordering::Relaxed);
        return Ok(());
    }
    if let Err(e) = copy_tree(from, to, p) {
        let _ = std::fs::remove_dir_all(to);
        return Err(e);
    }
    // The copy is complete; a leftover original is only wasted space.
    let _ = std::fs::remove_dir_all(from);
    Ok(())
}

/// Moves every instance folder in `dirs` from `old` to `new`. Returns how many moved.
pub fn move_all(dirs: &[String], old: &Path, new: &Path, p: &Progress) -> Result<usize, CoreError> {
    p.set_stage("measure");
    let present: Vec<&String> = dirs.iter().filter(|d| old.join(d).is_dir()).collect();
    let total: u64 = present.iter().map(|d| size_of(&old.join(d))).sum();
    let files: u64 = present.iter().map(|d| count_files(&old.join(d))).sum();
    p.total_bytes.store(total, Ordering::Relaxed);
    p.total_files.store(files, Ordering::Relaxed);
    p.set_stage("move");
    let mut moved: Vec<&String> = Vec::new();
    for d in &present {
        if let Err(e) = move_one(&old.join(d), &new.join(d), p) {
            p.set_stage("rollback");
            let scratch = Progress::default();
            for back in moved.iter().rev() {
                let _ = move_one(&new.join(back), &old.join(back), &scratch);
            }
            p.set_stage("error");
            return Err(CoreError::msg(format!("Не удалось перенести «{d}»: {e}")));
        }
        moved.push(d);
    }
    // Leave no empty shell of the old custom folder behind.
    let _ = std::fs::remove_dir(old);
    p.set_stage("done");
    Ok(moved.len())
}

pub fn to_setting(new: &Path, default: &Path) -> Option<String> {
    (new != default).then(|| new.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-reloc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn moves_and_checks() {
        let base = tmp();
        let data = base.join("data");
        let old = data.join("instances");
        std::fs::create_dir_all(old.join("A/saves")).unwrap();
        std::fs::write(old.join("A/saves/w.dat"), b"12345").unwrap();
        std::fs::create_dir_all(old.join("B")).unwrap();
        let dirs = vec!["A".to_string(), "B".to_string(), "missing".to_string()];
        assert!(check_target(&old, &old.join("x"), &data, &dirs).is_err());
        assert!(check_target(&old, &base, &data, &dirs).is_err());
        assert!(check_target(&old, Path::new("rel"), &data, &dirs).is_err());
        let new = base.join("games");
        check_target(&old, &new, &data, &dirs).unwrap();
        let p = Progress::default();
        assert_eq!(move_all(&dirs, &old, &new, &p).unwrap(), 2);
        assert_eq!(std::fs::read(new.join("A/saves/w.dat")).unwrap(), b"12345");
        assert!(!old.exists());
        assert_eq!(p.done_bytes.load(Ordering::Relaxed), 5);
        // A clash is refused before anything moves.
        std::fs::create_dir_all(old.join("A")).unwrap();
        assert!(check_target(&new, &old, &data, &dirs).is_err());
        let c = tmp();
        copy_tree(&new, &c, &Progress::default()).unwrap();
        assert!(c.join("A/saves/w.dat").is_file());
        let _ = std::fs::remove_dir_all(base);
        let _ = std::fs::remove_dir_all(c);
    }
}