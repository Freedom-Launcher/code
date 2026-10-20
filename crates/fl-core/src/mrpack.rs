//! Modrinth modpacks (`.mrpack`): a zip with `modrinth.index.json` (files to
//! download, game and loader versions) and `overrides/` copied into the
//! instance as-is. Nothing in a pack may write outside the instance folder.

use std::{
    collections::BTreeMap,
    io::{Read, Seek},
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use crate::{game::download::Task, CoreError};

const INDEX: &str = "modrinth.index.json";
/// Download hosts the Modrinth pack format allows.
const HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];
/// Cap for unpacked overrides, against zip bombs.
const MAX_OVERRIDES: u64 = 4 << 30;
/// Largest pack held in memory (the browser fallback upload).
pub const MAX_PACK: usize = 512 << 20;
/// Largest pack file staged from disk or downloaded; the file is streamed, never read whole.
pub const MAX_PACK_FILE: u64 = 32 << 30;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackIndex {
    pub format_version: u32,
    pub game: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub files: Vec<PackFile>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    pub path: String,
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub env: Option<Env>,
    #[serde(default)]
    pub downloads: Vec<String>,
    #[serde(default)]
    pub file_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct Env {
    #[serde(default)]
    pub client: String,
}

/// What importing a pack will do.
#[derive(Debug)]
pub struct Plan {
    pub name: String,
    pub mc: String,
    pub loader: String,
    pub loader_version: String,
    pub tasks: Vec<Task>,
    /// Archive folder copied over the instance (`overrides` unless a CurseForge pack says otherwise).
    pub overrides: String,
    /// Files that could not be downloaded (CurseForge files kept on its site only).
    pub blocked: Vec<String>,
    /// Catalog origin of downloaded files by path in the instance (CurseForge packs).
    pub index: Vec<(String, crate::mods::Source)>,
}

fn bad(m: impl Into<String>) -> CoreError {
    CoreError::BadArgs(m.into())
}

/// A relative path that stays inside its root: no `..`, no root, no drive.
pub fn safe_rel(p: &str) -> Option<PathBuf> {
    if p.is_empty() || p.contains(['\\', ':', '\0']) {
        return None;
    }
    let mut out = PathBuf::new();
    for c in Path::new(p).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

fn allowed(url: &str) -> bool {
    reqwest::Url::parse(url)
        .is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some_and(|h| HOSTS.contains(&h)))
}

fn loader_of(d: &BTreeMap<String, String>) -> Result<(String, String), CoreError> {
    // Quilt first: a Quilt pack may also pin fabric-loader for its Fabric mods.
    for (key, name) in [
        ("quilt-loader", "quilt"),
        ("fabric-loader", "fabric"),
        ("neoforge", "neoforge"),
        ("forge", "forge"),
    ] {
        if let Some(v) = d.get(key) {
            return Ok((name.into(), v.clone()));
        }
    }
    Ok(("vanilla".into(), String::new()))
}

pub fn read_index<R: Read + Seek>(z: &mut zip::ZipArchive<R>) -> Result<PackIndex, CoreError> {
    let mut e = z
        .by_name(INDEX)
        .map_err(|_| bad("Это не сборка Modrinth (.mrpack)"))?;
    let mut s = String::new();
    e.by_ref().take(16 << 20).read_to_string(&mut s)?;
    serde_json::from_str(&s).map_err(|e| bad(format!("Повреждённый {INDEX}: {e}")))
}

/// Checks the index and turns it into download tasks under `game_dir`.
pub fn plan(idx: PackIndex, game_dir: &Path) -> Result<Plan, CoreError> {
    if idx.format_version != 1 || idx.game != "minecraft" {
        return Err(bad("Неподдерживаемый формат сборки"));
    }
    let mc = idx
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| bad("В сборке не указана версия Minecraft"))?;
    let (loader, loader_version) = loader_of(&idx.dependencies)?;
    let mut tasks = Vec::new();
    for f in idx.files {
        if f.env.as_ref().is_some_and(|e| e.client == "unsupported") {
            continue; // server-only file
        }
        let rel = safe_rel(&f.path).ok_or_else(|| bad(format!("Недопустимый путь: {}", f.path)))?;
        let url = f
            .downloads
            .iter()
            .find(|u| allowed(u))
            .ok_or_else(|| bad(format!("Недопустимый источник файла: {}", f.path)))?;
        let sha1 = f
            .hashes
            .get("sha1")
            .filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| bad(format!("Нет контрольной суммы: {}", f.path)))?;
        tasks.push(Task {
            url: url.clone(),
            path: game_dir.join(rel),
            sha1: sha1.to_lowercase(),
            size: f.file_size,
        });
    }
    let name = match idx.name.trim() {
        "" => "Сборка".to_owned(),
        n => n.chars().take(64).collect(),
    };
    Ok(Plan {
        name,
        mc,
        loader,
        loader_version,
        tasks,
        overrides: "overrides".into(),
        blocked: Vec::new(),
        index: Vec::new(),
    })
}

/// Copies `overrides/`, then `client-overrides/` (which wins) into `game_dir`.
pub fn extract_overrides<R: Read + Seek>(
    z: &mut zip::ZipArchive<R>,
    game_dir: &Path,
) -> Result<u64, CoreError> {
    extract_overrides_from(z, game_dir, "overrides")
}

/// `extract_overrides` with the pack's own overrides folder (CurseForge packs name it).
pub fn extract_overrides_from<R: Read + Seek>(
    z: &mut zip::ZipArchive<R>,
    game_dir: &Path,
    overrides: &str,
) -> Result<u64, CoreError> {
    let main = format!("{}/", overrides.trim_matches('/'));
    let mut total = 0u64;
    for prefix in [main.as_str(), "client-overrides/"] {
        for i in 0..z.len() {
            let mut e = z.by_index(i).map_err(|e| bad(e.to_string()))?;
            let Some(rest) = e.name().strip_prefix(prefix).map(str::to_owned) else {
                continue;
            };
            if e.is_dir() || rest.trim_end_matches('/').is_empty() {
                continue;
            }
            let rel = safe_rel(&rest).ok_or_else(|| bad(format!("Недопустимый путь: {rest}")))?;
            let dest = game_dir.join(rel);
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&dest)?);
            total += std::io::copy(&mut e.by_ref().take(MAX_OVERRIDES - total + 1), &mut out)?;
            std::io::Write::flush(&mut out)?;
            if total > MAX_OVERRIDES {
                return Err(bad("Сборка слишком большая"));
            }
        }
    }
    Ok(total)
}

/// Saves an uploaded pack under `imports/` and returns its token.
pub fn stage(data_dir: &Path, bytes: &[u8]) -> Result<String, CoreError> {
    if bytes.len() > MAX_PACK || !bytes.starts_with(b"PK\x03\x04") {
        return Err(bad("Это не сборка Modrinth (.mrpack)"));
    }
    let token = uuid::Uuid::new_v4().to_string();
    crate::game::download::write_atomic(&staged(data_dir, &token)?, bytes)?;
    Ok(token)
}

/// Copies a pack from disk under `imports/` without loading it into memory.
pub fn stage_file(data_dir: &Path, src: &Path) -> Result<String, CoreError> {
    let mut f = std::fs::File::open(src)?;
    let len = f.metadata()?.len();
    let mut magic = [0u8; 4];
    if len > MAX_PACK_FILE || f.read_exact(&mut magic).is_err() || &magic != b"PK\x03\x04" {
        return Err(bad("Это не сборка Modrinth (.mrpack)"));
    }
    // A pack picked from disk is read in place: copying a 9 GB pack doubled disk use
    // and wrote gigabytes for nothing. Only a small pointer file goes under `imports/`.
    let (token, dest) = new_staged(data_dir)?;
    let abs = std::fs::canonicalize(src)?;
    std::fs::write(dest.with_extension("ref"), abs.to_string_lossy().as_bytes())?;
    Ok(token)
}

/// Forgets a staged pack. Deletes only what lives under `imports/`, never the user's own file.
pub fn release(data_dir: &Path, token: &str) {
    if let Ok(id) = uuid::Uuid::parse_str(token) {
        let base = data_dir.join("imports").join(id.to_string());
        let _ = std::fs::remove_file(base.with_extension("mrpack"));
        let _ = std::fs::remove_file(base.with_extension("ref"));
    }
}

/// A fresh token and the path its pack will live at.
pub fn new_staged(data_dir: &Path) -> Result<(String, PathBuf), CoreError> {
    let token = uuid::Uuid::new_v4().to_string();
    let path = staged(data_dir, &token)?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    Ok((token, path))
}

pub fn staged(data_dir: &Path, token: &str) -> Result<PathBuf, CoreError> {
    let id = uuid::Uuid::parse_str(token).map_err(|_| bad("bad import token"))?;
    let base = data_dir.join("imports").join(id.to_string());
    if let Ok(src) = std::fs::read_to_string(base.with_extension("ref")) {
        return Ok(PathBuf::from(src));
    }
    Ok(base.with_extension("mrpack"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    const SHA: &str = "ae3d1af44b32754b21ad4776495ff001e4339487";

    fn pack(
        index: &serde_json::Value,
        extra: &[(&str, &[u8])],
    ) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        w.start_file(INDEX, opts).unwrap();
        w.write_all(index.to_string().as_bytes()).unwrap();
        for (name, data) in extra {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        let buf = w.finish().unwrap().into_inner();
        zip::ZipArchive::new(Cursor::new(buf)).unwrap()
    }

    fn index(files: serde_json::Value, deps: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "formatVersion": 1, "game": "minecraft", "name": "Test Pack",
            "files": files, "dependencies": deps
        })
    }

    fn file(path: &str, url: &str) -> serde_json::Value {
        serde_json::json!({ "path": path, "hashes": { "sha1": SHA },
                            "downloads": [url], "fileSize": 10 })
    }

    #[test]
    fn paths_stay_inside() {
        for bad in ["../x", "/etc/x", "a/../../x", "C:\\x", "a\\b", "", "./"] {
            assert!(safe_rel(bad).is_none(), "{bad}");
        }
        assert_eq!(safe_rel("mods/a.jar").unwrap(), PathBuf::from("mods/a.jar"));
        assert_eq!(
            safe_rel("./config/x.toml").unwrap(),
            PathBuf::from("config/x.toml")
        );
    }

    #[test]
    fn only_spec_hosts_over_https() {
        assert!(allowed("https://cdn.modrinth.com/data/a/b.jar"));
        assert!(allowed("https://github.com/x/y/releases/download/1/a.jar"));
        assert!(!allowed("http://cdn.modrinth.com/a.jar"));
        assert!(!allowed("https://cdn.modrinth.com.evil.io/a.jar"));
        assert!(!allowed("https://example.com/a.jar"));
    }

    #[test]
    fn plan_reads_loader_and_files() {
        let d = Path::new("/game");
        let mut z = pack(
            &index(
                serde_json::json!([
                    file("mods/sodium.jar", "https://cdn.modrinth.com/s.jar"),
                    { "path": "mods/server.jar", "hashes": { "sha1": SHA },
                      "env": { "client": "unsupported", "server": "required" },
                      "downloads": ["https://cdn.modrinth.com/x.jar"] }
                ]),
                serde_json::json!({ "minecraft": "1.21.4", "fabric-loader": "0.16.10" }),
            ),
            &[],
        );
        let p = plan(read_index(&mut z).unwrap(), d).unwrap();
        assert_eq!((p.mc.as_str(), p.loader.as_str()), ("1.21.4", "fabric"));
        assert_eq!(p.loader_version, "0.16.10");
        assert_eq!(p.name, "Test Pack");
        assert_eq!(p.tasks.len(), 1);
        assert_eq!(p.tasks[0].path, d.join("mods/sodium.jar"));
    }

    #[test]
    fn quilt_pack_wins_over_its_fabric_pin() {
        let deps: BTreeMap<String, String> = [
            ("minecraft", "1.21.1"),
            ("fabric-loader", "0.16.10"),
            ("quilt-loader", "0.29.2"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        assert_eq!(loader_of(&deps).unwrap(), ("quilt".into(), "0.29.2".into()));
    }

    #[test]
    fn plan_rejects_unsafe_packs() {
        let deps = serde_json::json!({ "minecraft": "1.21.4" });
        let cases = [
            index(
                serde_json::json!([file("../evil.jar", "https://cdn.modrinth.com/a")]),
                deps.clone(),
            ),
            index(
                serde_json::json!([file("mods/a.jar", "https://evil.io/a")]),
                deps.clone(),
            ),
            index(serde_json::json!([]), serde_json::json!({})),
        ];
        for c in cases {
            let mut z = pack(&c, &[]);
            assert!(
                plan(read_index(&mut z).unwrap(), Path::new("/g")).is_err(),
                "{c}"
            );
        }
        let mut no_hash = index(
            serde_json::json!([file("mods/a.jar", "https://cdn.modrinth.com/a")]),
            deps,
        );
        no_hash["files"][0]["hashes"] = serde_json::json!({});
        let mut z = pack(&no_hash, &[]);
        assert!(plan(read_index(&mut z).unwrap(), Path::new("/g")).is_err());
    }

    #[test]
    fn overrides_extract_and_client_wins() {
        let d = std::env::temp_dir().join(format!("fl-mrpack-{}", uuid::Uuid::new_v4()));
        let idx = index(
            serde_json::json!([]),
            serde_json::json!({ "minecraft": "1.21.4" }),
        );
        let mut z = pack(
            &idx,
            &[
                ("overrides/config/a.toml", b"base"),
                ("overrides/options.txt", b"o"),
                ("client-overrides/config/a.toml", b"client"),
                ("other/ignored.txt", b"x"),
            ],
        );
        assert_eq!(extract_overrides(&mut z, &d).unwrap(), 11);
        assert_eq!(std::fs::read(d.join("config/a.toml")).unwrap(), b"client");
        assert!(d.join("options.txt").exists());
        assert!(!d.join("ignored.txt").exists() && !d.join("other").exists());
        std::fs::remove_dir_all(&d).unwrap();

        let mut evil = pack(&idx, &[("overrides/../../evil.txt", b"x")]);
        assert!(extract_overrides(&mut evil, &d).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn staging_checks_magic_and_token() {
        let d = std::env::temp_dir().join(format!("fl-stage-{}", uuid::Uuid::new_v4()));
        assert!(stage(&d, b"not a zip").is_err());
        let t = stage(&d, b"PK\x03\x04rest").unwrap();
        assert!(staged(&d, &t).unwrap().exists());
        assert!(staged(&d, "../../x").is_err());
        let src = d.join("big.mrpack");
        std::fs::write(&src, b"PK\x03\x04more").unwrap();
        let t2 = stage_file(&d, &src).unwrap();
        assert_eq!(std::fs::read(staged(&d, &t2).unwrap()).unwrap(), b"PK\x03\x04more");
        release(&d, &t2);
        assert!(src.exists(), "the user's own pack must survive release");
        assert!(!d.join("imports").join(format!("{t2}.ref")).exists());
        release(&d, &t);
        assert!(!staged(&d, &t).unwrap().exists());
        std::fs::write(&src, b"nope").unwrap();
        assert!(stage_file(&d, &src).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
