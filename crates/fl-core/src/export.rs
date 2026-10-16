//! `.mrpack` export, as in Modrinth App: files Modrinth hosts go into the index
//! by hash, everything else into `overrides/`.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use sha2::Sha512;

use crate::{instances::Instance, modrinth, Core, CoreError};

/// Pre-checked in the export dialog (Modrinth's defaults).
pub const DEFAULT_SELECTED: &[&str] = &[
    "mods",
    "datapacks",
    "resourcepacks",
    "shaderpacks",
    "config",
];
/// Never exported: launcher/loader caches and natives (Modrinth's list).
const NEVER: &[&str] = &[
    "profile.json",
    "modrinth_logs",
    "mods/.connector",
    ".sable/natives",
    "local/crash_assistant",
    "mods/mcef-libraries",
    "mods/mcef-cache",
    "config/super_resolution/libraries",
    "config/Veinminer/update",
    "config/epicfight/native",
    "essential",
    ".mixin.out",
    ".fabric",
    "__MACOSX",
];
/// Folders whose files may be on Modrinth (looked up by hash).
const HOSTED: &[&str] = &["mods", "resourcepacks", "shaderpacks", "datapacks"];
const MAX_FILES: usize = 20_000;

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub path: String,
    pub dir: bool,
    pub selected: bool,
    pub files: usize,
    pub size: u64,
}

#[derive(Debug, Serialize)]
pub struct Exported {
    pub path: String,
    pub file: String,
    /// Files referenced from Modrinth / copied into the pack.
    pub linked: usize,
    pub bundled: usize,
}

fn bad(m: impl Into<String>) -> CoreError {
    CoreError::BadArgs(m.into())
}

fn zerr(e: zip::result::ZipError) -> CoreError {
    CoreError::msg(format!("Не удалось записать сборку: {e}"))
}

fn hex(d: impl AsRef<[u8]>) -> String {
    d.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn exportable(rel: &str) -> bool {
    let top_file = !rel.contains('/');
    !(NEVER
        .iter()
        .any(|n| rel == *n || rel.starts_with(&format!("{n}/")))
        || rel.ends_with(".DS_Store")
        || (top_file && rel.starts_with("freedom-") && rel.ends_with(".json")))
}

/// Exportable files under `rel` (file or folder) as `/` paths. Symlinks are skipped.
fn walk(root: &Path, rel: &str, out: &mut Vec<String>) {
    if out.len() >= MAX_FILES || !exportable(rel) {
        return;
    }
    let Ok(m) = std::fs::symlink_metadata(root.join(rel)) else {
        return;
    };
    if m.is_file() {
        out.push(rel.to_owned());
    } else if m.is_dir() {
        let Ok(rd) = std::fs::read_dir(root.join(rel)) else {
            return;
        };
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for n in names {
            walk(root, &format!("{rel}/{n}"), out);
        }
    }
}

/// Top-level entries of the game folder, for the export dialog.
pub fn candidates(game_dir: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(game_dir) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let mut files = Vec::new();
        walk(game_dir, &name, &mut files);
        if files.is_empty() {
            continue;
        }
        let size = files
            .iter()
            .filter_map(|f| std::fs::metadata(game_dir.join(f)).ok())
            .map(|m| m.len())
            .sum();
        out.push(Candidate {
            selected: DEFAULT_SELECTED.contains(&name.as_str()),
            dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
            files: files.len(),
            path: name,
            size,
        });
    }
    out.sort_by(|a, b| {
        b.selected
            .cmp(&a.selected)
            .then(b.dir.cmp(&a.dir))
            .then(a.path.cmp(&b.path))
    });
    out
}

pub fn dependencies(inst: &Instance) -> BTreeMap<String, String> {
    let mut d = BTreeMap::from([("minecraft".to_owned(), inst.version.clone())]);
    let key = match inst.loader.as_str() {
        "fabric" => Some("fabric-loader"),
        "quilt" => Some("quilt-loader"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        _ => None,
    };
    if let (Some(k), false) = (key, inst.loader_version.is_empty()) {
        d.insert(k.to_owned(), inst.loader_version.clone());
    }
    d
}

fn hosted_candidate(f: &str) -> bool {
    let mut it = f.split('/');
    matches!((it.next(), it.next(), it.next()), (Some(t), Some(_), None) if HOSTED.contains(&t))
        && !f.ends_with(".disabled")
}

/// Writes `index` plus `overrides/<file>` for each bundled file to `out`.
pub fn write_pack(
    out: &Path,
    index: &Value,
    game_dir: &Path,
    bundled: &[String],
) -> Result<(), CoreError> {
    let tmp = out.with_extension("mrpack.part");
    let res = (|| {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&tmp)?);
        let opts = zip::write::SimpleFileOptions::default().large_file(true);
        w.start_file("modrinth.index.json", opts).map_err(zerr)?;
        w.write_all(&serde_json::to_vec_pretty(index)?)?;
        for f in bundled {
            w.start_file(format!("overrides/{f}"), opts).map_err(zerr)?;
            std::io::copy(&mut std::fs::File::open(game_dir.join(f))?, &mut w)?;
        }
        w.finish().map_err(zerr)?;
        std::fs::rename(&tmp, out)?;
        Ok(())
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

async fn by_hash(
    c: &Core,
    hashes: Vec<String>,
) -> Result<HashMap<String, modrinth::Version>, CoreError> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(c.http()
        .post(format!("{}/version_files", modrinth::API))
        .json(&json!({ "hashes": hashes, "algorithm": "sha1" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub async fn export(
    c: &Core,
    inst: &Instance,
    paths: &[String],
    name: &str,
    version: &str,
) -> Result<Exported, CoreError> {
    let name = name.trim();
    let version = match version.trim() {
        "" => "1.0.0",
        v => v,
    };
    if name.is_empty() || name.chars().count() > 64 || version.chars().count() > 32 {
        return Err(bad("Укажите название (до 64 символов) и версию"));
    }
    let game_dir = inst.game_dir(&c.data_dir);
    let mut files = Vec::new();
    for p in paths {
        let rel =
            crate::mrpack::safe_rel(p).ok_or_else(|| bad(format!("Недопустимый путь: {p}")))?;
        walk(
            &game_dir,
            &rel.to_string_lossy().replace('\\', "/"),
            &mut files,
        );
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        return Err(bad("Выберите, что положить в сборку"));
    }
    let mut sums: HashMap<String, (String, String, u64)> = HashMap::new();
    for f in files.iter().filter(|f| hosted_candidate(f)) {
        let b = std::fs::read(game_dir.join(f))?;
        sums.insert(
            f.clone(),
            (
                hex(Sha1::digest(&b)),
                hex(Sha512::digest(&b)),
                b.len() as u64,
            ),
        );
    }
    // Offline or Modrinth down: everything is bundled, the pack still works.
    let hosted = by_hash(c, sums.values().map(|s| s.0.clone()).collect())
        .await
        .unwrap_or_default();
    let (mut linked, mut bundled) = (Vec::new(), Vec::new());
    for f in &files {
        let hit = sums.get(f).and_then(|s| {
            let v = hosted.get(&s.0)?;
            let x = v.files.iter().find(|x| {
                x.hashes.sha1.eq_ignore_ascii_case(&s.0)
                    && x.url.starts_with("https://cdn.modrinth.com/")
            })?;
            Some((s, x.url.clone()))
        });
        match hit {
            Some((s, url)) => linked.push(json!({
                "path": f, "hashes": { "sha1": s.0, "sha512": s.1 },
                "env": { "client": "required", "server": "required" }, "downloads": [url], "fileSize": s.2,
            })),
            None => bundled.push(f.clone()),
        }
    }
    let index = json!({
        "formatVersion": 1, "game": "minecraft", "versionId": version, "name": name,
        "files": linked, "dependencies": dependencies(inst),
    });
    let dir = c.data_dir.join("exports");
    std::fs::create_dir_all(&dir)?;
    let base = format!("{} {version}", crate::instances::folder_base(name));
    let mut out: PathBuf = dir.join(format!("{base}.mrpack"));
    for n in 2.. {
        if !out.exists() {
            break;
        }
        out = dir.join(format!("{base} ({n}).mrpack"));
    }
    write_pack(&out, &index, &game_dir, &bundled)?;
    Ok(Exported {
        file: out
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: out.to_string_lossy().into_owned(),
        linked: linked.len(),
        bundled: bundled.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusions_follow_modrinth() {
        assert!(
            exportable("mods/sodium.jar")
                && exportable("config/a.toml")
                && exportable("options.txt")
        );
        assert!(
            !exportable("essential")
                && !exportable("essential/x")
                && !exportable("mods/.connector/a")
        );
        assert!(!exportable("config/.DS_Store") && !exportable("freedom-mods.json"));
        assert!(
            hosted_candidate("mods/a.jar")
                && !hosted_candidate("mods/a.jar.disabled")
                && !hosted_candidate("config/a/b.jar")
        );
    }

    #[test]
    fn exported_pack_imports_back() {
        let root = std::env::temp_dir().join(format!("fl-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("config/a.toml"), b"x=1").unwrap();
        std::fs::write(root.join("freedom-mods.json"), b"{}").unwrap();
        let mut files = Vec::new();
        walk(&root, "config", &mut files);
        walk(&root, "freedom-mods.json", &mut files);
        assert_eq!(files, ["config/a.toml"]);
        let c = candidates(&root);
        assert_eq!(
            (c.len(), c[0].path.as_str(), c[0].selected),
            (1, "config", true)
        );
        let idx = json!({ "formatVersion": 1, "game": "minecraft", "versionId": "1.0.0", "name": "T", "files": [],
            "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.9" } });
        let out = root.join("t.mrpack");
        write_pack(&out, &idx, &root, &files).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let plan =
            crate::mrpack::plan(crate::mrpack::read_index(&mut z).unwrap(), Path::new("")).unwrap();
        assert_eq!(
            (plan.mc.as_str(), plan.loader.as_str()),
            ("1.21.1", "fabric")
        );
        assert!(z.by_name("overrides/config/a.toml").is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }
}
