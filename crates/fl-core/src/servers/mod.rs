//! Servers the user manages from the launcher («Управление серверами»).
//!
//! A server is like an instance, but a server: a registry entry in
//! `<data>/servers.json` that points to where the server is hosted. Version 1
//! knows one kind of hosting, a folder on this computer (`local`); hosting
//! panels and remote machines get their own `Host` kinds later.
//!
//! Every action needs the `servers` permission («Управлять серверами»,
//! tier «Расширенный»). A plugin cannot name a folder itself: `servers.add`
//! is built-in only, and plugins reach it through the native folder picker
//! (`servers.pick` in the app shell), so a server folder is always one the
//! user chose. Inside a server folder the same rules as for instance files
//! apply: no links out, no executable code written, text files only.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    accounts,
    actions::ActionRegistry,
    addon_data::{inside, writable, FILE_MAX, LIST_MAX},
    skins, store, unix_now, Core, CoreError, Permission as P,
};

pub mod backups;
mod cores;
pub mod run;
pub use run::{any_running, stop_all, Procs};
pub use cores::{forge_launch, script_text, slug, token, ver_key, Installed, CORES, MANAGED, MARK, SCRIPT};

pub const FILE: &str = "servers.json";
pub const PROPS: &str = "server.properties";
pub const ICON: &str = "server-icon.png";
const ICON_MAX: usize = 256 * 1024;
const NAME_MAX: usize = 64;

/// Where a server runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Host {
    /// A server folder on this computer.
    Local { dir: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: Host,
    #[serde(default)]
    pub created_at: u64,
    /// The core the launcher installed (`None` for a folder added as it was).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<Installed>,
    /// Memory for a folder added as it was (an installed core keeps its own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// Start again by itself after a crash.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_restart: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct ServerStore {
    #[serde(default)]
    pub servers: Vec<Server>,
}

fn bad(m: impl Into<String>) -> CoreError {
    CoreError::BadArgs(m.into())
}
fn denied(m: impl Into<String>) -> CoreError {
    CoreError::Denied(m.into())
}

fn args<T: DeserializeOwned>(v: Value) -> Result<T, CoreError> {
    let v = if v.is_null() { json!({}) } else { v };
    serde_json::from_value(v).map_err(|e| bad(e.to_string()))
}

fn clean_name(n: &str) -> Result<String, CoreError> {
    let n = n.trim();
    if n.is_empty() || n.chars().count() > NAME_MAX || n.chars().any(char::is_control) {
        return Err(bad(format!("название: от 1 до {NAME_MAX} символов")));
    }
    Ok(n.to_owned())
}

fn load(c: &Core) -> Result<ServerStore, CoreError> {
    store::load(&c.path(FILE))
}

fn find(c: &Core, id: &str) -> Result<Server, CoreError> {
    load(c)?
        .servers
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| bad(format!("нет такого сервера: {id}")))
}

impl Server {
    pub fn dir(&self) -> PathBuf {
        match &self.host {
            Host::Local { dir } => PathBuf::from(dir),
        }
    }

    /// The server folder, if it is still there.
    pub fn root(&self) -> Result<PathBuf, CoreError> {
        let d = self.dir();
        if !d.is_dir() {
            return Err(bad("папка сервера не найдена: её переместили или удалили"));
        }
        Ok(d)
    }
}

/// A folder may hold a server unless it is a filesystem root, the user's home,
/// or overlaps the launcher's own data (accounts, settings, instances).
pub fn allowed_dir(data_dir: &Path, dir: &Path) -> Result<PathBuf, CoreError> {
    if !dir.is_absolute() {
        return Err(bad("нужен полный путь к папке"));
    }
    let d = dir.canonicalize().map_err(|_| bad("папка не найдена"))?;
    if !d.is_dir() {
        return Err(bad("это не папка"));
    }
    if d.parent().is_none() {
        return Err(denied("корень диска не может быть папкой сервера"));
    }
    let data = data_dir.canonicalize().unwrap_or_else(|_| data_dir.to_path_buf());
    // Servers the launcher creates itself live in `<data>/servers/<name>`.
    let managed = data.join(cores::MANAGED);
    let own = d.starts_with(&managed) && d != managed;
    if !own && (d.starts_with(&data) || data.starts_with(&d)) {
        return Err(denied("папка пересекается с данными лаунчера"));
    }
    for var in ["HOME", "USERPROFILE"] {
        if let Some(h) = std::env::var_os(var).and_then(|h| PathBuf::from(h).canonicalize().ok()) {
            if h == d {
                return Err(denied("домашняя папка не может быть папкой сервера"));
            }
        }
    }
    Ok(d)
}

// ---------- server.properties ----------

/// A value as Java's `Properties.load` reads it (`\uXXXX`, `\t`, `\\` …).
pub fn unescape(s: &str) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(s.len());
    let mut it = s.chars().peekable();
    let mut buf = [0u16; 2];
    while let Some(ch) = it.next() {
        if ch != '\\' {
            units.extend_from_slice(ch.encode_utf16(&mut buf));
            continue;
        }
        match it.next() {
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                match u16::from_str_radix(&hex, 16) {
                    Ok(u) if hex.len() == 4 => units.push(u),
                    _ => units.extend(format!("u{hex}").encode_utf16()),
                }
            }
            Some('t') => units.push('\t' as u16),
            Some('n') => units.push('\n' as u16),
            Some('r') => units.push('\r' as u16),
            Some('f') => units.push(0x0c),
            Some(o) => units.extend_from_slice(o.encode_utf16(&mut buf)),
            None => {}
        }
    }
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect()
}

/// A value Java reads back exactly, whatever charset the server assumes:
/// anything outside printable ASCII becomes `\uXXXX`.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        match ch {
            '\\' => out.push_str("\\\\"),
            ' ' if i == 0 => out.push_str("\\ "),
            ' '..='~' => out.push(ch),
            _ => {
                let mut b = [0u16; 2];
                for u in ch.encode_utf16(&mut b) {
                    out.push_str(&format!("\\u{u:04X}"));
                }
            }
        }
    }
    out
}

fn split_line(line: &str) -> Option<(&str, &str)> {
    let t = line.trim_start();
    if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
        return None;
    }
    let cut = t.find(['=', ':'])?;
    Some((t[..cut].trim(), t[cut + 1..].trim_start()))
}

/// `key=value` pairs in file order.
pub fn parse_props(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(split_line)
        .map(|(k, v)| (k.to_owned(), unescape(v)))
        .collect()
}

/// The file with `values` changed in place; comments and order are kept,
/// keys the file did not have are added at the end.
pub fn update_props(text: &str, values: &[(String, String)]) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut done = vec![false; values.len()];
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let hit = split_line(line).and_then(|(k, _)| values.iter().position(|(key, _)| key == k));
        match hit {
            Some(i) => {
                if !done[i] {
                    out.push(format!("{}={}", values[i].0, escape(&values[i].1)));
                    done[i] = true;
                }
            }
            None => out.push(line.to_owned()),
        }
    }
    for (i, (k, v)) in values.iter().enumerate() {
        if !done[i] {
            out.push(format!("{k}={}", escape(v)));
        }
    }
    let mut s = out.join(nl);
    s.push_str(nl);
    s
}

fn valid_key(k: &str) -> bool {
    (1..=64).contains(&k.len())
        && k.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
}

fn read_props(root: &Path) -> Vec<(String, String)> {
    fs::read(root.join(PROPS))
        .map(|b| parse_props(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

fn prop<'a>(p: &'a [(String, String)], k: &str) -> Option<&'a str> {
    p.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str())
}

// ---------- what the folder holds ----------

/// The Minecraft version in a jar or folder name: `1.20.1`, `1.21`, `26.1.2`.
pub fn mc_version_in(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let num = |mut j: usize| {
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        j
    };
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_digit() || b[i - 1] == b'.' && i >= 2 && b[i - 2].is_ascii_digit())) {
            let j = num(i);
            let major: u32 = s[i..j].parse().unwrap_or(0);
            if (major == 1 || (25..=40).contains(&major)) && j + 1 < b.len() && b[j] == b'.' && b[j + 1].is_ascii_digit() {
                let mut k = num(j + 1);
                if k + 1 < b.len() && b[k] == b'.' && b[k + 1].is_ascii_digit() {
                    k = num(k + 1);
                }
                return Some(s[i..k].to_owned());
            }
            i = j;
            continue;
        }
        i += 1;
    }
    None
}

/// NeoForge `21.1.77` runs Minecraft `1.21.1`; from 26.x on the numbers match
/// the release: `26.1.0.5` → `26.1`, `26.1.2.3` → `26.1.2`.
fn neoforge_mc(v: &str) -> Option<String> {
    let num = |s: &str| s.split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse::<u32>().ok());
    let mut p = v.split('.');
    let major = num(p.next()?)?;
    let minor = num(p.next()?)?;
    if major < 20 {
        return None;
    }
    Some(if major >= 25 {
        match p.next().and_then(num) {
            Some(patch) if patch > 0 => format!("{major}.{minor}.{patch}"),
            _ => format!("{major}.{minor}"),
        }
    } else if minor == 0 {
        format!("1.{major}")
    } else {
        format!("1.{major}.{minor}")
    })
}

fn first_dir_name(p: &Path) -> Option<String> {
    let mut v: Vec<String> = fs::read_dir(p)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v.pop()
}

fn jar_version(p: &Path) -> Option<String> {
    let f = fs::File::open(p).ok()?;
    let mut z = zip::ZipArchive::new(f).ok()?;
    let mut e = z.by_name("version.json").ok()?;
    if e.size() > 64 * 1024 {
        return None;
    }
    let mut s = String::new();
    std::io::Read::read_to_string(&mut e, &mut s).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    v.get("id").or_else(|| v.get("name"))?.as_str().map(str::to_owned)
}

/// Which server software the folder runs, as far as its files tell.
pub fn software(root: &Path) -> Value {
    let mut jars: Vec<String> = fs::read_dir(root)
        .map(|r| {
            r.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.to_ascii_lowercase().ends_with(".jar"))
                .collect()
        })
        .unwrap_or_default();
    jars.sort();
    let low: Vec<String> = jars.iter().map(|j| j.to_ascii_lowercase()).collect();
    let jar = |k: &str| low.iter().position(|j| j.contains(k)).map(|i| jars[i].clone());
    let has = |p: &str| root.join(p).exists();

    let (kind, main, mut version) = if has("libraries/net/neoforged/neoforge") {
        let v = first_dir_name(&root.join("libraries/net/neoforged/neoforge"));
        ("NeoForge", jar("neoforge"), v.as_deref().and_then(neoforge_mc))
    } else if has("libraries/net/minecraftforge/forge") {
        let v = first_dir_name(&root.join("libraries/net/minecraftforge/forge"));
        ("Forge", jar("forge"), v.as_deref().and_then(mc_version_in))
    } else if let Some(j) = jar("quilt") {
        ("Quilt", Some(j), None)
    } else if let Some(j) = jar("fabric") {
        ("Fabric", Some(j), None)
    } else if has(".fabric") {
        ("Fabric", None, None)
    } else if let Some(j) = jar("purpur") {
        ("Purpur", Some(j), None)
    } else if let Some(j) = jar("folia") {
        ("Folia", Some(j), None)
    } else if let Some(j) = jar("paper") {
        ("Paper", Some(j), None)
    } else if let Some(j) = jar("spigot") {
        ("Spigot", Some(j), None)
    } else if let Some(j) = jar("velocity") {
        ("Velocity", Some(j), None)
    } else if let Some(j) = jar("server").or_else(|| jars.first().cloned()) {
        ("Vanilla", Some(j), None)
    } else {
        return Value::Null;
    };
    if version.is_none() {
        version = fs::read(root.join("version_history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["currentVersion"].as_str().map(str::to_owned))
            .and_then(|s| s.split_once("MC: ").map(|(_, r)| r.trim_end_matches(')').trim().to_owned()));
    }
    if version.is_none() {
        version = main.as_deref().and_then(mc_version_in);
    }
    if version.is_none() {
        version = main.as_ref().and_then(|j| jar_version(&root.join(j)));
    }
    json!({ "kind": kind, "version": version, "jar": main })
}

fn count_jars(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".jar"))
                .count()
        })
        .unwrap_or(0)
}

fn icon(root: &Path) -> Option<String> {
    let b = fs::read(root.join(ICON)).ok()?;
    (b.len() <= ICON_MAX && skins::png_size(&b).is_some())
        .then(|| format!("data:image/png;base64,{}", skins::b64_encode(&b)))
}

/// What a card in the server list shows.
fn summary(c: &Core, s: &Server) -> Value {
    let d = s.dir();
    let exists = d.is_dir();
    let p = if exists { read_props(&d) } else { Vec::new() };
    json!({
        "id": s.id,
        "name": s.name,
        "host": s.host,
        "created_at": s.created_at,
        "exists": exists,
        "icon": if exists { icon(&d) } else { None },
        "motd": prop(&p, "motd").unwrap_or("A Minecraft Server"),
        "port": prop(&p, "server-port").unwrap_or("25565"),
        "max_players": prop(&p, "max-players").unwrap_or("20"),
        "software": if exists { cores::software_of(s).unwrap_or_else(|| software(&d)) } else { Value::Null },
        "core": s.core,
        "managed": cores::managed(c, s),
        "script": (exists && d.join(SCRIPT).is_file()).then_some(SCRIPT),
        "memory_mb": s.core.as_ref().map(|i| i.memory_mb).or(s.memory_mb).unwrap_or(cores::MEMORY_DEFAULT),
        "auto_restart": s.auto_restart,
        "run": c.server_procs.status(&s.id),
    })
}

// ---------- players ----------

pub const LISTS: [(&str, &str); 5] = [
    ("whitelist", "whitelist.json"),
    ("ops", "ops.json"),
    ("banned", "banned-players.json"),
    ("banned_ips", "banned-ips.json"),
    ("known", "usercache.json"),
];

fn list_file(list: &str) -> Result<&'static str, CoreError> {
    LISTS
        .iter()
        .find(|(k, _)| *k == list)
        .map(|(_, f)| *f)
        .ok_or_else(|| bad(format!("нет такого списка: {list}")))
}

fn read_list(root: &Path, file: &str) -> Vec<Value> {
    fs::read(root.join(file))
        .ok()
        .and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(Value::is_object)
        .collect()
}

pub fn valid_player(n: &str) -> bool {
    (1..=16).contains(&n.len()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn hyphenate(id: &str) -> Option<String> {
    let h: String = id.chars().filter(|c| *c != '-').collect();
    (h.len() == 32 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]).to_ascii_lowercase())
}

/// Ban dates as the server writes them: `2026-10-21 10:11:00 +0000`.
pub fn mc_date(secs: u64) -> String {
    let z = (secs / 86400) as i64 + 719_468;
    let rem = secs % 86400;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} +0000", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The player's name and UUID: the server's own cache first, then the offline
/// UUID for `online-mode=false`, otherwise Mojang.
async fn resolve(c: &Core, root: &Path, name: &str) -> Result<(String, String), CoreError> {
    for e in read_list(root, "usercache.json") {
        if e["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            if let Some(u) = e["uuid"].as_str() {
                return Ok((e["name"].as_str().unwrap_or(name).to_owned(), u.to_owned()));
            }
        }
    }
    if prop(&read_props(root), "online-mode") == Some("false") {
        return Ok((name.to_owned(), accounts::offline_uuid(name)));
    }
    let r = c
        .http()
        .get(format!("https://api.mojang.com/users/profiles/minecraft/{name}"))
        .send()
        .await?;
    if r.status().as_u16() == 204 || r.status().as_u16() == 404 {
        return Err(CoreError::msg(format!("игрок {name} не найден среди лицензионных аккаунтов")));
    }
    let v: Value = r.error_for_status()?.json().await?;
    let uuid = v["id"].as_str().and_then(hyphenate).ok_or_else(|| CoreError::msg("Mojang вернул странный ответ"))?;
    Ok((v["name"].as_str().unwrap_or(name).to_owned(), uuid))
}

fn write_list(root: &Path, file: &str, list: &[Value]) -> Result<(), CoreError> {
    let mut s = serde_json::to_string_pretty(list)?;
    s.push('\n');
    fs::write(root.join(file), s)?;
    Ok(())
}

fn same_player(e: &Value, who: &str) -> bool {
    e["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(who))
        || e["uuid"].as_str().is_some_and(|u| u.eq_ignore_ascii_case(who))
}

// ---------- files ----------

/// Files that hold Java arguments of the server (Forge args, JVM options):
/// a plugin must not be able to put code into the server's start.
fn launch_file(path: &str) -> bool {
    let n = path.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    n.ends_with("_args.txt") || n == "user_jvm_args.txt" || n.ends_with(".vmoptions")
}

#[derive(Deserialize)]
struct FileArg {
    id: String,
    #[serde(default)]
    path: String,
}

fn mtime(m: &fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

#[derive(Deserialize)]
struct IdArg {
    id: String,
}

pub(crate) fn register(r: &mut ActionRegistry) {
    r.register("servers.list", P::Servers, false, |c, _| async move {
        let st = load(&c)?;
        Ok(json!(st.servers.iter().map(|s| summary(&c, s)).collect::<Vec<_>>()))
    });

    // Built-in only: plugins go through the native folder picker (`servers.pick`).
    r.register("servers.add", P::Internal, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            #[serde(default)]
            name: String,
            dir: String,
        }
        let A { name, dir } = args(a)?;
        let d = allowed_dir(&c.data_dir, Path::new(&dir))?;
        let name = if name.trim().is_empty() {
            d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Сервер".into())
        } else {
            name
        };
        let name = clean_name(&name)?;
        let _g = c.state_lock.lock().await;
        let mut st = load(&c)?;
        let host = Host::Local { dir: d.to_string_lossy().into_owned() };
        if let Some(s) = st.servers.iter().find(|s| s.host == host) {
            return Ok(json!({ "id": s.id, "existing": true }));
        }
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_owned();
        st.servers.push(Server { id: id.clone(), name, host, created_at: unix_now(), core: None, memory_mb: None, auto_restart: false });
        store::save(&c.path(FILE), &st, false)?;
        Ok(json!({ "id": id, "existing": false }))
    });

    r.register("servers.update", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            name: String,
        }
        let A { id, name } = args(a)?;
        let name = clean_name(&name)?;
        let _g = c.state_lock.lock().await;
        let mut st = load(&c)?;
        let s = st.servers.iter_mut().find(|s| s.id == id).ok_or_else(|| bad("нет такого сервера"))?;
        s.name = name;
        store::save(&c.path(FILE), &st, false)?;
        Ok(json!({ "ok": true }))
    });

    // Forgets the server; its folder stays untouched, unless the launcher
    // created that folder itself and `delete_files` is set.
    r.register("servers.remove", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            #[serde(default)]
            delete_files: bool,
        }
        let A { id, delete_files } = args(a)?;
        if c.server_procs.alive(&id) {
            return Err(bad("сначала остановите сервер"));
        }
        if delete_files {
            let s = find(&c, &id)?;
            if !cores::managed(&c, &s) {
                return Err(denied("файлы удаляются только у серверов, которые создал лаунчер"));
            }
            let d = s.dir();
            if d.is_dir() {
                fs::remove_dir_all(&d)?;
            }
        }
        let _g = c.state_lock.lock().await;
        let mut st = load(&c)?;
        st.servers.retain(|s| s.id != id);
        store::save(&c.path(FILE), &st, false)?;
        if delete_files {
            backups::remove_all(&c, &id);
        }
        c.server_procs.forget(&id);
        Ok(json!({ "ok": true }))
    });

    r.register("servers.info", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        let s = find(&c, &id)?;
        let mut v = summary(&c, &s);
        let d = s.dir();
        v["dir"] = json!(d.to_string_lossy());
        if v["exists"] == json!(true) {
            let p = read_props(&d);
            let level = prop(&p, "level-name").unwrap_or("world").to_owned();
            let eula = fs::read_to_string(d.join("eula.txt")).unwrap_or_default();
            v["has_properties"] = json!(d.join(PROPS).is_file());
            v["properties"] = json!(p.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect::<Vec<_>>());
            v["eula"] = json!(eula.lines().any(|l| l.trim().eq_ignore_ascii_case("eula=true")));
            v["world"] = json!({ "name": level, "exists": d.join(&level).is_dir() });
            v["mods"] = json!(count_jars(&d.join("mods")));
            v["plugins"] = json!(count_jars(&d.join("plugins")));
        }
        Ok(v)
    });

    r.register("servers.properties.set", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            values: serde_json::Map<String, Value>,
        }
        let A { id, values } = args(a)?;
        let mut vals = Vec::new();
        for (k, v) in values {
            if !valid_key(&k) {
                return Err(bad(format!("{k}: такого параметра не бывает")));
            }
            let v = match v {
                Value::String(s) => s,
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                _ => return Err(bad(format!("{k}: нужна строка, число или да/нет"))),
            };
            if v.contains(['\n', '\r']) || v.len() > 4096 {
                return Err(bad(format!("{k}: одна строка, до 4096 символов")));
            }
            vals.push((k, v));
        }
        let root = find(&c, &id)?.root()?;
        let p = inside(&root, PROPS)?;
        let text = fs::read(&p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        fs::write(&p, update_props(&text, &vals))?;
        Ok(json!({ "ok": true }))
    });

    r.register("servers.icon.set", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            png: String,
        }
        let A { id, png } = args(a)?;
        let b64 = png.split_once("base64,").map_or(png.as_str(), |(_, b)| b);
        let b = skins::b64_decode(b64).ok_or_else(|| bad("png: не base64"))?;
        if b.len() > ICON_MAX || skins::png_size(&b) != Some((64, 64)) {
            return Err(bad("иконка сервера: PNG 64×64"));
        }
        let root = find(&c, &id)?.root()?;
        fs::write(inside(&root, ICON)?, b)?;
        Ok(json!({ "ok": true }))
    });
    r.register("servers.icon.delete", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        let p = inside(&find(&c, &id)?.root()?, ICON)?;
        if p.is_file() {
            fs::remove_file(p)?;
        }
        Ok(json!({ "ok": true }))
    });

    r.register("servers.players", P::Servers, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        let root = find(&c, &id)?.root()?;
        let p = read_props(&root);
        let mut v = json!({
            "whitelist_enabled": prop(&p, "white-list") == Some("true"),
            "online_mode": prop(&p, "online-mode") != Some("false"),
        });
        for (k, f) in LISTS {
            v[k] = json!(read_list(&root, f));
        }
        Ok(v)
    });

    r.register("servers.players.edit", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            list: String,
            op: String,
            name: String,
            #[serde(default)]
            reason: String,
        }
        let A { id, list, op, name, reason } = args(a)?;
        if !matches!(list.as_str(), "whitelist" | "ops" | "banned") {
            return Err(bad("менять можно вайтлист, операторов и баны"));
        }
        let file = list_file(&list)?;
        let root = find(&c, &id)?.root()?;
        let mut items = read_list(&root, file);
        match op.as_str() {
            "remove" => {
                let before = items.len();
                items.retain(|e| !same_player(e, &name));
                if items.len() != before {
                    write_list(&root, file, &items)?;
                }
                Ok(json!({ "ok": true }))
            }
            "add" => {
                if !valid_player(&name) {
                    return Err(bad("ник: латиница, цифры и _, до 16 символов"));
                }
                if items.iter().any(|e| same_player(e, &name)) {
                    return Ok(json!({ "ok": true, "existing": true }));
                }
                let (name, uuid) = resolve(&c, &root, &name).await?;
                let e = match list.as_str() {
                    "whitelist" => json!({ "uuid": uuid, "name": name }),
                    "ops" => json!({ "uuid": uuid, "name": name, "level": 4, "bypassesPlayerLimit": false }),
                    _ => json!({
                        "uuid": uuid, "name": name, "created": mc_date(unix_now()),
                        "source": "Freedom Launcher", "expires": "forever",
                        "reason": if reason.trim().is_empty() { "Banned by an operator." } else { reason.trim() },
                    }),
                };
                items.push(e);
                write_list(&root, file, &items)?;
                Ok(json!({ "ok": true, "name": name, "uuid": uuid }))
            }
            _ => Err(bad("op: add или remove")),
        }
    });

    r.register("servers.files.list", P::Servers, false, |c, a| async move {
        let FileArg { id, path } = args(a)?;
        let d = inside(&find(&c, &id)?.root()?, &path)?;
        if !d.is_dir() {
            return Err(bad("папка не найдена"));
        }
        let mut out = Vec::new();
        for e in fs::read_dir(&d)?.flatten() {
            let (Ok(m), Ok(t)) = (e.metadata(), e.file_type()) else { continue };
            if t.is_symlink() {
                continue;
            }
            out.push(json!({
                "name": e.file_name().to_string_lossy(),
                "dir": t.is_dir(),
                "size": if t.is_dir() { 0 } else { m.len() },
                "modified": mtime(&m),
            }));
            if out.len() >= LIST_MAX {
                break;
            }
        }
        out.sort_by(|a, b| {
            b["dir"].as_bool().cmp(&a["dir"].as_bool()).then_with(|| {
                a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase())
            })
        });
        Ok(json!(out))
    });
    r.register("servers.files.read", P::Servers, false, |c, a| async move {
        let FileArg { id, path } = args(a)?;
        let p = inside(&find(&c, &id)?.root()?, &path)?;
        if !p.is_file() {
            return Err(bad("файл не найден"));
        }
        if fs::metadata(&p)?.len() as usize > FILE_MAX {
            return Err(bad("файл больше 2 МБ"));
        }
        let text = String::from_utf8(fs::read(&p)?).map_err(|_| bad("это не текстовый файл"))?;
        Ok(json!({ "text": text, "writable": writable(&path) }))
    });
    r.register("servers.files.write", P::Servers, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
            path: String,
            text: String,
        }
        let A { id, path, text } = args(a)?;
        if !writable(&path) || launch_file(&path) {
            return Err(denied(format!("{path}: этот файл менять из плагина нельзя")));
        }
        if text.len() > FILE_MAX {
            return Err(bad("файл больше 2 МБ"));
        }
        let root = find(&c, &id)?.root()?;
        let p = inside(&root, &path)?;
        if p.is_dir() {
            return Err(bad("по этому пути папка"));
        }
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
            if !parent.canonicalize()?.starts_with(root.canonicalize()?) {
                return Err(denied("путь вне папки сервера"));
            }
        }
        fs::write(&p, text)?;
        Ok(json!({ "ok": true }))
    });
    r.register("servers.files.delete", P::Servers, false, |c, a| async move {
        let FileArg { id, path } = args(a)?;
        if !writable(&path) || launch_file(&path) {
            return Err(denied(format!("{path}: этот файл менять из плагина нельзя")));
        }
        let p = inside(&find(&c, &id)?.root()?, &path)?;
        if !p.is_file() {
            return Err(bad("файл не найден"));
        }
        fs::remove_file(&p)?;
        Ok(json!({ "ok": true }))
    });
    cores::register(r);
    run::register(r);
    backups::register(r);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_roundtrip() {
        let text = "#Minecraft server properties\n#Wed Oct 21\nmotd=\\u00A7aПривет\nmax-players=20\nwhite-list=false\n";
        let p = parse_props(text);
        assert_eq!(p[0], ("motd".into(), "§aПривет".into()));
        assert_eq!(p.len(), 3);
        let out = update_props(text, &[("max-players".into(), "50".into()), ("motd".into(), "§bСервер".into()), ("pvp".into(), "false".into())]);
        assert!(out.starts_with("#Minecraft server properties\n#Wed Oct 21\nmotd=\\u00A7b\\u0421"));
        assert!(out.contains("\nmax-players=50\nwhite-list=false\npvp=false\n"));
        assert_eq!(prop(&parse_props(&out), "motd"), Some("§bСервер"));
        assert_eq!(unescape(&escape("a\\b 😀")), "a\\b 😀");
        assert_eq!(update_props("a=1\r\nb=2\r\n", &[("b".into(), "3".into())]), "a=1\r\nb=3\r\n");
    }

    #[test]
    fn versions() {
        assert_eq!(mc_version_in("paper-1.20.1-196.jar").as_deref(), Some("1.20.1"));
        assert_eq!(mc_version_in("minecraft_server.1.12.2.jar").as_deref(), Some("1.12.2"));
        assert_eq!(mc_version_in("fabric-server-mc.1.21-loader.0.16.5-launcher.1.0.1.jar").as_deref(), Some("1.21"));
        assert_eq!(mc_version_in("paper-26.1.2-12.jar").as_deref(), Some("26.1.2"));
        assert_eq!(mc_version_in("1.20.1-47.2.0").as_deref(), Some("1.20.1"));
        assert_eq!(mc_version_in("server.jar"), None);
        assert_eq!(mc_version_in("forge-47.2.0.jar"), None);
        assert_eq!(neoforge_mc("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_mc("20.2.3").as_deref(), Some("1.20.2"));
        assert_eq!(neoforge_mc("26.1.0.5").as_deref(), Some("26.1"));
        assert_eq!(neoforge_mc("26.1.2.3").as_deref(), Some("26.1.2"));
        assert_eq!(neoforge_mc("21.0.14-beta").as_deref(), Some("1.21"));
        assert_eq!(neoforge_mc("0.25w14craftmine.3-beta"), None);
    }

    #[test]
    fn players_and_dates() {
        assert!(valid_player("Notch_2") && !valid_player("ник") && !valid_player("a_very_long_name_x"));
        assert_eq!(hyphenate("069a79f444e94726a5befca90e38aaf5").as_deref(), Some("069a79f4-44e9-4726-a5be-fca90e38aaf5"));
        assert_eq!(mc_date(0), "1970-01-01 00:00:00 +0000");
        assert_eq!(mc_date(1_792_577_972), "2026-10-21 10:19:32 +0000");
    }
}