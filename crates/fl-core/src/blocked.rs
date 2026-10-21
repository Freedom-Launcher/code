//! Pack files whose authors allow downloads only from the CurseForge site.
//! A launcher may not fetch them, so the user downloads each one in the browser
//! and the launcher picks it up from the Downloads folder (checked by SHA-1)
//! and puts it into the instance.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{game::download, mods, mrpack, store, CoreError};

/// The instance's list of files still to be downloaded by hand.
pub const FILE: &str = "freedom-blocked.json";
/// Entries looked at per folder, so a huge Downloads folder stays fast.
const MAX_SCAN: usize = 5000;
/// Larger files are never hashed (no pack file is this big).
const MAX_FILE: u64 = 1 << 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Blocked {
    pub title: String,
    pub file_name: String,
    /// Where the file goes in the instance, e.g. `mods/x.jar`.
    pub rel: String,
    /// Lowercase hex; empty when CurseForge gave none.
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    /// The file's download page on curseforge.com.
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<mods::Source>,
}

pub fn load(game_dir: &Path) -> Result<Vec<Blocked>, CoreError> {
    store::load(&game_dir.join(FILE))
}

pub fn save(game_dir: &Path, list: &[Blocked]) -> Result<(), CoreError> {
    let p = game_dir.join(FILE);
    if list.is_empty() {
        match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => return Ok(()),
        }
    }
    store::save(&p, &list, false)
}

fn present(game_dir: &Path, b: &Blocked) -> bool {
    mrpack::safe_rel(&b.rel).is_some_and(|r| {
        let at = game_dir.join(r);
        at.exists() || PathBuf::from(format!("{}.disabled", at.display())).exists()
    })
}

/// Files of the list that are not in the instance yet.
pub fn pending(game_dir: &Path) -> Result<Vec<Blocked>, CoreError> {
    Ok(load(game_dir)?.into_iter().filter(|b| !present(game_dir, b)).collect())
}

/// The user's Downloads folder(s).
pub fn downloads_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for var in ["USERPROFILE", "HOME"] {
        if let Some(h) = std::env::var_os(var).filter(|h| !h.is_empty()) {
            let d = PathBuf::from(h).join("Downloads");
            if d.is_dir() && !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out
}

/// Looks for the pending files in `extra` folders and Downloads and copies the
/// ones found into the instance. A file matches by SHA-1 (so a browser-renamed
/// `x (1).jar` still counts); without a known hash only the exact name does.
/// Returns the titles added and what is still missing.
pub fn resolve(game_dir: &Path, extra: &[PathBuf]) -> Result<(Vec<String>, Vec<Blocked>), CoreError> {
    let mut want = pending(game_dir)?;
    let mut found = Vec::new();
    if want.is_empty() {
        return Ok((found, want));
    }
    let mut dirs: Vec<PathBuf> = extra.iter().filter(|d| d.is_dir()).cloned().collect();
    dirs.extend(downloads_dirs());
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten().take(MAX_SCAN) {
            if want.is_empty() {
                break;
            }
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() || meta.len() > MAX_FILE {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let len = meta.len();
            let cand = |b: &Blocked| name == b.file_name || (!b.sha1.is_empty() && b.size > 0 && b.size == len);
            if !want.iter().any(cand) {
                continue;
            }
            let Ok(bytes) = std::fs::read(e.path()) else { continue };
            let hash = download::sha1_hex(&bytes);
            let Some(pos) = want.iter().position(|b| {
                if b.sha1.is_empty() {
                    name == b.file_name
                } else {
                    hash.eq_ignore_ascii_case(&b.sha1)
                }
            }) else {
                continue;
            };
            let b = want.remove(pos);
            let Some(rel) = mrpack::safe_rel(&b.rel) else { continue };
            let at = game_dir.join(rel);
            if let Some(p) = at.parent() {
                std::fs::create_dir_all(p)?;
            }
            download::write_atomic(&at, &bytes)?;
            if let Some(s) = &b.source {
                let _ = mods::index_sources(game_dir, &[(b.rel.clone(), s.clone())]);
            }
            found.push(b.title);
        }
    }
    Ok((found, want))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-blk-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn picks_up_renamed_download_by_hash() {
        let (gd, dl) = (tmp("gd"), tmp("dl"));
        let body = b"jar bytes".to_vec();
        let list = vec![
            Blocked {
                title: "Structory".into(),
                file_name: "Structory_1.20.1.jar".into(),
                rel: "mods/Structory_1.20.1.jar".into(),
                sha1: download::sha1_hex(&body),
                size: body.len() as u64,
                url: "https://www.curseforge.com/minecraft/mc-mods/structory/download/1".into(),
                source: Some(mods::Source { project_id: "cf:1".into(), version_id: "cf:2".into(), title: "Structory".into(), ..Default::default() }),
            },
            Blocked { title: "Other".into(), file_name: "o.jar".into(), rel: "mods/o.jar".into(), size: 3, sha1: "00".repeat(20), url: String::new(), source: None },
        ];
        save(&gd, &list).unwrap();
        std::fs::write(dl.join("Structory_1.20.1 (1).jar"), &body).unwrap();
        std::fs::write(dl.join("o.jar"), b"bad").unwrap(); // right name, wrong hash
        let (found, left) = resolve(&gd, &[dl.clone()]).unwrap();
        assert_eq!(found, ["Structory"]);
        assert_eq!(left.len(), 1);
        assert_eq!(std::fs::read(gd.join("mods/Structory_1.20.1.jar")).unwrap(), body);
        assert_eq!(pending(&gd).unwrap().len(), 1);
        let idx = mods::load_index(&gd, crate::modrinth::Kind::Mod).unwrap();
        assert!(idx.contains_key("Structory_1.20.1.jar"));
        save(&gd, &[]).unwrap();
        assert!(!gd.join(FILE).exists());
    }
}