//! Extraction of native libraries (`.dll`, `.so`, `.dylib`) from natives jars.

use std::{fs, io, path::Path};

use crate::CoreError;

/// Extracts `jar` into `dest`, skipping `META-INF/` and any `exclude` prefix.
/// Only plain files are written; entries that would escape `dest` are ignored.
pub fn extract(jar: &Path, dest: &Path, exclude: &[String]) -> Result<usize, CoreError> {
    let file = fs::File::open(jar)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::msg(format!("bad natives archive {}: {e}", jar.display())))?;
    fs::create_dir_all(dest)?;
    let mut written = 0;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| CoreError::msg(format!("bad natives archive: {e}")))?;
        let name = entry.name().to_owned();
        if entry.is_dir()
            || name.starts_with("META-INF/")
            || exclude.iter().any(|x| name.starts_with(x.as_str()))
        {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = fs::File::create(&out)?;
        io::copy(&mut entry, &mut f)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn skips_meta_inf_and_excludes() {
        let dir = std::env::temp_dir().join(format!("fl-nat-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("n.jar");
        let mut w = zip::ZipWriter::new(fs::File::create(&jar).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for name in ["lwjgl.dll", "META-INF/MANIFEST.MF", "skip/me.txt"] {
            w.start_file(name, opts).unwrap();
            w.write_all(b"x").unwrap();
        }
        w.finish().unwrap();
        let out = dir.join("out");
        let n = extract(&jar, &out, &["skip/".into()]).unwrap();
        assert_eq!(n, 1);
        assert!(out.join("lwjgl.dll").exists());
        assert!(!out.join("META-INF").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
