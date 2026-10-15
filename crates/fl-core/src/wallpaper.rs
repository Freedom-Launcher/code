//! The user's own wallpaper behind the glass: a single image in the data folder.
//! The type is taken from the file's bytes, not its name, so only real images are kept.

use std::path::Path;

use crate::CoreError;

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
const EXTS: [&str; 4] = ["png", "jpg", "webp", "gif"];

/// `(extension, mime)` of a supported image, by its magic bytes.
pub fn kind(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some(("png", "image/png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("jpg", "image/jpeg"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("webp", "image/webp"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(("gif", "image/gif"))
    } else {
        None
    }
}

/// Replaces the wallpaper with `bytes`.
pub fn save(data_dir: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    if bytes.len() > MAX_BYTES {
        return Err(CoreError::msg("Картинка больше 16 МБ"));
    }
    let (ext, _) =
        kind(bytes).ok_or_else(|| CoreError::msg("Нужна картинка PNG, JPEG, WebP или GIF"))?;
    std::fs::create_dir_all(data_dir)?;
    let tmp = data_dir.join("wallpaper.tmp");
    std::fs::write(&tmp, bytes)?;
    clear(data_dir)?;
    std::fs::rename(&tmp, data_dir.join(format!("wallpaper.{ext}")))?;
    Ok(())
}

/// The current wallpaper and its mime type, if one is set.
pub fn read(data_dir: &Path) -> Option<(Vec<u8>, &'static str)> {
    EXTS.iter().find_map(|ext| {
        let bytes = std::fs::read(data_dir.join(format!("wallpaper.{ext}"))).ok()?;
        let (_, mime) = kind(&bytes)?;
        Some((bytes, mime))
    })
}

pub fn clear(data_dir: &Path) -> Result<(), CoreError> {
    for ext in EXTS {
        match std::fs::remove_file(data_dir.join(format!("wallpaper.{ext}"))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
    const JPG: &[u8] = &[0xff, 0xd8, 0xff, 0xe0, 1, 2];

    #[test]
    fn save_read_replace_clear() {
        let dir = std::env::temp_dir().join(format!("fl-wall-{}", uuid::Uuid::new_v4()));
        assert!(read(&dir).is_none());
        save(&dir, PNG).unwrap();
        assert_eq!(read(&dir).unwrap().1, "image/png");
        save(&dir, JPG).unwrap();
        let (bytes, mime) = read(&dir).unwrap();
        assert_eq!((bytes.as_slice(), mime), (JPG, "image/jpeg"));
        assert!(!dir.join("wallpaper.png").exists());
        clear(&dir).unwrap();
        assert!(read(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_images_and_huge_files() {
        let dir = std::env::temp_dir().join(format!("fl-wall-{}", uuid::Uuid::new_v4()));
        assert!(save(&dir, b"<svg onload=alert(1)>").is_err());
        let mut big = JPG.to_vec();
        big.resize(MAX_BYTES + 1, 0);
        assert!(save(&dir, &big).is_err());
        assert!(read(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
