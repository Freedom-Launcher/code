//! «Собрать диагностику»: the launcher's and the games' logs in one zip for a bug report.

use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use zip::ZipWriter;

use crate::CoreError;

/// Only the end of a log matters; a runaway `latest.log` must not make a 1 GB archive.
const TAIL: u64 = 2 * 1024 * 1024;
/// Games with the most recent logs that go into the archive.
const GAMES: usize = 6;
/// Archives kept in `<data>/diagnostics/`, the new one included.
const KEEP: usize = 5;

/// An instance's game folder and its display name.
pub struct Game {
    pub name: String,
    pub dir: PathBuf,
}

fn z(e: zip::result::ZipError) -> CoreError {
    CoreError::msg(e.to_string())
}

fn put(w: &mut ZipWriter<fs::File>, name: &str, data: &[u8]) -> Result<(), CoreError> {
    w.start_file(name, zip::write::SimpleFileOptions::default())
        .map_err(z)?;
    w.write_all(data)?;
    Ok(())
}

fn tail(p: &Path) -> Option<Vec<u8>> {
    let mut f = fs::File::open(p).ok()?;
    let len = f.metadata().ok()?.len();
    if len > TAIL {
        f.seek(SeekFrom::Start(len - TAIL)).ok()?;
    }
    let mut v = Vec::new();
    f.read_to_end(&mut v).ok()?;
    Some(v)
}

fn mtime(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).ok()?.modified().ok()
}

/// The newest `n` files in `dir` whose names pass `keep`.
fn newest(dir: &Path, keep: impl Fn(&str) -> bool, n: usize) -> Vec<PathBuf> {
    let mut v: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| keep(&e.file_name().to_string_lossy()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().take(n).map(|x| x.1).collect()
}

fn safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || "-_ ".contains(c) { c } else { '_' })
        .collect();
    s.trim().to_owned()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Writes `<data>/diagnostics/fl-diagnostics-<now>.zip` and returns its path.
pub fn collect(data_dir: &Path, info: &str, games: &[Game], now: u64) -> Result<PathBuf, CoreError> {
    let out_dir = data_dir.join("diagnostics");
    fs::create_dir_all(&out_dir)?;
    for old in newest(&out_dir, |n| n.ends_with(".zip"), usize::MAX)
        .into_iter()
        .skip(KEEP - 1)
    {
        let _ = fs::remove_file(old);
    }
    let out = out_dir.join(format!("fl-diagnostics-{now}.zip"));
    let mut w = ZipWriter::new(fs::File::create(&out)?);
    put(&mut w, "info.txt", info.as_bytes())?;

    for p in newest(&data_dir.join("logs"), |n| n.ends_with(".log"), 12) {
        if let Some(d) = tail(&p) {
            put(&mut w, &format!("launcher/{}", file_name(&p)), &d)?;
        }
    }

    let mut recent: Vec<(SystemTime, &Game)> = games
        .iter()
        .filter_map(|g| {
            let logs = g.dir.join("logs");
            let t = mtime(&logs.join("latest.log")).max(mtime(&logs.join("launcher-output.log")))?;
            Some((t, g))
        })
        .collect();
    recent.sort_by(|a, b| b.0.cmp(&a.0));
    for (n, (_, g)) in recent.into_iter().take(GAMES).enumerate() {
        let base = format!("games/{}-{}", n + 1, safe(&g.name));
        let mut files = vec![
            g.dir.join("logs").join("latest.log"),
            g.dir.join("logs").join("launcher-output.log"),
        ];
        files.extend(newest(&g.dir.join("crash-reports"), |n| n.ends_with(".txt"), 3));
        files.extend(newest(&g.dir, |n| n.starts_with("hs_err_pid") && n.ends_with(".log"), 3));
        for p in files {
            if let Some(d) = tail(&p) {
                put(&mut w, &format!("{base}/{}", file_name(&p)), &d)?;
            }
        }
    }
    w.finish().map_err(z)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_launcher_and_game_logs() {
        let root = std::env::temp_dir().join(format!("fl-diag-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let game = root.join("inst");
        fs::create_dir_all(root.join("logs")).unwrap();
        fs::create_dir_all(game.join("logs")).unwrap();
        fs::create_dir_all(game.join("crash-reports")).unwrap();
        fs::write(root.join("logs").join("session.log"), "start 1\n").unwrap();
        fs::write(game.join("logs").join("latest.log"), "game log").unwrap();
        fs::write(game.join("crash-reports").join("crash-1.txt"), "boom").unwrap();
        fs::write(game.join("hs_err_pid42.log"), "jvm").unwrap();
        let games = [Game { name: "My/Pack".into(), dir: game }];
        let out = collect(&root, "{}", &games, 7).unwrap();
        let mut a = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
        let mut names: Vec<String> = (0..a.len()).map(|i| a.by_index(i).unwrap().name().to_owned()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "games/1-My_Pack/crash-1.txt",
                "games/1-My_Pack/hs_err_pid42.log",
                "games/1-My_Pack/latest.log",
                "info.txt",
                "launcher/session.log",
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }
}