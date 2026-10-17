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

pub fn save(data_dir: &Path, id: &str, kind: &str, bytes: &[u8]) -> Result<(), CoreError> {
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
    Ok(())
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
}
