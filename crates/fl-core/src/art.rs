//! The user's own instance pictures: an icon and a cover per instance, already cropped
//! by the launcher. Kept in `art/<instance>.<icon|cover>.<ext>` under the data folder;
//! the type comes from the bytes, as with the wallpaper.

use std::path::{Path, PathBuf};

use crate::{wallpaper, CoreError};

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const KINDS: [&str; 2] = ["icon", "cover"];
const EXTS: [&str; 4] = ["png", "jpg", "webp", "gif"];

fn ok_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn base(data_dir: &Path, id: &str, kind: &str) -> Result<PathBuf, CoreError> {
    if !ok_id(id) || !KINDS.contains(&kind) {
        return Err(CoreError::BadArgs(format!("art: {id}/{kind}")));
    }
    Ok(data_dir.join("art").join(format!("{id}.{kind}")))
}

/// Returns the picture's name in the library (see [`library`]).
pub fn save(data_dir: &Path, id: &str, kind: &str, bytes: &[u8]) -> Result<String, CoreError> {
    let b = base(data_dir, id, kind)?;
    if bytes.len() > MAX_BYTES {
        return Err(CoreError::msg("Картинка больше 8 МБ"));
    }
    let (ext, _) = wallpaper::kind(bytes)
        .ok_or_else(|| CoreError::msg("Нужна картинка PNG, JPEG, WebP или GIF"))?;
    std::fs::create_dir_all(b.parent().unwrap())?;
    let tmp = b.with_extension(format!("{kind}.tmp"));
    std::fs::write(&tmp, bytes)?;
    clear(data_dir, id, kind)?;
    std::fs::rename(&tmp, b.with_extension(format!("{kind}.{ext}")))?;
    remember(data_dir, kind, bytes)
}

pub fn read(data_dir: &Path, id: &str, kind: &str) -> Option<(Vec<u8>, &'static str)> {
    let b = base(data_dir, id, kind).ok()?;
    EXTS.iter().find_map(|ext| {
        let bytes = std::fs::read(b.with_extension(format!("{kind}.{ext}"))).ok()?;
        Some((bytes.clone(), wallpaper::kind(&bytes)?.1))
    })
}

pub fn clear(data_dir: &Path, id: &str, kind: &str) -> Result<(), CoreError> {
    let b = base(data_dir, id, kind)?;
    for ext in EXTS {
        match std::fs::remove_file(b.with_extension(format!("{kind}.{ext}"))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

/// Both pictures of a deleted instance (best effort).
pub fn forget(data_dir: &Path, id: &str) {
    for k in KINDS {
        let _ = clear(data_dir, id, k);
    }
}

/// A duplicated instance keeps its pictures.
pub fn copy(data_dir: &Path, from: &str, to: &str) {
    for k in KINDS {
        if let Some((bytes, _)) = read(data_dir, from, k) {
            let _ = save(data_dir, to, k, &bytes);
        }
    }
}

// ---------- Library: every picture the user cropped, by content ----------
// `art/lib/<kind>-<sha1 16 hex>.<ext>`, so it can be picked again for any instance.

fn lib_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("art").join("lib")
}

/// The kind of a valid library name, `None` for anything else (paths, odd names).
fn lib_kind(name: &str) -> Option<&'static str> {
    let (stem, ext) = name.rsplit_once('.')?;
    let (kind, hash) = stem.split_once('-')?;
    let kind = *KINDS.iter().find(|k| **k == kind)?;
    let hex = hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    (EXTS.contains(&ext) && hex).then_some(kind)
}

pub fn remember(data_dir: &Path, kind: &str, bytes: &[u8]) -> Result<String, CoreError> {
    use sha1::{Digest, Sha1};
    if !KINDS.contains(&kind) {
        return Err(CoreError::BadArgs(format!("art: {kind}")));
    }
    let (ext, _) = wallpaper::kind(bytes).ok_or_else(|| CoreError::msg("Нужна картинка PNG, JPEG, WebP или GIF"))?;
    let hash: String = Sha1::digest(bytes).iter().take(8).map(|b| format!("{b:02x}")).collect();
    let name = format!("{kind}-{hash}.{ext}");
    let p = lib_dir(data_dir).join(&name);
    if !p.exists() {
        std::fs::create_dir_all(lib_dir(data_dir))?;
        let tmp = p.with_extension("part");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &p)?;
    }
    Ok(name)
}

/// Library names of one kind, newest first. Pictures saved before the library
/// existed join it on the first call.
pub fn library(data_dir: &Path, kind: &str) -> Vec<String> {
    if !KINDS.contains(&kind) {
        return Vec::new();
    }
    let mark = lib_dir(data_dir).join(format!(".seeded-{kind}"));
    if !mark.exists() {
        for e in std::fs::read_dir(data_dir.join("art")).into_iter().flatten().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let p: Vec<&str> = n.split('.').collect();
            if p.len() == 3 && p[1] == kind && EXTS.contains(&p[2]) {
                if let Ok(b) = std::fs::read(e.path()) {
                    let _ = remember(data_dir, kind, &b);
                }
            }
        }
        let _ = std::fs::create_dir_all(lib_dir(data_dir));
        let _ = std::fs::write(&mark, b"");
    }
    let mut out: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(lib_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (lib_kind(&n) == Some(kind)).then(|| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), n))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    out.into_iter().take(48).map(|(_, n)| n).collect()
}

pub fn lib_read(data_dir: &Path, name: &str) -> Option<(Vec<u8>, &'static str)> {
    lib_kind(name)?;
    let bytes = std::fs::read(lib_dir(data_dir).join(name)).ok()?;
    let mime = wallpaper::kind(&bytes)?.1;
    Some((bytes, mime))
}

/// Puts a library picture on an instance; returns its kind.
pub fn use_lib(data_dir: &Path, id: &str, name: &str) -> Result<&'static str, CoreError> {
    let kind = lib_kind(name).ok_or_else(|| CoreError::BadArgs(format!("art: {name}")))?;
    let bytes = std::fs::read(lib_dir(data_dir).join(name)).map_err(|_| CoreError::msg("Картинки больше нет"))?;
    save(data_dir, id, kind, &bytes)?;
    Ok(kind)
}

pub fn lib_remove(data_dir: &Path, name: &str) -> Result<(), CoreError> {
    lib_kind(name).ok_or_else(|| CoreError::BadArgs(format!("art: {name}")))?;
    match std::fs::remove_file(lib_dir(data_dir).join(name)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
    const JPG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 1, 2];

    #[test]
    fn save_read_replace_copy_forget() {
        let dir = std::env::temp_dir().join(format!("fl-art-{}", uuid::Uuid::new_v4()));
        assert!(read(&dir, "a-1", "icon").is_none());
        save(&dir, "a-1", "icon", PNG).unwrap();
        save(&dir, "a-1", "cover", PNG).unwrap();
        save(&dir, "a-1", "cover", JPG).unwrap();
        assert_eq!(read(&dir, "a-1", "icon").unwrap().1, "image/png");
        assert_eq!(read(&dir, "a-1", "cover").unwrap(), (JPG.to_vec(), "image/jpeg"));
        assert!(!dir.join("art/a-1.cover.png").exists());
        copy(&dir, "a-1", "b-2");
        assert!(read(&dir, "b-2", "cover").is_some());
        forget(&dir, "a-1");
        assert!(read(&dir, "a-1", "icon").is_none() && read(&dir, "a-1", "cover").is_none());
        assert!(save(&dir, "../x", "icon", PNG).is_err());
        assert!(save(&dir, "a", "wall", PNG).is_err());
        assert!(save(&dir, "a", "icon", b"<svg>").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn library_keeps_every_picture() {
        let dir = std::env::temp_dir().join(format!("fl-artlib-{}", uuid::Uuid::new_v4()));
        let a = save(&dir, "a-1", "cover", PNG).unwrap();
        save(&dir, "a-1", "cover", JPG).unwrap();
        save(&dir, "a-1", "icon", PNG).unwrap();
        let covers = library(&dir, "cover");
        assert_eq!(covers.len(), 2);
        assert!(covers.contains(&a) && a.starts_with("cover-") && a.ends_with(".png"));
        assert_eq!(library(&dir, "icon").len(), 1);
        assert_eq!(use_lib(&dir, "b-2", &a).unwrap(), "cover");
        assert_eq!(read(&dir, "b-2", "cover").unwrap().0, PNG.to_vec());
        assert!(use_lib(&dir, "b-2", "../x.png").is_err() && lib_read(&dir, "cover-zz.png").is_none());
        lib_remove(&dir, &a).unwrap();
        assert_eq!(library(&dir, "cover").len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
