//! Mods of one instance: the `mods/` folder plus a small index that remembers
//! where each jar came from (Modrinth project, version, icon).
//! A disabled mod is the same jar renamed to `<file>.disabled`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{game::download, modrinth, store, Core, CoreError};

const INDEX: &str = "freedom-mods.json";
const OFF: &str = ".disabled";
/// Guards against dependency cycles and runaway chains.
const MAX_DEPS: usize = 32;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

/// Jar file name (without `.disabled`) → where it came from.
pub type Index = BTreeMap<String, Source>;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModEntry {
    /// Jar file name without `.disabled`; the id used by the actions.
    pub file: String,
    pub enabled: bool,
    pub title: String,
    #[serde(flatten)]
    pub source: Option<Source>,
}

pub fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

pub fn load_index(game_dir: &Path) -> Result<Index, CoreError> {
    store::load(&game_dir.join(INDEX))
}

fn save_index(game_dir: &Path, idx: &Index) -> Result<(), CoreError> {
    store::save(&game_dir.join(INDEX), idx, false)
}

/// Rejects anything that is not a plain jar name inside `mods/`.
pub fn check_name(file: &str) -> Result<(), CoreError> {
    let ok = file.ends_with(".jar")
        && !file.starts_with('.')
        && !file.contains(['/', '\\', ':'])
        && !file.contains("..");
    if ok {
        Ok(())
    } else {
        Err(CoreError::BadArgs(format!("bad mod file: {file}")))
    }
}

pub fn list(game_dir: &Path) -> Result<Vec<ModEntry>, CoreError> {
    let idx = load_index(game_dir)?;
    let rd = match std::fs::read_dir(mods_dir(game_dir)) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let (file, enabled) = match name.strip_suffix(OFF) {
            Some(f) => (f.to_owned(), false),
            None => (name, true),
        };
        if check_name(&file).is_err() {
            continue;
        }
        let source = idx.get(&file).cloned();
        let title = source.as_ref().map_or_else(
            || file.trim_end_matches(".jar").to_owned(),
            |s| s.title.clone(),
        );
        out.push(ModEntry {
            file,
            enabled,
            title,
            source,
        });
    }
    out.sort_by_key(|m| m.title.to_lowercase());
    Ok(out)
}

pub fn set_enabled(game_dir: &Path, file: &str, enabled: bool) -> Result<(), CoreError> {
    check_name(file)?;
    let on = mods_dir(game_dir).join(file);
    let off = mods_dir(game_dir).join(format!("{file}{OFF}"));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if from.exists() {
        std::fs::rename(from, to)?;
    } else if !to.exists() {
        return Err(CoreError::BadArgs(format!("no such mod: {file}")));
    }
    Ok(())
}

pub fn remove(game_dir: &Path, file: &str) -> Result<(), CoreError> {
    check_name(file)?;
    for name in [file.to_owned(), format!("{file}{OFF}")] {
        match std::fs::remove_file(mods_dir(game_dir).join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    let mut idx = load_index(game_dir)?;
    if idx.remove(file).is_some() {
        save_index(game_dir, &idx)?;
    }
    Ok(())
}

/// Installs a Modrinth project and its required dependencies into the
/// instance. Already installed projects are left alone. Returns the titles
/// of what was added.
pub async fn install(
    core: &Core,
    game_dir: &Path,
    mc: &str,
    loader: &str,
    project_id: &str,
) -> Result<Vec<String>, CoreError> {
    let mut idx = load_index(game_dir)?;
    let mut have: Vec<String> = idx.values().map(|s| s.project_id.clone()).collect();
    let mut queue = vec![project_id.to_owned()];
    let mut added = Vec::new();
    while let Some(id) = queue.pop() {
        if have.contains(&id) {
            continue;
        }
        if added.len() >= MAX_DEPS {
            return Err(CoreError::msg("too many dependencies"));
        }
        let (project, versions) = futures_util::future::try_join(
            modrinth::project(core, &id),
            modrinth::versions(core, &id, mc, loader),
        )
        .await?;
        let v = modrinth::pick(&versions, mc, loader).ok_or_else(|| {
            CoreError::msg(format!("{}: нет версии для {mc} ({loader})", project.title))
        })?;
        let f = v.primary_file().expect("pick keeps versions with files");
        let bytes = fetch_checked(core, f).await?;
        download::write_atomic(&mods_dir(game_dir).join(&f.filename), &bytes)?;
        idx.insert(
            f.filename.clone(),
            Source {
                project_id: project.id.clone(),
                version_id: v.id.clone(),
                title: project.title.clone(),
                version_number: v.version_number.clone(),
                icon_url: project.icon_url.clone(),
            },
        );
        save_index(game_dir, &idx)?;
        have.push(project.id.clone());
        have.push(id);
        added.push(project.title);
        queue.extend(v.required().map(str::to_owned));
    }
    Ok(added)
}

/// Downloads a version file, refusing bad names and checksum mismatches.
async fn fetch_checked(core: &Core, f: &modrinth::VersionFile) -> Result<Vec<u8>, CoreError> {
    check_name(&f.filename)?;
    let bytes = core
        .http()
        .get(&f.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !f.hashes.sha1.is_empty() && !download::sha1_hex(&bytes).eq_ignore_ascii_case(&f.hashes.sha1)
    {
        return Err(CoreError::msg(format!("checksum mismatch: {}", f.filename)));
    }
    Ok(bytes.to_vec())
}

/// A newer version available for an installed mod.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Update {
    pub file: String,
    pub title: String,
    pub version_id: String,
    pub version_number: String,
}

/// Where to move from `current` (the installed version and file hash): the
/// newest release, else the newest of any type. Never offers a beta over a
/// release, and nothing when the installed file already is the newest.
pub fn choose_update<'a>(
    current: &str,
    hash: &str,
    any: Option<&'a modrinth::Version>,
    release: Option<&'a modrinth::Version>,
) -> Option<&'a modrinth::Version> {
    let same = |v: &modrinth::Version| {
        v.id == current
            || v.primary_file()
                .is_some_and(|f| f.hashes.sha1.eq_ignore_ascii_case(hash))
    };
    if any.is_some_and(same) {
        return None;
    }
    release
        .or(any)
        .filter(|v| !same(v) && v.primary_file().is_some())
}

/// Indexed mods as (file id, path on disk incl. `.disabled`, sha1).
async fn hashed(game_dir: &Path) -> Result<Vec<(String, PathBuf, String)>, CoreError> {
    let entries = list(game_dir)?;
    let dir = mods_dir(game_dir);
    tokio::task::spawn_blocking(move || {
        entries
            .into_iter()
            .filter(|m| m.source.is_some())
            .map(|m| {
                let name = if m.enabled {
                    m.file.clone()
                } else {
                    format!("{}{OFF}", m.file)
                };
                let path = dir.join(name);
                let sha1 = download::sha1_hex(&std::fs::read(&path)?);
                Ok((m.file, path, sha1))
            })
            .collect::<Result<Vec<_>, CoreError>>()
    })
    .await
    .map_err(|e| CoreError::msg(e.to_string()))?
}

/// Mods with a newer fitting version: (file id, path, new version).
async fn pending(
    core: &Core,
    game_dir: &Path,
    mc: &str,
    loader: &str,
) -> Result<Vec<(String, PathBuf, modrinth::Version)>, CoreError> {
    let idx = load_index(game_dir)?;
    let files = hashed(game_dir).await?;
    let hashes: Vec<String> = files.iter().map(|f| f.2.clone()).collect();
    let (any, rel) = futures_util::future::try_join(
        modrinth::latest_by_hash(core, &hashes, mc, loader, false),
        modrinth::latest_by_hash(core, &hashes, mc, loader, true),
    )
    .await?;
    let mut out = Vec::new();
    for (file, path, h) in files {
        let Some(src) = idx.get(&file) else { continue };
        if let Some(v) = choose_update(&src.version_id, &h, any.get(&h), rel.get(&h)) {
            if v.project_id == src.project_id {
                out.push((file, path, v.clone()));
            }
        }
    }
    Ok(out)
}

/// Updates one mod (`only`) or all of them. A disabled mod stays disabled.
/// The new jar is written before the old one is removed, so a failed
/// download never leaves the instance without the mod. Returns titles.
pub async fn apply_updates(
    core: &Core,
    game_dir: &Path,
    mc: &str,
    loader: &str,
    only: Option<&str>,
) -> Result<Vec<String>, CoreError> {
    if let Some(f) = only {
        check_name(f)?;
    }
    let mut todo = pending(core, game_dir, mc, loader).await?;
    todo.retain(|(file, ..)| only.is_none_or(|o| o == file));
    let mut idx = load_index(game_dir)?;
    let mut done = Vec::new();
    for (file, old_path, v) in todo {
        let f = v.primary_file().expect("pending keeps versions with files");
        let new_name = f.filename.clone();
        if new_name != file && idx.contains_key(&new_name) {
            continue; // another indexed mod already uses that name
        }
        let disabled = old_path.to_string_lossy().ends_with(OFF);
        let bytes = fetch_checked(core, f).await?;
        let on_disk = if disabled {
            format!("{new_name}{OFF}")
        } else {
            new_name.clone()
        };
        let new_path = mods_dir(game_dir).join(on_disk);
        download::write_atomic(&new_path, &bytes)?;
        if new_path != old_path {
            std::fs::remove_file(&old_path)?;
        }
        let Some(mut src) = idx.remove(&file) else {
            continue;
        };
        src.version_id = v.id.clone();
        src.version_number = v.version_number.clone();
        done.push(src.title.clone());
        idx.insert(new_name, src);
        save_index(game_dir, &idx)?;
    }
    Ok(done)
}

pub async fn updates(
    core: &Core,
    game_dir: &Path,
    mc: &str,
    loader: &str,
) -> Result<Vec<Update>, CoreError> {
    let idx = load_index(game_dir)?;
    Ok(pending(core, game_dir, mc, loader)
        .await?
        .into_iter()
        .map(|(file, _, v)| Update {
            title: idx.get(&file).map(|s| s.title.clone()).unwrap_or_default(),
            file,
            version_id: v.id,
            version_number: v.version_number,
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-mods-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(mods_dir(&d)).unwrap();
        d
    }

    #[test]
    fn names_outside_mods_are_rejected() {
        for bad in [
            "../x.jar",
            "a/b.jar",
            "a\\b.jar",
            "x.txt",
            ".hidden.jar",
            "C:x.jar",
        ] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
        assert!(check_name("fabric-api-0.119.4+1.21.4.jar").is_ok());
    }

    #[test]
    fn list_toggle_remove() {
        let d = tmp();
        std::fs::write(mods_dir(&d).join("sodium.jar"), b"a").unwrap();
        std::fs::write(mods_dir(&d).join("zoom.jar.disabled"), b"b").unwrap();
        std::fs::write(mods_dir(&d).join("notes.txt"), b"c").unwrap();
        let mut idx = Index::new();
        idx.insert(
            "sodium.jar".into(),
            Source {
                project_id: "AANobbMI".into(),
                title: "Sodium".into(),
                ..Default::default()
            },
        );
        save_index(&d, &idx).unwrap();

        let l = list(&d).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].title.as_str(), l[0].enabled), ("Sodium", true));
        assert_eq!((l[1].file.as_str(), l[1].enabled), ("zoom.jar", false));

        set_enabled(&d, "sodium.jar", false).unwrap();
        set_enabled(&d, "sodium.jar", false).unwrap(); // idempotent
        assert!(mods_dir(&d).join("sodium.jar.disabled").exists());
        set_enabled(&d, "zoom.jar", true).unwrap();
        assert!(mods_dir(&d).join("zoom.jar").exists());
        assert!(set_enabled(&d, "missing.jar", true).is_err());

        remove(&d, "sodium.jar").unwrap();
        assert!(!mods_dir(&d).join("sodium.jar.disabled").exists());
        assert!(load_index(&d).unwrap().is_empty());
        assert_eq!(list(&d).unwrap().len(), 1);
        std::fs::remove_dir_all(d).unwrap();
    }

    fn ver(id: &str, ty: &str, sha1: &str) -> modrinth::Version {
        serde_json::from_value(serde_json::json!({
            "id": id, "project_id": "p", "version_number": id, "version_type": ty,
            "files": [{ "url": "u", "filename": format!("{id}.jar"), "primary": true,
                        "hashes": { "sha1": sha1 } }]
        }))
        .unwrap()
    }

    #[test]
    fn update_choice() {
        let (rel, beta, cur) = (
            ver("rel", "release", "aa"),
            ver("beta", "beta", "bb"),
            ver("cur", "release", "cc"),
        );
        // Newer release wins over a newer beta.
        assert_eq!(
            choose_update("cur", "cc", Some(&beta), Some(&rel))
                .unwrap()
                .id,
            "rel"
        );
        // Only betas exist: offer the beta.
        assert_eq!(
            choose_update("cur", "cc", Some(&beta), None).unwrap().id,
            "beta"
        );
        // Already newest (by id or by hash): nothing.
        assert!(choose_update("cur", "cc", Some(&cur), Some(&cur)).is_none());
        assert!(choose_update("old-id", "cc", Some(&cur), None).is_none());
        // Installed a beta that is newer than the newest release: no downgrade.
        assert!(choose_update("beta", "bb", Some(&beta), Some(&rel)).is_none());
        assert!(choose_update("cur", "cc", None, None).is_none());
    }

    #[test]
    fn missing_mods_dir_is_empty_list() {
        let d = std::env::temp_dir().join(format!("fl-mods-{}", uuid::Uuid::new_v4()));
        assert!(list(&d).unwrap().is_empty());
    }
}
