//! World backups of a server («Резервные копии»): zip files of the world
//! folders in `<data>/server-backups/<server id>/`, outside the server folder,
//! so a broken server folder does not take them with it.
//!
//! A running server is backed up too: the launcher turns its autosave off
//! (`save-off`), has it write everything (`save-all flush`), copies, and turns
//! autosave back on. Restoring needs the server stopped and first backs up
//! the current world, so a restore can always be undone.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{json, Value};

use super::{bad, find, prop, read_props, Server};
use serde::Deserialize;

use super::{args, IdArg};
use crate::{actions::ActionRegistry, unix_now, Core, CoreError, Permission as P};

pub const DIR: &str = "server-backups";
const SAVE_WAIT: Duration = Duration::from_secs(60);

pub fn dir_of(c: &Core, id: &str) -> PathBuf {
    c.path(DIR).join(id)
}

/// `world-20261021-171844.zip`, `world-20261021-171844-before-restore.zip`.
pub fn valid_name(n: &str) -> bool {
    n.len() <= 96
        && n.ends_with(".zip")
        && !n.starts_with(['.', '-'])
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !n.contains("..")
}

/// World folders of the server: the level, plus the Nether and End folders
/// that Paper-like servers keep next to it.
pub fn worlds(root: &Path) -> Vec<String> {
    let level = prop(&read_props(root), "level-name").unwrap_or("world").trim().to_owned();
    if level.is_empty() || level.contains(['/', '\\']) || level.contains("..") {
        return Vec::new();
    }
    [level.clone(), format!("{level}_nether"), format!("{level}_the_end")]
        .into_iter()
        .filter(|w| root.join(w).is_dir() && !fs::symlink_metadata(root.join(w)).is_ok_and(|m| m.file_type().is_symlink()))
        .collect()
}

fn stamp(secs: u64) -> String {
    // mc_date: `2026-10-21 10:19:32 +0000`
    let d = super::mc_date(secs);
    d[..19].replace(['-', ':'], "").replace(' ', "-")
}

fn add_dir(z: &mut zip::ZipWriter<fs::File>, base: &Path, rel: &str, opts: zip::write::SimpleFileOptions) -> io::Result<u64> {
    let mut n = 0;
    let mut entries: Vec<_> = fs::read_dir(base.join(rel))?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(t) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        let r = format!("{rel}/{name}");
        if t.is_symlink() {
            continue;
        } else if t.is_dir() {
            z.add_directory(format!("{r}/"), opts)?;
            n += add_dir(z, base, &r, opts)?;
        } else if t.is_file() {
            if name == "session.lock" {
                continue; // held open by a running server on Windows
            }
            let mut f = match fs::File::open(e.path()) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let big = f.metadata().map(|m| m.len() >= u32::MAX as u64).unwrap_or(false);
            z.start_file(r, opts.large_file(big))?;
            n += io::copy(&mut f, z)?;
        }
    }
    Ok(n)
}

/// Zips the worlds of `root` into `dest` (written to a temp name first).
pub fn zip_worlds(root: &Path, worlds: &[String], dest: &Path) -> Result<u64, CoreError> {
    let tmp = dest.with_extension("zip.part");
    let res = (|| -> io::Result<u64> {
        let f = fs::File::create(&tmp)?;
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(5));
        let mut n = 0;
        for w in worlds {
            z.add_directory(format!("{w}/"), opts)?;
            n += add_dir(&mut z, root, w, opts)?;
        }
        z.finish()?.flush()?;
        Ok(n)
    })();
    match res {
        Ok(n) => {
            fs::rename(&tmp, dest)?;
            Ok(n)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(CoreError::msg(format!("не удалось сделать копию: {e}")))
        }
    }
}

pub fn list(c: &Core, id: &str) -> Vec<Value> {
    let mut out: Vec<Value> = fs::read_dir(dir_of(c, id))
        .map(|r| {
            r.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let m = e.metadata().ok()?;
                    (valid_name(&name) && m.is_file()).then(|| {
                        let created = m
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map_or(0, |d| d.as_secs());
                        json!({ "name": name, "size": m.len(), "created": created, "auto": name.contains("-before-") })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b["created"].as_u64().cmp(&a["created"].as_u64()).then_with(|| b["name"].as_str().cmp(&a["name"].as_str())));
    out
}

fn unique(dir: &Path, base: &str) -> PathBuf {
    let mut p = dir.join(format!("{base}.zip"));
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{base}-{i}.zip"));
        i += 1;
    }
    p
}

async fn make(c: &Arc<Core>, s: &Server, suffix: &str) -> Result<Value, CoreError> {
    let root = s.root()?;
    let ws = worlds(&root);
    if ws.is_empty() {
        return Err(bad("мира ещё нет: он появится после первого запуска сервера"));
    }
    let dir = dir_of(c, &s.id);
    fs::create_dir_all(&dir)?;
    let level = &ws[0];
    let safe_level: String = level.chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' }).take(40).collect();
    let dest = unique(&dir, &format!("{safe_level}-{}{suffix}", stamp(unix_now())));
    let running = c.server_procs.get(&s.id).filter(|p| matches!(p.state(), "running" | "starting"));
    if let Some(p) = &running {
        let before = p.saves();
        p.note("Резервная копия: сохраняем мир…");
        p.send("save-off").await?;
        p.send("save-all flush").await?;
        let end = tokio::time::Instant::now() + SAVE_WAIT;
        while p.saves() == before && p.alive() && tokio::time::Instant::now() < end {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let (root2, ws2, dest2) = (root.clone(), ws.clone(), dest.clone());
    let res = tokio::task::spawn_blocking(move || zip_worlds(&root2, &ws2, &dest2))
        .await
        .map_err(|e| CoreError::msg(e.to_string()))?;
    if let Some(p) = &running {
        if p.alive() {
            let _ = p.send("save-on").await;
            p.note(if res.is_ok() { "Резервная копия готова" } else { "Резервная копия не получилась" });
        }
    }
    res?;
    let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    Ok(json!({
        "name": dest.file_name().map(|n| n.to_string_lossy().into_owned()),
        "size": size,
        "created": unix_now(),
        "worlds": ws,
    }))
}

pub async fn create(c: &Arc<Core>, id: &str) -> Result<Value, CoreError> {
    let s = find(c, id)?;
    make(c, &s, "").await
}

fn extract(zip_path: &Path, root: &Path, allowed: &[String]) -> Result<(), CoreError> {
    let f = fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| CoreError::msg(format!("копия повреждена: {e}")))?;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| CoreError::msg(format!("копия повреждена: {e}")))?;
        let Some(rel) = e.enclosed_name() else {
            return Err(CoreError::msg("в копии странные пути"));
        };
        let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
        if !allowed.contains(&top) {
            return Err(CoreError::msg("в копии есть файлы вне папок мира"));
        }
        let out = root.join(&rel);
        if e.is_dir() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            let mut w = fs::File::create(&out)?;
            io::copy(&mut e, &mut w)?;
        }
    }
    Ok(())
}

/// Folders a backup holds at its top level.
fn tops(zip_path: &Path) -> Result<Vec<String>, CoreError> {
    let f = fs::File::open(zip_path)?;
    let z = zip::ZipArchive::new(f).map_err(|e| CoreError::msg(format!("копия повреждена: {e}")))?;
    let mut out: Vec<String> = Vec::new();
    for n in z.file_names() {
        let top = n.split('/').next().unwrap_or("").to_owned();
        if !top.is_empty() && !out.contains(&top) {
            out.push(top);
        }
    }
    Ok(out)
}

pub async fn restore(c: &Arc<Core>, id: &str, name: &str) -> Result<Value, CoreError> {
    if !valid_name(name) {
        return Err(bad("нет такой копии"));
    }
    if c.server_procs.alive(id) {
        return Err(bad("остановите сервер перед восстановлением мира"));
    }
    let s = find(c, id)?;
    let root = s.root()?;
    let src = dir_of(c, id).join(name);
    if !src.is_file() {
        return Err(bad("нет такой копии"));
    }
    let level = prop(&read_props(&root), "level-name").unwrap_or("world").trim().to_owned();
    let allowed = [level.clone(), format!("{level}_nether"), format!("{level}_the_end")];
    let top = tops(&src)?;
    if top.is_empty() || !top.iter().all(|t| allowed.contains(t)) {
        return Err(bad(format!("эта копия от мира с другим названием (сейчас level-name = {level})")));
    }
    let safety = if worlds(&root).is_empty() { None } else { make(c, &s, "-before-restore").await?["name"].as_str().map(str::to_owned) };
    let (root2, src2) = (root.clone(), src.clone());
    tokio::task::spawn_blocking(move || -> Result<(), CoreError> {
        for w in &allowed {
            let p = root2.join(w);
            if p.is_dir() && !fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                fs::remove_dir_all(&p)?;
            }
        }
        extract(&src2, &root2, &allowed)
    })
    .await
    .map_err(|e| CoreError::msg(e.to_string()))??;
    Ok(json!({ "ok": true, "safety": safety }))
}

pub fn delete(c: &Core, id: &str, name: &str) -> Result<Value, CoreError> {
    if !valid_name(name) {
        return Err(bad("нет такой копии"));
    }
    let p = dir_of(c, id).join(name);
    if !p.is_file() {
        return Err(bad("нет такой копии"));
    }
    fs::remove_file(p)?;
    Ok(json!({ "ok": true }))
}

pub fn remove_all(c: &Core, id: &str) {
    if !id.is_empty() && id.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        let _ = fs::remove_dir_all(dir_of(c, id));
    }
}

/// Holds the per-server busy flag for the length of a backup or restore.
struct Busy<'a>(&'a Core, String);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.server_procs.end_busy(&self.1);
    }
}
fn busy<'a>(c: &'a Core, id: &str) -> Result<Busy<'a>, CoreError> {
    if !c.server_procs.begin_busy(id) {
        return Err(bad("с миром этого сервера уже идёт работа, подождите"));
    }
    Ok(Busy(c, id.to_owned()))
}

#[derive(Deserialize)]
struct NameArg {
    id: String,
    name: String,
}

pub(super) fn register(r: &mut ActionRegistry) {
    r.register("servers.backups", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        let s = find(&c, &id)?;
        let total: u64 = list(&c, &id).iter().filter_map(|b| b["size"].as_u64()).sum();
        Ok(json!({
            "backups": list(&c, &id),
            "total": total,
            "worlds": s.root().map(|r| worlds(&r)).unwrap_or_default(),
            "busy": c.server_procs.is_busy(&id),
            "dir": dir_of(&c, &id).to_string_lossy(),
        }))
    });
    r.register("servers.backup.create", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        let _b = busy(&c, &id)?;
        create(&c, &id).await
    });
    r.register("servers.backup.restore", P::Servers, false, |c, a| async move {
        let NameArg { id, name } = args(a)?;
        let _b = busy(&c, &id)?;
        restore(&c, &id, &name).await
    });
    r.register("servers.backup.delete", P::Servers, false, |c, a| async move {
        let NameArg { id, name } = args(a)?;
        delete(&c, &id, &name)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_stamps() {
        assert!(valid_name("world-20261021-171844.zip") && valid_name("world-20261021-171844-before-restore-2.zip"));
        assert!(!valid_name("../a.zip") && !valid_name("a/b.zip") && !valid_name(".zip") && !valid_name("a.txt"));
        assert_eq!(stamp(1_792_577_972), "20261021-101932");
    }

    #[test]
    fn zip_and_extract_roundtrip() {
        let d = std::env::temp_dir().join(format!("fl-bk-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(d.join("world/region")).unwrap();
        fs::create_dir_all(d.join("world_nether/DIM-1")).unwrap();
        fs::write(d.join("world/level.dat"), b"level").unwrap();
        fs::write(d.join("world/session.lock"), b"x").unwrap();
        fs::write(d.join("world/region/r.0.0.mca"), vec![7u8; 100_000]).unwrap();
        fs::write(d.join("world_nether/DIM-1/x"), b"n").unwrap();
        fs::write(d.join("server.properties"), "level-name=world\n").unwrap();
        let ws = worlds(&d);
        assert_eq!(ws, vec!["world".to_string(), "world_nether".into()]);
        let z = d.join("b.zip");
        assert!(zip_worlds(&d, &ws, &z).unwrap() >= 100_006);
        assert_eq!(tops(&z).unwrap(), ws);
        fs::remove_dir_all(d.join("world")).unwrap();
        extract(&z, &d, &["world".into(), "world_nether".into()]).unwrap();
        assert_eq!(fs::read(d.join("world/level.dat")).unwrap(), b"level");
        assert_eq!(fs::read(d.join("world/region/r.0.0.mca")).unwrap().len(), 100_000);
        assert!(!d.join("world/session.lock").exists());
        assert!(extract(&z, &d, &["world".into()]).is_err(), "nether is not allowed");
        fs::remove_dir_all(d).unwrap();
    }
}