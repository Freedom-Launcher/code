//! Server cores («Ядро»): which server software a server runs and for which
//! Minecraft version, picked from the official sources and installed into the
//! server folder together with a start script.
//!
//! Sources: Mojang's version manifest (Vanilla), PaperMC Fill v3 (Paper),
//! the Purpur API v2, Fabric meta (the server launcher jar) and the Forge and
//! NeoForge Maven repositories (their installer runs with `--installServer`).
//! Every jar is checked against the hash its source publishes, when it
//! publishes one. Java comes from the launcher's own Java manager.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{atomic::Ordering, Arc},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Digest;

use super::{args, bad, clean_name, denied, find, load, update_props, Host, Server, FILE, PROPS};
use crate::{
    actions::ActionRegistry,
    game::{download::Progress, forge, java, javas, loader, service, version::JavaVersion},
    store, unix_now, versions, Core, CoreError, Permission as P,
};

const PAPER: &str = "https://fill.papermc.io/v3/projects/paper";
const PURPUR: &str = "https://api.purpurmc.org/v2/purpur";
const FABRIC: &str = "https://meta.fabricmc.net/v2";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge/";
const FORGE_PROMOS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEO_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge/";

/// Folder (under the data dir) for servers the launcher creates itself.
pub const MANAGED: &str = "servers";
/// Marks a start script the launcher wrote (and may rewrite).
pub const MARK: &str = "Freedom Launcher";
#[cfg(windows)]
pub const SCRIPT: &str = "start.bat";
#[cfg(not(windows))]
pub const SCRIPT: &str = "start.sh";

pub const MEMORY_MIN: u32 = 512;
pub const MEMORY_MAX: u32 = 131_072;
pub const MEMORY_DEFAULT: u32 = 2048;

/// `(id, name, description, what it adds: "plugins" | "mods" | "")`.
pub const CORES: &[(&str, &str, &str, &str)] = &[
    ("vanilla", "Vanilla", "Официальный сервер Mojang, без плагинов и модов.", ""),
    ("paper", "Paper", "Быстрый сервер с плагинами Bukkit и Paper. Подходит почти всем.", "plugins"),
    ("purpur", "Purpur", "Paper с сотнями дополнительных настроек игры.", "plugins"),
    ("fabric", "Fabric", "Лёгкие моды Fabric, быстро выходит на новых версиях.", "mods"),
    ("forge", "Forge", "Классические моды и большие сборки.", "mods"),
    ("neoforge", "NeoForge", "Моды NeoForge, продолжение Forge на новых версиях.", "mods"),
];

/// The core the launcher installed into a server folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub core: String,
    pub mc: String,
    #[serde(default)]
    pub build: String,
    /// Arguments after Java and memory: `["-jar", "paper.jar"]` or `["@libraries/…/win_args.txt"]`.
    pub launch: Vec<String>,
    #[serde(default)]
    pub java_major: u32,
    pub memory_mb: u32,
    #[serde(default)]
    pub installed_at: u64,
}

impl Installed {
    pub fn main_jar(&self) -> Option<&str> {
        match self.launch.as_slice() {
            [flag, jar, ..] if flag == "-jar" => Some(jar),
            _ => None,
        }
    }
}

fn label(core: &str) -> Option<&'static str> {
    CORES.iter().find(|c| c.0 == core).map(|c| c.1)
}

fn kind(core: &str) -> Result<&'static str, CoreError> {
    CORES
        .iter()
        .find(|c| c.0 == core)
        .map(|c| c.0)
        .ok_or_else(|| bad(format!("нет такого ядра: {core}")))
}

/// What the server list shows for a core the launcher installed.
pub fn software_of(s: &Server) -> Option<Value> {
    let i = s.core.as_ref()?;
    Some(json!({
        "kind": label(&i.core).unwrap_or(&i.core),
        "core": i.core,
        "version": i.mc,
        "build": i.build,
        "jar": i.main_jar(),
    }))
}

/// A server folder the launcher created itself (under `<data>/servers`).
pub fn managed(c: &Core, s: &Server) -> bool {
    let (Ok(base), Ok(d)) = (c.path(MANAGED).canonicalize(), s.dir().canonicalize()) else {
        return false;
    };
    d.starts_with(&base) && d != base
}

// ---------- validation ----------

/// A version or build id: it ends up in URLs and file names.
pub fn token(s: &str, what: &str) -> Result<String, CoreError> {
    let s = s.trim();
    let ok = !s.is_empty()
        && s.len() <= 64
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c));
    if !ok {
        return Err(bad(format!("{what}: непонятная версия «{s}»")));
    }
    Ok(s.to_owned())
}

pub(super) fn memory(m: Option<u32>) -> Result<u32, CoreError> {
    let m = m.unwrap_or(MEMORY_DEFAULT);
    if !(MEMORY_MIN..=MEMORY_MAX).contains(&m) {
        return Err(bad(format!("память: от {MEMORY_MIN} до {MEMORY_MAX} МБ")));
    }
    Ok(m)
}

/// Progress key of an install the UI polls with `servers.progress`.
fn job_key(job: Option<&str>) -> Result<String, CoreError> {
    let job = match job {
        Some(j) => j.to_owned(),
        None => uuid::Uuid::new_v4().simple().to_string(),
    };
    if job.is_empty() || job.len() > 64 || !job.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(bad("job: латиница, цифры, - и _, до 64 символов"));
    }
    Ok(format!("server-job:{job}"))
}

fn safe_file(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 128
        && !n.starts_with('.')
        && n.to_ascii_lowercase().ends_with(".jar")
        && n.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

fn safe_arg(a: &str) -> bool {
    !a.is_empty() && !a.contains("..") && a.chars().all(|c| c.is_ascii_alphanumeric() || "._+-/@".contains(c))
}

/// Sort key of a version: `1.21.4` → `[1, 21, 4]`, `26.1` → `[26, 1]`.
pub fn ver_key(v: &str) -> Vec<u64> {
    v.split(['.', '-', '+', '_'])
        .map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0))
        .collect()
}

/// A folder name from the server name: `Ванилла с друзьями` → `vanilla-s-druzyami`.
pub fn slug(name: &str) -> String {
    const RU: [&str; 33] = [
        "a", "b", "v", "g", "d", "e", "zh", "z", "i", "y", "k", "l", "m", "n", "o", "p", "r", "s", "t", "u",
        "f", "h", "ts", "ch", "sh", "sch", "", "y", "", "e", "yu", "ya", "e",
    ];
    let mut out = String::new();
    for ch in name.to_lowercase().chars() {
        let piece: String = if ch.is_ascii_alphanumeric() {
            ch.to_string()
        } else if ('а'..='я').contains(&ch) {
            RU[(ch as u32 - 'а' as u32) as usize].to_owned()
        } else if ch == 'ё' {
            RU[32].to_owned()
        } else {
            "-".to_owned()
        };
        for c in piece.chars() {
            if c == '-' && (out.is_empty() || out.ends_with('-')) {
                continue;
            }
            out.push(c);
        }
    }
    let mut out: String = out.trim_matches('-').chars().take(40).collect();
    out = out.trim_end_matches('-').to_owned();
    const RESERVED: [&str; 6] = ["con", "prn", "aux", "nul", "com1", "lpt1"];
    if out.is_empty() {
        out = "server".into();
    } else if RESERVED.contains(&out.as_str()) {
        out.push_str("-server");
    }
    out
}

fn unique_dir(base: &Path, slug: &str) -> PathBuf {
    let mut d = base.join(slug);
    let mut n = 2;
    while d.exists() {
        d = base.join(format!("{slug}-{n}"));
        n += 1;
    }
    d
}

// ---------- sources ----------

async fn get(c: &Core, url: &str) -> Result<Vec<u8>, CoreError> {
    Ok(c.http().get(url).send().await?.error_for_status()?.bytes().await?.to_vec())
}

async fn get_json(c: &Core, url: &str) -> Result<Value, CoreError> {
    Ok(serde_json::from_slice(&get(c, url).await?)?)
}

fn strs(v: &Value) -> impl DoubleEndedIterator<Item = &str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str)
}

fn sort_desc(v: &mut Vec<(String, bool)>) {
    v.sort_by(|a, b| ver_key(&b.0).cmp(&ver_key(&a.0)));
    v.dedup_by(|a, b| a.0 == b.0);
}

/// Minecraft versions a core exists for, newest first: `[{id, stable}]`.
/// Without `all`, only releases.
pub async fn mc_versions(c: &Core, core: &str, all: bool) -> Result<Vec<Value>, CoreError> {
    let mut out: Vec<(String, bool)> = match core {
        "vanilla" => {
            let m = versions::manifest(c).await?;
            m["versions"]
                .as_array()
                .into_iter()
                .flatten()
                // The oldest releases have no server jar on Mojang's servers.
                .filter(|v| v["releaseTime"].as_str().is_some_and(|t| t >= "2012-03"))
                .filter_map(|v| {
                    let (id, t) = (v["id"].as_str()?, v["type"].as_str()?);
                    (t == "release" || t == "snapshot").then(|| (id.to_owned(), t == "release"))
                })
                .collect()
        }
        "paper" => {
            let v = get_json(c, &format!("{PAPER}/versions")).await?;
            v["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|e| e["builds"].as_array().is_some_and(|b| !b.is_empty()))
                .filter_map(|e| e["version"]["id"].as_str())
                .map(|id| (id.to_owned(), !id.contains('-')))
                .collect()
        }
        "purpur" => {
            let v = get_json(c, PURPUR).await?;
            strs(&v["versions"]).rev().map(|id| (id.to_owned(), true)).collect()
        }
        "fabric" => {
            let v = get_json(c, &format!("{FABRIC}/versions/game")).await?;
            v.as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| Some((e["version"].as_str()?.to_owned(), e["stable"].as_bool().unwrap_or(false))))
                .collect()
        }
        "forge" => {
            let v = get_json(c, FORGE_PROMOS).await?;
            let mut out: Vec<(String, bool)> = v["promos"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(k, _)| k.rsplit_once('-').map(|(mc, _)| (mc.to_owned(), true)))
                .collect();
            sort_desc(&mut out);
            out
        }
        "neoforge" => {
            let xml = String::from_utf8_lossy(&get(c, &format!("{NEO_MAVEN}maven-metadata.xml")).await?).into_owned();
            let mut map: std::collections::BTreeMap<String, bool> = Default::default();
            for v in xml.split("<version>").skip(1).filter_map(|s| s.split('<').next()) {
                if let Some(mc) = super::neoforge_mc(v) {
                    *map.entry(mc).or_default() |= !v.contains('-');
                }
            }
            let mut out: Vec<(String, bool)> = map.into_iter().collect();
            sort_desc(&mut out);
            out
        }
        _ => return Err(kind(core).unwrap_err()),
    };
    out.retain(|(id, stable)| (all || *stable) && token(id, "").is_ok());
    Ok(out.into_iter().map(|(id, stable)| json!({ "id": id, "stable": stable })).collect())
}

/// Builds of a core for one Minecraft version, newest first:
/// `{builds: [{id, stable}], recommended}`. Vanilla has none.
pub async fn builds(c: &Core, core: &str, mc: &str) -> Result<Value, CoreError> {
    let missing = || bad(format!("{} для Minecraft {mc} не найден", label(core).unwrap_or(core)));
    let mut recommended = None;
    let mut out: Vec<(String, bool)> = match core {
        "vanilla" => Vec::new(),
        "paper" => {
            let v = get_json(c, &format!("{PAPER}/versions/{mc}/builds")).await.map_err(|_| missing())?;
            v.as_array()
                .into_iter()
                .flatten()
                .filter_map(|b| Some((b["id"].as_u64()?.to_string(), b["channel"] == "STABLE")))
                .collect()
        }
        "purpur" => {
            let v = get_json(c, &format!("{PURPUR}/{mc}")).await.map_err(|_| missing())?;
            recommended = v["builds"]["latest"].as_str().map(str::to_owned);
            strs(&v["builds"]["all"]).rev().map(|b| (b.to_owned(), true)).collect()
        }
        "fabric" => loader::versions(c, "fabric", mc)
            .await
            .map_err(|_| missing())?
            .iter()
            .filter_map(|e| Some((e["version"].as_str()?.to_owned(), e["stable"].as_bool().unwrap_or(false))))
            .collect(),
        "forge" => {
            let promos = get_json(c, FORGE_PROMOS).await?;
            recommended = ["recommended", "latest"]
                .iter()
                .find_map(|k| promos["promos"][format!("{mc}-{k}")].as_str().map(str::to_owned));
            let mut v: Vec<(String, bool)> =
                forge::versions(c, "forge", mc).await?.into_iter().map(|b| (b, true)).collect();
            sort_desc(&mut v);
            v
        }
        "neoforge" => {
            let mut v: Vec<(String, bool)> = forge::versions(c, "neoforge", mc)
                .await?
                .into_iter()
                .map(|b| {
                    let st = !b.contains('-');
                    (b, st)
                })
                .collect();
            sort_desc(&mut v);
            v
        }
        _ => return Err(kind(core).unwrap_err()),
    };
    out.retain(|(b, _)| token(b, "").is_ok());
    out.truncate(80);
    if core != "vanilla" && out.is_empty() {
        return Err(missing());
    }
    let recommended = recommended
        .filter(|r| out.iter().any(|b| &b.0 == r))
        .or_else(|| out.iter().find(|b| b.1).or(out.first()).map(|b| b.0.clone()));
    Ok(json!({
        "builds": out.iter().map(|(id, stable)| json!({ "id": id, "stable": stable })).collect::<Vec<_>>(),
        "recommended": recommended,
    }))
}

// ---------- download ----------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Hash {
    None,
    Sha1(String),
    Sha256(String),
    Md5(String),
}

impl Hash {
    fn want(&self) -> Option<&str> {
        match self {
            Hash::None => None,
            Hash::Sha1(h) | Hash::Sha256(h) | Hash::Md5(h) => Some(h),
        }
    }
}

enum Hasher {
    None,
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Md5(md5::Context),
}

impl Hasher {
    fn new(h: &Hash) -> Self {
        match h {
            Hash::None => Hasher::None,
            Hash::Sha1(_) => Hasher::Sha1(sha1::Sha1::new()),
            Hash::Sha256(_) => Hasher::Sha256(sha2::Sha256::new()),
            Hash::Md5(_) => Hasher::Md5(md5::Context::new()),
        }
    }
    fn update(&mut self, b: &[u8]) {
        match self {
            Hasher::None => {}
            Hasher::Sha1(h) => Digest::update(h, b),
            Hasher::Sha256(h) => Digest::update(h, b),
            Hasher::Md5(h) => h.consume(b),
        }
    }
    fn hex(self) -> Option<String> {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        match self {
            Hasher::None => None,
            Hasher::Sha1(h) => Some(hex(&h.finalize())),
            Hasher::Sha256(h) => Some(hex(&h.finalize())),
            Hasher::Md5(h) => Some(hex(&h.compute().0)),
        }
    }
}

#[derive(Debug, Clone)]
struct Dl {
    url: String,
    /// File name in the server folder.
    name: String,
    hash: Hash,
    size: u64,
}

fn file_matches(path: &Path, hash: &Hash) -> bool {
    let Some(want) = hash.want() else { return false };
    let Ok(mut f) = fs::File::open(path) else { return false };
    let mut h = Hasher::new(hash);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    h.hex().is_some_and(|got| got.eq_ignore_ascii_case(want))
}

async fn download(c: &Core, f: &Dl, dest: &Path, p: &Progress) -> Result<(), CoreError> {
    if file_matches(dest, &f.hash) {
        return Ok(());
    }
    let mut last = None;
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(700 * attempt)).await;
        }
        match try_download(c, f, dest, p).await {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| CoreError::msg("не удалось скачать")))
}

async fn try_download(c: &Core, f: &Dl, dest: &Path, p: &Progress) -> Result<(), CoreError> {
    let mut resp = c.http().get(&f.url).send().await?.error_for_status()?;
    let len = resp.content_length().unwrap_or(f.size);
    p.total_bytes.fetch_add(len, Ordering::Relaxed);
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let mut got = 0u64;
    let res = async {
        let mut file = fs::File::create(&part)?;
        let mut h = Hasher::new(&f.hash);
        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk)?;
            h.update(&chunk);
            got += chunk.len() as u64;
            p.done_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
        file.flush()?;
        drop(file);
        if let (Some(want), Some(have)) = (f.hash.want(), h.hex()) {
            if !have.eq_ignore_ascii_case(want) {
                return Err(CoreError::msg(format!("{}: файл повреждён (контрольная сумма не сошлась)", f.name)));
            }
        }
        fs::rename(&part, dest)?;
        Ok(())
    }
    .await;
    if res.is_err() {
        let _ = fs::remove_file(&part);
        p.done_bytes.fetch_sub(got, Ordering::Relaxed);
        p.total_bytes.fetch_sub(len, Ordering::Relaxed);
    }
    res
}

// ---------- plans ----------

/// What to download for a core, and how it starts.
#[derive(Debug)]
struct Plan {
    build: String,
    files: Vec<Dl>,
    /// Forge / NeoForge installer jar (one of `files`) to run with `--installServer`.
    installer: Option<String>,
    /// The jar started with `-jar` (when there is no installer).
    main: Option<String>,
}

async fn vanilla_jar(c: &Core, mc: &str, name: &str) -> Result<Dl, CoreError> {
    let (url, _) = versions::locate(c, mc).await.map_err(|_| bad(format!("Minecraft {mc} не найден")))?;
    let v = get_json(c, &url).await?;
    let s = &v["downloads"]["server"];
    let url = s["url"].as_str().ok_or_else(|| bad(format!("у Minecraft {mc} нет серверного jar")))?;
    Ok(Dl {
        url: url.to_owned(),
        name: name.to_owned(),
        hash: s["sha1"].as_str().map_or(Hash::None, |h| Hash::Sha1(h.to_owned())),
        size: s["size"].as_u64().unwrap_or(0),
    })
}

async fn maven_sha1(c: &Core, url: &str) -> Hash {
    match get(c, &format!("{url}.sha1")).await {
        Ok(b) => {
            let s = String::from_utf8_lossy(&b).trim().to_ascii_lowercase();
            if s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                Hash::Sha1(s)
            } else {
                Hash::None
            }
        }
        Err(_) => Hash::None,
    }
}

async fn plan(c: &Core, core: &str, mc: &str, build: Option<&str>) -> Result<Plan, CoreError> {
    let missing = || bad(format!("{} для Minecraft {mc} не найден", label(core).unwrap_or(core)));
    let jar = |dl: Dl, build: String| Plan { build, main: Some(dl.name.clone()), files: vec![dl], installer: None };
    Ok(match core {
        "vanilla" => jar(vanilla_jar(c, mc, &format!("minecraft_server.{mc}.jar")).await?, String::new()),
        "paper" => {
            let list = get_json(c, &format!("{PAPER}/versions/{mc}/builds")).await.map_err(|_| missing())?;
            let arr = list.as_array().ok_or_else(missing)?;
            let b = match build {
                Some(want) => arr.iter().find(|b| b["id"].as_u64().map(|i| i.to_string()).as_deref() == Some(want)),
                None => arr.iter().find(|b| b["channel"] == "STABLE").or(arr.first()),
            }
            .ok_or_else(|| bad(format!("сборка Paper {} не найдена", build.unwrap_or(""))))?;
            let d = &b["downloads"]["server:default"];
            let name = d["name"].as_str().filter(|n| safe_file(n)).ok_or_else(missing)?;
            let dl = Dl {
                url: d["url"].as_str().ok_or_else(missing)?.to_owned(),
                name: name.to_owned(),
                hash: d["checksums"]["sha256"].as_str().map_or(Hash::None, |h| Hash::Sha256(h.to_owned())),
                size: d["size"].as_u64().unwrap_or(0),
            };
            jar(dl, b["id"].as_u64().unwrap_or(0).to_string())
        }
        "purpur" => {
            let b = match build {
                Some(b) => b.to_owned(),
                None => {
                    let v = get_json(c, &format!("{PURPUR}/{mc}")).await.map_err(|_| missing())?;
                    v["builds"]["latest"].as_str().ok_or_else(missing)?.to_owned()
                }
            };
            let b = token(&b, "сборка")?;
            let meta = get_json(c, &format!("{PURPUR}/{mc}/{b}")).await.map_err(|_| missing())?;
            let dl = Dl {
                url: format!("{PURPUR}/{mc}/{b}/download"),
                name: format!("purpur-{mc}-{b}.jar"),
                hash: meta["md5"].as_str().map_or(Hash::None, |h| Hash::Md5(h.to_owned())),
                size: 0,
            };
            jar(dl, b)
        }
        "fabric" => {
            let l = match build {
                Some(b) => b.to_owned(),
                None => loader::fabric_latest(c, mc).await.map_err(|_| missing())?,
            };
            let l = token(&l, "загрузчик")?;
            let inst = get_json(c, &format!("{FABRIC}/versions/installer")).await?;
            let arr = inst.as_array().ok_or_else(missing)?;
            let i = arr
                .iter()
                .find(|e| e["stable"] == true)
                .or(arr.first())
                .and_then(|e| e["version"].as_str())
                .ok_or_else(missing)?;
            let i = token(i, "установщик")?;
            let launcher = Dl {
                url: format!("{FABRIC}/versions/loader/{mc}/{l}/{i}/server/jar"),
                name: format!("fabric-server-mc.{mc}-loader.{l}-launcher.{i}.jar"),
                hash: Hash::None,
                size: 0,
            };
            // The launcher starts the vanilla `server.jar` next to it.
            let vanilla = vanilla_jar(c, mc, "server.jar").await?;
            Plan { build: l, main: Some(launcher.name.clone()), files: vec![launcher, vanilla], installer: None }
        }
        "forge" | "neoforge" => {
            let b = match build {
                Some(b) => b.to_owned(),
                None => builds(c, core, mc).await?["recommended"].as_str().ok_or_else(missing)?.to_owned(),
            };
            let b = token(&b, "сборка")?;
            let (url, name) = if core == "forge" {
                let v = format!("{mc}-{b}");
                (format!("{FORGE_MAVEN}{v}/forge-{v}-installer.jar"), format!("forge-{v}-installer.jar"))
            } else {
                (format!("{NEO_MAVEN}{b}/neoforge-{b}-installer.jar"), format!("neoforge-{b}-installer.jar"))
            };
            let hash = maven_sha1(c, &url).await;
            Plan {
                build: b,
                installer: Some(name.clone()),
                main: None,
                files: vec![Dl { url, name, hash, size: 0 }],
            }
        }
        _ => return Err(kind(core).unwrap_err()),
    })
}

// ---------- install ----------

/// The console `java` next to the executable the Java manager returns
/// (`javaw.exe` on Windows has no console for the server log).
fn console_java(exe: PathBuf) -> PathBuf {
    if exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe")) {
        let j = exe.with_file_name("java.exe");
        if j.exists() {
            return j;
        }
    }
    exe
}

pub(super) async fn java_for(c: &Core, mc: &str, p: Arc<Progress>) -> Result<(u32, PathBuf), CoreError> {
    let major = javas::required_major(mc);
    let jv = JavaVersion { component: javas::component_for_major(major).into(), major_version: major };
    Ok((major, console_java(java::ensure(c, Some(&jv), p).await?)))
}

async fn run_installer(java: &Path, root: &Path, jar: &str) -> Result<(), CoreError> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-jar")
        .arg(jar)
        .arg("--installServer")
        .arg(root)
        .current_dir(root)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().await?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let tail: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).rev().take(4).collect();
    Err(CoreError::msg(format!(
        "установщик {jar} завершился с ошибкой: {}",
        tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
    )))
}

/// How a Forge / NeoForge server starts after `--installServer`: the args file
/// of modern versions, else the runnable jar (old Forge, Forge's shim jar).
pub fn forge_launch(root: &Path, core: &str, mc: &str, build: &str) -> Option<Vec<String>> {
    let os = if cfg!(windows) { "win" } else { "unix" };
    let lib = if core == "forge" {
        format!("libraries/net/minecraftforge/forge/{mc}-{build}")
    } else {
        format!("libraries/net/neoforged/neoforge/{build}")
    };
    let args = format!("{lib}/{os}_args.txt");
    if root.join(&args).is_file() {
        return Some(vec![format!("@{args}")]);
    }
    let pre = if core == "forge" { "forge-" } else { "neoforge-" };
    let mut jars: Vec<String> = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(pre) && n.ends_with(".jar") && !n.contains("installer") && safe_file(n))
        .collect();
    jars.sort_by_key(|n| (!n.contains("shim"), !n.contains(build), n.len()));
    jars.into_iter().next().map(|j| vec!["-jar".into(), j])
}

/// The start script text. Java and the arguments come from the launcher only.
pub fn script_text(java: &Path, memory_mb: u32, launch: &[String], windows: bool) -> String {
    let xms = memory_mb.min(1024);
    let args = launch.join(" ");
    if windows {
        let j = java.to_string_lossy().replace('%', "%%");
        format!(
            "@echo off\r\nrem {MARK}: start script of this server, rewritten when its core or memory changes.\r\n\
             cd /d \"%~dp0\"\r\n\"{j}\" -Xms{xms}M -Xmx{memory_mb}M {args} nogui\r\npause\r\n"
        )
    } else {
        let j = java.to_string_lossy().replace('\'', "'\\''");
        format!(
            "#!/bin/sh\n# {MARK}: start script of this server, rewritten when its core or memory changes.\n\
             cd \"$(dirname \"$0\")\" || exit 1\nexec '{j}' -Xms{xms}M -Xmx{memory_mb}M {args} nogui \"$@\"\n"
        )
    }
}

/// Writes the start script unless the user has their own under that name.
fn write_script(root: &Path, java: &Path, memory_mb: u32, launch: &[String]) -> Result<Option<&'static str>, CoreError> {
    if !launch.iter().all(|a| safe_arg(a)) {
        return Err(CoreError::msg("непонятные аргументы запуска"));
    }
    let p = root.join(SCRIPT);
    if let Ok(old) = fs::read_to_string(&p) {
        if !old.contains(MARK) {
            return Ok(None);
        }
    }
    fs::write(&p, script_text(java, memory_mb, launch, cfg!(windows)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755))?;
    }
    Ok(Some(SCRIPT))
}

/// Downloads and installs a core into `root`; returns it and the files it
/// put there (so a core change knows what not to clean up).
async fn install(
    c: &Core,
    root: &Path,
    core: &str,
    mc: &str,
    build: Option<&str>,
    memory_mb: u32,
    p: &Arc<Progress>,
) -> Result<(Installed, Vec<String>), CoreError> {
    p.set_stage("meta");
    let plan = plan(c, core, mc, build).await?;
    p.set_stage("java");
    let (major, java) = java_for(c, mc, Arc::clone(p)).await?;
    p.set_stage("download");
    p.total_files.fetch_add(plan.files.len() as u64, Ordering::Relaxed);
    for f in &plan.files {
        if !safe_file(&f.name) {
            return Err(CoreError::msg(format!("странное имя файла: {}", f.name)));
        }
        download(c, f, &root.join(&f.name), p).await?;
        p.done_files.fetch_add(1, Ordering::Relaxed);
    }
    let launch = match (&plan.installer, &plan.main) {
        (Some(inst), _) => {
            p.set_stage("install");
            run_installer(&java, root, inst).await?;
            let _ = fs::remove_file(root.join(inst));
            let _ = fs::remove_file(root.join(format!("{inst}.log")));
            forge_launch(root, core, mc, &plan.build)
                .ok_or_else(|| CoreError::msg("установщик отработал, но не оставил, что запускать"))?
        }
        (None, Some(main)) => vec!["-jar".into(), main.clone()],
        (None, None) => return Err(CoreError::msg("нечего запускать")),
    };
    p.set_stage("finish");
    write_script(root, &java, memory_mb, &launch)?;
    let files = plan.files.iter().filter(|f| Some(&f.name) != plan.installer.as_ref()).map(|f| f.name.clone()).collect();
    Ok((
        Installed {
            core: core.to_owned(),
            mc: mc.to_owned(),
            build: plan.build,
            launch,
            java_major: major,
            memory_mb,
            installed_at: unix_now(),
        },
        files,
    ))
}

const EULA: &str = "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\neula=true\n";

#[derive(Deserialize)]
struct CreateArgs {
    #[serde(default)]
    name: String,
    core: String,
    mc: String,
    #[serde(default)]
    build: Option<String>,
    #[serde(default)]
    memory_mb: Option<u32>,
    #[serde(default)]
    eula: bool,
    #[serde(default)]
    job: Option<String>,
    #[serde(default)]
    dir: Option<String>,
    #[serde(default)]
    choose_dir: bool,
}

fn opt_token(b: Option<String>, what: &str) -> Result<Option<String>, CoreError> {
    b.filter(|b| !b.trim().is_empty()).map(|b| token(&b, what)).transpose()
}

async fn create(c: &Arc<Core>, a: CreateArgs, at: Option<PathBuf>) -> Result<Value, CoreError> {
    let core = kind(&a.core)?;
    let mc = token(&a.mc, "версия Minecraft")?;
    let build = opt_token(a.build, "сборка")?;
    let memory_mb = memory(a.memory_mb)?;
    let name = if a.name.trim().is_empty() { format!("{} {mc}", label(core).unwrap_or(core)) } else { a.name };
    let name = clean_name(&name)?;
    let key = job_key(a.job.as_deref())?;

    let (dir, made) = match at {
        None => {
            let base = c.path(MANAGED);
            fs::create_dir_all(&base)?;
            let d = unique_dir(&base, &slug(&name));
            fs::create_dir(&d)?;
            (d, true)
        }
        Some(p) => {
            if !p.is_absolute() || !p.is_dir() {
                return Err(bad("папка не найдена"));
            }
            if fs::read_dir(&p)?.next().is_none() {
                (p, false)
            } else {
                let d = unique_dir(&p, &slug(&name));
                fs::create_dir(&d)?;
                (d, true)
            }
        }
    };
    let p = service::progress_for(c, &key);
    let res = async {
        let root = super::allowed_dir(&c.data_dir, &dir)?;
        let (info, _) = install(c, &root, core, &mc, build.as_deref(), memory_mb, &p).await?;
        if a.eula {
            fs::write(root.join("eula.txt"), EULA)?;
        }
        let props = root.join(PROPS);
        if !props.exists() {
            fs::write(&props, update_props("#Minecraft server properties\n", &[("motd".into(), name.clone())]))?;
        }
        let _g = c.state_lock.lock().await;
        let mut st = load(c)?;
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        let host = Host::Local { dir: root.to_string_lossy().into_owned() };
        st.servers.push(Server { id: id.clone(), name, host, created_at: unix_now(), core: Some(info), memory_mb: None, auto_restart: false });
        store::save(&c.path(FILE), &st, false)?;
        Ok::<_, CoreError>(json!({ "id": id, "dir": root.to_string_lossy() }))
    }
    .await;
    match res {
        Ok(v) => {
            p.set_stage("done");
            Ok(v)
        }
        Err(e) => {
            p.set_stage("failed");
            if made {
                let _ = fs::remove_dir_all(&dir);
            }
            Err(e)
        }
    }
}

/// Names a server core jar typically has (so a core change cleans up only those).
fn looks_like_core(jar: &str) -> bool {
    let j = jar.to_ascii_lowercase();
    ["server", "paper", "purpur", "fabric", "forge", "neoforge", "spigot", "folia", "quilt", "craftbukkit"]
        .iter()
        .any(|k| j.contains(k))
}

async fn save_core(c: &Core, id: &str, info: Installed) -> Result<(), CoreError> {
    let _g = c.state_lock.lock().await;
    let mut st = load(c)?;
    let s = st.servers.iter_mut().find(|s| s.id == id).ok_or_else(|| bad("нет такого сервера"))?;
    s.core = Some(info);
    store::save(&c.path(FILE), &st, false)
}

pub(super) fn register(r: &mut ActionRegistry) {
    r.register("servers.cores", P::Servers, false, |_c, _a| async move {
        Ok(json!(CORES
            .iter()
            .map(|(id, name, text, adds)| json!({ "id": id, "name": name, "text": text, "adds": adds }))
            .collect::<Vec<_>>()))
    });

    r.register("servers.core.versions", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            core: String,
            #[serde(default)]
            all: bool,
        }
        let A { core, all } = args(a)?;
        let core = kind(&core)?;
        Ok(json!(mc_versions(&c, core, all).await?))
    });

    r.register("servers.core.builds", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            core: String,
            mc: String,
        }
        let A { core, mc } = args(a)?;
        let core = kind(&core)?;
        builds(&c, core, &token(&mc, "версия Minecraft")?).await
    });

    r.register("servers.progress", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            job: String,
        }
        let A { job } = args(a)?;
        Ok(service::progress(&c, &job_key(Some(&job))?))
    });

    // A new server in `<data>/servers/<name>`. A folder of the user's choice
    // goes through the native picker (`choose_dir: true` in the app shell).
    r.register("servers.create", P::Servers, false, |c, a| async move {
        let a: CreateArgs = args(a)?;
        if a.dir.is_some() {
            return Err(denied("папку для сервера выбирает только пользователь"));
        }
        if a.choose_dir {
            return Err(bad("выбор папки работает только в лаунчере"));
        }
        create(&c, a, None).await
    });

    // Built-in only: in a folder the user picked in the native dialog.
    r.register("servers.create.at", P::Internal, false, |c, a| async move {
        let a: CreateArgs = args(a)?;
        let dir = a.dir.clone().ok_or_else(|| bad("dir: нужна папка"))?;
        create(&c, a, Some(PathBuf::from(dir))).await
    });

    // Changes or updates the core of a server; the old core jar goes away only
    // once the new one is in place. Worlds, plugins, mods and settings stay.
    r.register("servers.core.install", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            core: String,
            mc: String,
            #[serde(default)]
            build: Option<String>,
            #[serde(default)]
            memory_mb: Option<u32>,
            #[serde(default)]
            job: Option<String>,
        }
        let a: A = args(a)?;
        let core = kind(&a.core)?;
        let mc = token(&a.mc, "версия Minecraft")?;
        let build = opt_token(a.build, "сборка")?;
        let s = find(&c, &a.id)?;
        if c.server_procs.alive(&a.id) {
            return Err(bad("остановите сервер перед сменой ядра"));
        }
        let root = s.root()?;
        let memory_mb = memory(a.memory_mb.or(s.core.as_ref().map(|i| i.memory_mb)).or(s.memory_mb))?;
        let old_jar = match &s.core {
            Some(i) => i.main_jar().map(str::to_owned),
            None => super::software(&root)["jar"].as_str().filter(|j| looks_like_core(j)).map(str::to_owned),
        };
        let p = service::progress_for(&c, &job_key(a.job.as_deref())?);
        let (info, files) = match install(&c, &root, core, &mc, build.as_deref(), memory_mb, &p).await {
            Ok(r) => r,
            Err(e) => {
                p.set_stage("failed");
                return Err(e);
            }
        };
        let mut removed = None;
        if let Some(j) = old_jar.filter(|j| safe_file(j) && !files.contains(j) && Some(j.as_str()) != info.main_jar()) {
            if fs::remove_file(root.join(&j)).is_ok() {
                removed = Some(j);
            }
        }
        save_core(&c, &a.id, info).await?;
        p.set_stage("done");
        Ok(json!({ "ok": true, "removed": removed }))
    });

    r.register("servers.memory.set", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            memory_mb: u32,
        }
        let A { id, memory_mb } = args(a)?;
        let memory_mb = memory(Some(memory_mb))?;
        let s = find(&c, &id)?;
        let root = s.root()?;
        let Some(mut info) = s.core.clone() else {
            // A folder added as it was: the launcher starts it with this memory.
            let _g = c.state_lock.lock().await;
            let mut st = load(&c)?;
            let s = st.servers.iter_mut().find(|s| s.id == id).ok_or_else(|| bad("нет такого сервера"))?;
            s.memory_mb = Some(memory_mb);
            store::save(&c.path(FILE), &st, false)?;
            return Ok(json!({ "ok": true, "script": null }));
        };
        info.memory_mb = memory_mb;
        let (_, java) = java_for(&c, &info.mc, Arc::new(Progress::default())).await?;
        let script = write_script(&root, &java, memory_mb, &info.launch)?;
        save_core(&c, &id, info).await?;
        Ok(json!({ "ok": true, "script": script }))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_tokens() {
        assert_eq!(slug("Ванилла с друзьями"), "vanilla-s-druzyami");
        assert_eq!(slug("  Paper 1.21.4!! "), "paper-1-21-4");
        assert_eq!(slug("ЁЖ"), "ezh");
        assert_eq!(slug("★★★"), "server");
        assert_eq!(slug("CON"), "con-server");
        assert!(token("1.21.4", "v").is_ok() && token("26.1-rc-1", "v").is_ok() && token("21.4.161-beta", "v").is_ok());
        assert!(token("../x", "v").is_err() && token("a b", "v").is_err() && token("", "v").is_err() && token(".1", "v").is_err());
        assert!(job_key(Some("a-1_b")).is_ok() && job_key(Some("a/b")).is_err());
        assert!(memory(Some(256)).is_err() && memory(Some(200_000)).is_err());
        assert_eq!(memory(None).unwrap(), MEMORY_DEFAULT);
    }

    #[test]
    fn version_order() {
        let mut v = vec![("1.9".to_string(), true), ("1.21.4".into(), true), ("26.1".into(), true), ("1.21".into(), true), ("1.21".into(), true)];
        sort_desc(&mut v);
        let ids: Vec<&str> = v.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(ids, ["26.1", "1.21.4", "1.21", "1.9"]);
    }

    #[test]
    fn hashes() {
        let mut h = Hasher::new(&Hash::Md5(String::new()));
        h.update(b"abc");
        assert_eq!(h.hex().unwrap(), "900150983cd24fb0d6963f7d28e17f72");
        let mut h = Hasher::new(&Hash::Sha256(String::new()));
        h.update(b"abc");
        assert_eq!(h.hex().unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let mut h = Hasher::new(&Hash::Sha1(String::new()));
        h.update(b"abc");
        assert_eq!(h.hex().unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn scripts() {
        let launch = vec!["-jar".to_string(), "paper-1.21.4-232.jar".to_string()];
        let w = script_text(Path::new(r"C:\Users\Я\AppData\java\bin\java.exe"), 4096, &launch, true);
        assert!(w.contains("\"C:\\Users\\Я\\AppData\\java\\bin\\java.exe\" -Xms1024M -Xmx4096M -jar paper-1.21.4-232.jar nogui\r\n"));
        assert!(w.contains(MARK) && w.contains("cd /d \"%~dp0\""));
        let u = script_text(Path::new("/home/o'k/java"), 768, &["@libraries/x/unix_args.txt".into()], false);
        assert!(u.starts_with("#!/bin/sh\n"));
        assert!(u.contains("exec '/home/o'\\''k/java' -Xms768M -Xmx768M @libraries/x/unix_args.txt nogui \"$@\"\n"));
        assert!(!safe_arg("a b") && !safe_arg("../x") && safe_arg("@libraries/net/a/1.0/unix_args.txt"));
        assert!(safe_file("purpur-1.21.4-2416.jar") && !safe_file("../a.jar") && !safe_file("a.bat"));
    }

    #[test]
    fn forge_layouts() {
        let d = std::env::temp_dir().join(format!("fl-forge-{}", uuid::Uuid::new_v4()));
        let os = if cfg!(windows) { "win" } else { "unix" };
        fs::create_dir_all(d.join("libraries/net/minecraftforge/forge/1.20.1-47.3.0")).unwrap();
        fs::write(d.join(format!("libraries/net/minecraftforge/forge/1.20.1-47.3.0/{os}_args.txt")), "x").unwrap();
        assert_eq!(
            forge_launch(&d, "forge", "1.20.1", "47.3.0"),
            Some(vec![format!("@libraries/net/minecraftforge/forge/1.20.1-47.3.0/{os}_args.txt")])
        );
        fs::write(d.join("forge-1.12.2-14.23.5.2860.jar"), "x").unwrap();
        fs::write(d.join("forge-1.12.2-14.23.5.2860-installer.jar"), "x").unwrap();
        assert_eq!(
            forge_launch(&d, "forge", "1.12.2", "14.23.5.2860"),
            Some(vec!["-jar".into(), "forge-1.12.2-14.23.5.2860.jar".into()])
        );
        assert_eq!(forge_launch(&d, "neoforge", "1.21.1", "21.1.77"), None);
        fs::remove_dir_all(d).unwrap();
    }
}