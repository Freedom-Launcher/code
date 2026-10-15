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
        check_name(&f.filename)?;
        let bytes = core
            .http()
            .get(&f.url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        if !f.hashes.sha1.is_empty()
            && !download::sha1_hex(&bytes).eq_ignore_ascii_case(&f.hashes.sha1)
        {
            return Err(CoreError::msg(format!("checksum mismatch: {}", f.filename)));
        }
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

    #[test]
    fn missing_mods_dir_is_empty_list() {
        let d = std::env::temp_dir().join(format!("fl-mods-{}", uuid::Uuid::new_v4()));
        assert!(list(&d).unwrap().is_empty());
    }
}
