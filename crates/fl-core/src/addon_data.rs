//! What an addon may touch beyond its UI, each behind its own permission:
//!
//! - `storage.*` (`storage`): the addon's own small key/value store in
//!   `<data>/addon-data/<id>/storage.json`. An addon sees only its own keys.
//! - `files.*` (`instance_files`): text files inside instance game folders
//!   (configs, options, worlds' data). Never outside an instance, never
//!   through a link, never code that would run (mods only via trusted mod
//!   actions), never the launcher's own bookkeeping files.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::{
    actions::ActionRegistry,
    instances::{Instance, InstanceStore},
    net::CALLER_KEY,
    store,
    themes::valid_id,
    Core, CoreError, Permission as P,
};

/// Whole storage of one addon, serialized.
pub const STORAGE_MAX: usize = 1024 * 1024;
pub const KEY_MAX: usize = 128;
/// Largest file an addon may read or write.
pub const FILE_MAX: usize = 2 * 1024 * 1024;
pub const LIST_MAX: usize = 1000;

/// Extensions an addon may never write or delete: things that run.
pub const CODE: [&str; 22] = [
    "jar", "class", "zip", "exe", "dll", "so", "dylib", "jnilib", "bat", "cmd", "ps1", "sh",
    "command", "com", "msi", "vbs", "js", "py", "lnk", "scr", "app", "url",
];
/// Top-level folders an addon may not write into (code lives there).
pub const CODE_DIRS: [&str; 4] = ["mods", "coremods", "kubejs", "natives"];

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

/// The calling addon, as stamped by the core (`$addon`); an addon cannot forge it.
fn addon_id(a: &Value) -> Result<String, CoreError> {
    a.get(CALLER_KEY)
        .and_then(Value::as_str)
        .filter(|s| valid_id(s))
        .map(str::to_owned)
        .ok_or_else(|| denied("хранилище доступно только плагинам"))
}

// ---------- storage ----------

pub fn storage_path(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join("addon-data").join(id).join("storage.json")
}

fn valid_key(k: &str) -> bool {
    (1..=KEY_MAX).contains(&k.chars().count()) && !k.chars().any(char::is_control)
}

#[derive(Deserialize)]
struct KeyArg {
    key: String,
}

// ---------- instance files ----------

/// A relative `a/b/c.txt` path: no `..`, no absolute paths, no drive letters,
/// no characters Windows would reinterpret.
pub fn rel_ok(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 240
        && p.split('/').all(|s| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && s.len() <= 120
                && !s.ends_with('.')
                && !s.ends_with(' ')
                && !s.chars().any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
        })
}

fn ext(p: &str) -> String {
    p.rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// May an addon change (write or delete) this path?
pub fn writable(p: &str) -> bool {
    let first = p.split('/').next().unwrap_or("").to_ascii_lowercase();
    let name = p.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    rel_ok(p)
        && !CODE.contains(&ext(p).as_str())
        && !CODE_DIRS.contains(&first.as_str())
        && !name.starts_with("freedom-")
}

/// `rel` inside `root`, refusing any link on the way so it cannot lead out.
pub fn inside(root: &Path, rel: &str) -> Result<PathBuf, CoreError> {
    if !rel.is_empty() && !rel_ok(rel) {
        return Err(bad("path: нужен относительный путь внутри сборки"));
    }
    let mut p = root.to_path_buf();
    for s in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(s);
        if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(denied("ссылки в пути запрещены"));
        }
    }
    if p.exists() {
        let r = root.canonicalize()?;
        if !p.canonicalize()?.starts_with(&r) {
            return Err(denied("путь вне сборки"));
        }
    }
    Ok(p)
}

fn game_dir(c: &Core, instance: &str) -> Result<PathBuf, CoreError> {
    let s: InstanceStore = store::load(&c.path("instances.json"))?;
    let i: Instance = s
        .instances
        .into_iter()
        .find(|i| i.id == instance)
        .ok_or_else(|| bad(format!("no such instance: {instance}")))?;
    let d = i.game_dir(&c.instances_root());
    if !d.is_dir() {
        return Err(bad("папка сборки ещё не создана"));
    }
    Ok(d)
}

#[derive(Deserialize)]
struct FileArg {
    instance: String,
    #[serde(default)]
    path: String,
}

pub(crate) fn register(r: &mut ActionRegistry) {
    r.register("storage.get", P::Storage, false, |c, a| async move {
        let id = addon_id(&a)?;
        let KeyArg { key } = args(a)?;
        let m: BTreeMap<String, Value> = store::load(&storage_path(&c.data_dir, &id))?;
        Ok(json!({ "value": m.get(&key).cloned().unwrap_or(Value::Null) }))
    });
    r.register("storage.list", P::Storage, false, |c, a| async move {
        let id = addon_id(&a)?;
        let m: BTreeMap<String, Value> = store::load(&storage_path(&c.data_dir, &id))?;
        Ok(json!({ "keys": m.keys().collect::<Vec<_>>() }))
    });
    r.register("storage.set", P::Storage, false, |c, a| async move {
        let id = addon_id(&a)?;
        #[derive(Deserialize)]
        struct A {
            key: String,
            value: Value,
        }
        let A { key, value } = args(a)?;
        if !valid_key(&key) {
            return Err(bad(format!("key: от 1 до {KEY_MAX} символов")));
        }
        let _g = c.state_lock.lock().await;
        let p = storage_path(&c.data_dir, &id);
        let mut m: BTreeMap<String, Value> = store::load(&p)?;
        m.insert(key, value);
        if serde_json::to_vec(&m)?.len() > STORAGE_MAX {
            return Err(bad("хранилище плагина переполнено (1 МБ)"));
        }
        store::save(&p, &m, false)?;
        Ok(json!({ "ok": true }))
    });
    r.register("storage.delete", P::Storage, false, |c, a| async move {
        let id = addon_id(&a)?;
        let KeyArg { key } = args(a)?;
        let _g = c.state_lock.lock().await;
        let p = storage_path(&c.data_dir, &id);
        let mut m: BTreeMap<String, Value> = store::load(&p)?;
        if m.remove(&key).is_some() {
            store::save(&p, &m, false)?;
        }
        Ok(json!({ "ok": true }))
    });

    r.register("files.list", P::InstanceFiles, false, |c, a| async move {
        let FileArg { instance, path } = args(a)?;
        let d = inside(&game_dir(&c, &instance)?, &path)?;
        let mut out = Vec::new();
        for e in fs::read_dir(&d)?.flatten() {
            let Ok(m) = e.metadata() else { continue };
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            out.push(json!({ "name": name, "dir": t.is_dir(), "size": if t.is_dir() { 0 } else { m.len() } }));
            if out.len() >= LIST_MAX {
                break;
            }
        }
        out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        Ok(json!(out))
    });
    r.register("files.read", P::InstanceFiles, false, |c, a| async move {
        let FileArg { instance, path } = args(a)?;
        let p = inside(&game_dir(&c, &instance)?, &path)?;
        if !p.is_file() {
            return Err(bad("файл не найден"));
        }
        if fs::metadata(&p)?.len() as usize > FILE_MAX {
            return Err(bad("файл больше 2 МБ"));
        }
        let text = String::from_utf8(fs::read(&p)?).map_err(|_| bad("это не текстовый файл"))?;
        Ok(json!({ "text": text }))
    });
    r.register("files.write", P::InstanceFiles, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            instance: String,
            path: String,
            text: String,
        }
        let A { instance, path, text } = args(a)?;
        if !writable(&path) {
            return Err(denied(format!("{path}: этот файл плагинам менять нельзя")));
        }
        if text.len() > FILE_MAX {
            return Err(bad("файл больше 2 МБ"));
        }
        let root = game_dir(&c, &instance)?;
        let p = inside(&root, &path)?;
        if p.is_dir() {
            return Err(bad("по этому пути папка"));
        }
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
            if !parent.canonicalize()?.starts_with(root.canonicalize()?) {
                return Err(denied("путь вне сборки"));
            }
        }
        fs::write(&p, text)?;
        Ok(json!({ "ok": true }))
    });
    r.register("files.delete", P::InstanceFiles, false, |c, a| async move {
        let FileArg { instance, path } = args(a)?;
        if !writable(&path) {
            return Err(denied(format!("{path}: этот файл плагинам менять нельзя")));
        }
        let p = inside(&game_dir(&c, &instance)?, &path)?;
        if !p.is_file() {
            return Err(bad("файл не найден"));
        }
        fs::remove_file(&p)?;
        Ok(json!({ "ok": true }))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        for ok in ["options.txt", "config/sodium.json", "saves/World 1/data/x.dat"] {
            assert!(rel_ok(ok), "{ok}");
        }
        for no in ["", "/etc/passwd", "../x", "a/../b", "a//b", "C:/x", "a\\b", "x.", "./a"] {
            assert!(!rel_ok(no), "{no}");
        }
        assert!(writable("config/sodium.json"));
        for no in ["mods/a.txt", "Mods/x.json", "a.jar", "x/y.EXE", "kubejs/s.txt", "freedom-blocked.json", "s.js"] {
            assert!(!writable(no), "{no}");
        }
    }
}