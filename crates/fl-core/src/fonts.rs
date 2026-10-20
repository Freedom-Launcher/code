//! The user's own interface font: a single font file in the data folder.
//! The type is taken from the file's bytes, not its name, so only real fonts are kept.

use std::path::Path;

use crate::CoreError;

pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const EXTS: [&str; 4] = ["ttf", "otf", "woff", "woff2"];

/// `(extension, mime)` of a supported font, by its magic bytes.
pub fn kind(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    let head: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    match &head {
        [0, 1, 0, 0] | b"true" => Some(("ttf", "font/ttf")),
        b"OTTO" => Some(("otf", "font/otf")),
        b"wOFF" => Some(("woff", "font/woff")),
        b"wOF2" => Some(("woff2", "font/woff2")),
        _ => None,
    }
}

/// Replaces the font with `bytes`.
pub fn save(data_dir: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    if bytes.len() > MAX_BYTES {
        return Err(CoreError::msg("Шрифт больше 32 МБ"));
    }
    let (ext, _) =
        kind(bytes).ok_or_else(|| CoreError::msg("Нужен шрифт TTF, OTF, WOFF или WOFF2"))?;
    std::fs::create_dir_all(data_dir)?;
    let tmp = data_dir.join("font.tmp");
    std::fs::write(&tmp, bytes)?;
    clear(data_dir)?;
    std::fs::rename(&tmp, data_dir.join(format!("font.{ext}")))?;
    Ok(())
}

/// The current font and its mime type, if one is set.
pub fn read(data_dir: &Path) -> Option<(Vec<u8>, &'static str)> {
    EXTS.iter().find_map(|ext| {
        let bytes = std::fs::read(data_dir.join(format!("font.{ext}"))).ok()?;
        let (_, mime) = kind(&bytes)?;
        Some((bytes, mime))
    })
}

pub fn clear(data_dir: &Path) -> Result<(), CoreError> {
    for ext in EXTS {
        match std::fs::remove_file(data_dir.join(format!("font.{ext}"))) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_bytes() {
        assert_eq!(kind(&[0, 1, 0, 0, 9]).unwrap().0, "ttf");
        assert_eq!(kind(b"OTTO..").unwrap().0, "otf");
        assert_eq!(kind(b"wOF2..").unwrap().1, "font/woff2");
        assert!(kind(b"wOF").is_none());
        assert!(kind(b"\x89PNG").is_none());
    }

    #[test]
    fn save_read_replace_clear() {
        let dir = std::env::temp_dir().join(format!("fl-font-{}", uuid::Uuid::new_v4()));
        assert!(read(&dir).is_none());
        assert!(save(&dir, b"<html>").is_err());
        save(&dir, b"OTTO1234").unwrap();
        assert_eq!(read(&dir).unwrap().1, "font/otf");
        save(&dir, b"wOF2abcd").unwrap();
        assert!(!dir.join("font.otf").exists());
        assert_eq!(read(&dir).unwrap().0, b"wOF2abcd");
        clear(&dir).unwrap();
        assert!(read(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}