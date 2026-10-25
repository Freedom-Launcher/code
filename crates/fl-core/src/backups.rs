//! World backups: zip copies of `saves/<world>` kept in `<game dir>/backups/`.
//!
//! Copies are named the way Minecraft's own "Make Backup" names them
//! (`YYYY-MM-DD_HH-MM-SS_<world>.zip`, with the world folder inside the archive),
//! so copies made from the game show up here, and ours open in the game's tooling.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::CoreError;

/// Where an instance keeps its world copies.
pub fn dir(game_dir: &Path) -> PathBuf {
    game_dir.join("backups")
}

/// Length of the `2026-10-25_21-14-03_` prefix.
const STAMP: usize = 20;

fn is_stamp(s: &[u8]) -> bool {
    s.len() == STAMP
        && s.iter().enumerate().all(|(i, c)| match i {
            4 | 7 | 13 | 16 => *c == b'-',
            10 | 19 => *c == b'_',
            _ => c.is_ascii_digit(),
        })
}

/// Drops a trailing ` (2)` / `(2)` that tells same-second copies apart.
fn strip_copy_no(s: &str) -> &str {
    let Some(inner) = s.strip_suffix(')') else { return s };
    let Some(open) = inner.rfind('(') else { return s };
    let n = &inner[open + 1..];
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return s;
    }
    inner[..open].trim_end()
}

/// Whether `file` (a name in the backups folder) is a copy of `world`.
pub fn matches(file: &str, world: &str) -> bool {
    let b = file.as_bytes();
    if b.len() <= STAMP + 4 || !is_stamp(&b[..STAMP]) {
        return false;
    }
    let Some(name) = file[STAMP..].strip_suffix(".zip") else { return false };
    name == world || strip_copy_no(name) == world
}

/// File names in the backups folder.
pub fn files(game_dir: &Path) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir(game_dir)) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

fn entry(path: &Path, file: &str) -> Value {
    let m = fs::metadata(path).ok();
    let at = m
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    json!({ "id": file, "at": at, "size": m.map_or(0, |m| m.len()) })
}

/// Copies of `world`, newest first.
pub fn list(game_dir: &Path, world: &str) -> Vec<Value> {
    let d = dir(game_dir);
    let mut v: Vec<Value> = files(game_dir)
        .into_iter()
        .filter(|f| matches(f, world))
        .map(|f| entry(&d.join(&f), &f))
        .collect();
    v.sort_by_key(|e| std::cmp::Reverse(order(e["id"].as_str().unwrap_or_default())));
    v
}

/// Sort key of a copy: the time in its name, then its same-second number.
fn order(file: &str) -> (String, u32) {
    let name = file.strip_suffix(".zip").unwrap_or(file);
    let no = name
        .strip_suffix(')')
        .and_then(|s| s.rfind('(').map(|i| &s[i + 1..]))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    (file.get(..STAMP).unwrap_or_default().to_owned(), no)
}

/// `YYYY-MM-DD_HH-MM-SS_` for a unix time (UTC).
fn stamp(t: u64) -> String {
    let (days, s) = ((t / 86_400) as i64, t % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}_{:02}-{:02}-{:02}_", s / 3600, s % 3600 / 60, s % 60)
}

fn zerr(e: zip::result::ZipError) -> CoreError {
    CoreError::msg(format!("Ошибка архива: {e}"))
}

fn broken() -> CoreError {
    CoreError::msg("Копия повреждена")
}

/// Zips `saves/<world>` into a new copy and returns its entry.
pub fn create(game_dir: &Path, world: &str, now: u64) -> Result<Value, CoreError> {
    let src = game_dir.join("saves").join(world);
    if !src.join("level.dat").is_file() {
        return Err(CoreError::BadArgs(format!("no such world: {world}")));
    }
    let out = dir(game_dir);
    fs::create_dir_all(&out)?;
    let base = format!("{}{world}", stamp(now));
    let mut name = format!("{base}.zip");
    let mut n = 1;
    while out.join(&name).exists() {
        name = format!("{base} ({n}).zip");
        n += 1;
    }
    let part = out.join(format!(".{name}.part"));
    if let Err(e) = write_zip(&src, world, &part) {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    let dest = out.join(&name);
    fs::rename(&part, &dest)?;
    Ok(entry(&dest, &name))
}

fn write_zip(src: &Path, world: &str, to: &Path) -> Result<(), CoreError> {
    let mut z = zip::ZipWriter::new(io::BufWriter::new(fs::File::create(to)?));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let mut items: Vec<_> = fs::read_dir(src.join(&rel))?.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        for e in items {
            let name = e.file_name();
            let n = name
                .to_str()
                .ok_or_else(|| CoreError::msg("В мире есть файл с недопустимым именем"))?;
            let r = rel.join(n);
            let zname = format!("{world}/{}", r.to_string_lossy().replace('\\', "/"));
            let t = e.file_type()?;
            if t.is_dir() {
                z.add_directory(format!("{zname}/"), opts).map_err(zerr)?;
                stack.push(r);
            } else if t.is_file() {
                // The game's lock file is not part of the world (and is locked while it runs).
                if rel.as_os_str().is_empty() && n == "session.lock" {
                    continue;
                }
                let len = e.metadata()?.len();
                z.start_file(zname, opts.large_file(len >= u64::from(u32::MAX))).map_err(zerr)?;
                io::copy(&mut fs::File::open(e.path())?, &mut z)?;
            }
        }
    }
    z.finish().map_err(zerr)?.flush()?;
    Ok(())
}

/// Unpacks the world inside a copy (the folder with the shallowest `level.dat`) into `to`.
fn extract(zip_path: &Path, to: &Path) -> Result<(), CoreError> {
    let f = io::BufReader::new(fs::File::open(zip_path)?);
    let mut z = zip::ZipArchive::new(f).map_err(|_| broken())?;
    let mut prefix: Option<PathBuf> = None;
    for i in 0..z.len() {
        let e = z.by_index_raw(i).map_err(|_| broken())?;
        let Some(p) = e.enclosed_name() else { return Err(broken()) };
        if p.file_name() == Some("level.dat".as_ref()) {
            let parent = p.parent().map(Path::to_path_buf).unwrap_or_default();
            if prefix.as_ref().is_none_or(|q| parent.components().count() < q.components().count()) {
                prefix = Some(parent);
            }
        }
    }
    let prefix = prefix.ok_or_else(|| CoreError::msg("В копии нет мира"))?;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|_| broken())?;
        let Some(p) = e.enclosed_name() else { return Err(broken()) };
        let Ok(rel) = p.strip_prefix(&prefix) else { continue };
        if rel.as_os_str().is_empty() || e.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            continue;
        }
        let out = to.join(rel);
        if e.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(d) = out.parent() {
            fs::create_dir_all(d)?;
        }
        io::copy(&mut e, &mut fs::File::create(&out)?).map_err(|_| broken())?;
    }
    Ok(())
}

/// The path of copy `file` of `world`, if it really is one.
pub fn path(game_dir: &Path, world: &str, file: &str) -> Result<PathBuf, CoreError> {
    let ok = !file.starts_with('.') && !file.contains(['/', '\\', ':', '\0']) && matches(file, world);
    let p = dir(game_dir).join(file);
    if !ok || !p.is_file() {
        return Err(CoreError::msg("Такой копии нет"));
    }
    Ok(p)
}

/// Replaces `saves/<world>` with copy `file`. The current world is saved as a new copy first,
/// so a restore can always be undone. `busy` is asked once more right before the swap.
pub fn restore(
    game_dir: &Path,
    world: &str,
    file: &str,
    now: u64,
    busy: impl Fn() -> bool,
) -> Result<Value, CoreError> {
    let src = path(game_dir, world, file)?;
    if busy() {
        return Err(CoreError::msg("Сначала закройте игру"));
    }
    let saves = game_dir.join("saves");
    let cur = saves.join(world);
    let tmp = saves.join(format!(".restoring-{world}"));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let cleanup = |e: CoreError| {
        let _ = fs::remove_dir_all(&tmp);
        e
    };
    extract(&src, &tmp).map_err(cleanup)?;
    let safety = if cur.join("level.dat").is_file() {
        Some(create(game_dir, world, now).map_err(cleanup)?)
    } else {
        None
    };
    if busy() {
        if let Some(id) = safety.as_ref().and_then(|s| s["id"].as_str()) {
            let _ = fs::remove_file(dir(game_dir).join(id));
        }
        return Err(cleanup(CoreError::msg("Сначала закройте игру")));
    }
    if cur.exists() {
        let old = saves.join(format!(".replaced-{world}"));
        let _ = fs::remove_dir_all(&old);
        fs::rename(&cur, &old).map_err(|_| {
            cleanup(CoreError::msg("Не удалось заменить мир: папка открыта в другой программе"))
        })?;
        if let Err(e) = fs::rename(&tmp, &cur) {
            let _ = fs::rename(&old, &cur);
            return Err(cleanup(e.into()));
        }
        let _ = fs::remove_dir_all(&old);
    } else {
        fs::rename(&tmp, &cur).map_err(|e| cleanup(e.into()))?;
    }
    Ok(json!({ "world": world, "restored": file, "safety": safety }))
}

/// Deletes copy `file` of `world`.
pub fn delete(game_dir: &Path, world: &str, file: &str) -> Result<(), CoreError> {
    fs::remove_file(path(game_dir, world, file)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-bk-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn names_follow_the_game() {
        assert_eq!(stamp(0), "1970-01-01_00-00-00_");
        assert_eq!(stamp(1_700_000_000), "2023-11-14_22-13-20_");
        assert_eq!(stamp(951_782_400), "2000-02-29_00-00-00_");
        assert!(matches("2026-10-25_21-14-03_New World.zip", "New World"));
        assert!(matches("2026-10-25_21-14-03_New World (2).zip", "New World"));
        assert!(matches("2026-10-25_21-14-03_Мир(3).zip", "Мир"));
        assert!(matches("2026-10-25_21-14-03_Мир (2).zip", "Мир (2)"));
        assert!(!matches("2026-10-25_21-14-03_New World.zip", "New"));
        assert!(!matches("notes.zip", "notes"));
        assert!(!matches(".2026-10-25_21-14-03_A.zip.part", "A"));
        assert!(!matches("2026-10-25_21-14-03_A.zip.part", "A"));
    }

    #[test]
    fn backup_restore_roundtrip() {
        let g = tmp();
        let w = g.join("saves/Мир");
        fs::create_dir_all(w.join("region")).unwrap();
        fs::write(w.join("level.dat"), b"v1").unwrap();
        fs::write(w.join("session.lock"), b"lock").unwrap();
        fs::write(w.join("region/r.0.0.mca"), vec![7u8; 100_000]).unwrap();

        let a = create(&g, "Мир", 1_700_000_000).unwrap();
        let b = create(&g, "Мир", 1_700_000_000).unwrap();
        assert_eq!(a["id"], "2023-11-14_22-13-20_Мир.zip");
        assert_eq!(b["id"], "2023-11-14_22-13-20_Мир (1).zip");
        assert_eq!(list(&g, "Мир").len(), 2);
        assert!(list(&g, "Other").is_empty());
        delete(&g, "Мир", b["id"].as_str().unwrap()).unwrap();
        assert!(delete(&g, "Мир", "../saves/Мир/level.dat").is_err());
        assert!(delete(&g, "Other", a["id"].as_str().unwrap()).is_err());

        // Change the world, then go back.
        fs::write(w.join("level.dat"), b"v2").unwrap();
        fs::write(w.join("new.dat"), b"x").unwrap();
        assert!(restore(&g, "Мир", a["id"].as_str().unwrap(), 1_700_000_100, || true).is_err());
        let calls = std::cell::Cell::new(0);
        let late = || { calls.set(calls.get() + 1); calls.get() > 1 };
        assert!(restore(&g, "Мир", a["id"].as_str().unwrap(), 1_700_000_100, late).is_err());
        assert_eq!(list(&g, "Мир").len(), 1, "no stray safety copy after a refused restore");
        assert_eq!(fs::read(w.join("level.dat")).unwrap(), b"v2");
        let r = restore(&g, "Мир", a["id"].as_str().unwrap(), 1_700_000_100, || false).unwrap();
        assert_eq!(fs::read(w.join("level.dat")).unwrap(), b"v1");
        assert_eq!(fs::read(w.join("region/r.0.0.mca")).unwrap().len(), 100_000);
        assert!(!w.join("new.dat").exists() && !w.join("session.lock").exists());
        // The world as it was before the restore is kept as a copy.
        let safety = r["safety"]["id"].as_str().unwrap().to_owned();
        assert_eq!(list(&g, "Мир")[0]["id"], safety);
        restore(&g, "Мир", &safety, 1_700_000_200, || false).unwrap();
        assert_eq!(fs::read(w.join("level.dat")).unwrap(), b"v2");
        assert!(w.join("new.dat").is_file());
        let left: Vec<String> = fs::read_dir(g.join("saves")).unwrap().flatten().map(|e| e.file_name().into_string().unwrap()).collect();
        assert_eq!(left, vec!["Мир".to_string()]);
        fs::remove_dir_all(&g).unwrap();
    }

    #[test]
    fn restores_copies_without_a_world_folder() {
        let g = tmp();
        fs::create_dir_all(g.join("saves/A")).unwrap();
        fs::write(g.join("saves/A/level.dat"), b"now").unwrap();
        fs::create_dir_all(dir(&g)).unwrap();
        let z = dir(&g).join("2020-01-01_00-00-00_A.zip");
        let mut w = zip::ZipWriter::new(fs::File::create(&z).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("level.dat", o).unwrap();
        w.write_all(b"old").unwrap();
        w.finish().unwrap();
        restore(&g, "A", "2020-01-01_00-00-00_A.zip", 1_700_000_000, || false).unwrap();
        assert_eq!(fs::read(g.join("saves/A/level.dat")).unwrap(), b"old");
        fs::remove_dir_all(&g).unwrap();
    }
}