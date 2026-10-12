//! Small JSON state files, written atomically (temp file + rename).

use std::{fs, io::Write, path::Path};

use serde::{de::DeserializeOwned, Serialize};

use crate::CoreError;

pub fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T, CoreError> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

/// `private = true` restricts the file to the current user (0600 on Unix).
pub fn save<T: Serialize>(path: &Path, value: &T, private: bool) -> Result<(), CoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = open(&tmp, private)?;
        f.write_all(&serde_json::to_vec_pretty(value)?)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(unix)]
fn open(path: &Path, private: bool) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(if private { 0o600 } else { 0o644 })
        .open(path)
}

#[cfg(not(unix))]
fn open(path: &Path, _private: bool) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}
