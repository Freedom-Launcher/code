//! Addons (M3): HTML/JS packages that run in an isolated iframe and reach the
//! core only through `dispatch(Caller::Addon{..})`, with exactly the permissions
//! the user approved. A folder `<data>/addons/<id>/` with an `addon.json`.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    themes::{safe_rel, valid_id},
    Caller, Permission, Trust,
};

/// UI slots (`data-slot`) an addon may mount into.
pub const SLOTS: [&str; 8] = [
    "rail.end",
    "topbar.end",
    "instance.end",
    "accounts.end",
    "discover.end",
    "settings.end",
    "menu.ctx.instance",
    "menu.instance",
];

/// Namespaces of built-in actions: no addon may take these ids.
pub const RESERVED: [&str; 13] = [
    "app",
    "settings",
    "safe",
    "themes",
    "addons",
    "versions",
    "accounts",
    "instances",
    "game",
    "skins",
    "java",
    "system",
    "net",
];

/// Files an addon may ship.
pub const ASSETS: [&str; 11] = [
    "html", "js", "css", "json", "png", "jpg", "jpeg", "webp", "svg", "woff2", "woff",
];

pub const STATE: &str = "addons.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub permissions: BTreeSet<Permission>,
    #[serde(default)]
    pub slots: Vec<String>,
    /// Hosts the addon may reach through the core's `network` action.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Present on account-provider addons (needs `accounts_provide`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_provider: Option<ProviderInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Name of the service shown on the accounts screen.
    pub label: String,
}

fn default_entry() -> String {
    "index.html".into()
}

/// What the user decided for one addon.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub enabled: bool,
    pub granted: BTreeSet<Permission>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub addons: BTreeMap<String, Entry>,
}

type R<T> = Result<T, String>;

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("addons")
}

/// A file of addon `id`, only if it really lies inside that addon's folder.
pub fn file(data_dir: &Path, id: &str, rel: &str) -> Option<PathBuf> {
    if !valid_id(id) || !safe_rel(rel, &ASSETS) {
        return None;
    }
    let root = dir(data_dir).join(id).canonicalize().ok()?;
    let p = root.join(rel).canonicalize().ok()?;
    (p.starts_with(&root) && p.is_file()).then_some(p)
}

fn valid_host(h: &str) -> bool {
    h.len() <= 253
        && h.split('.').count() >= 2
        && !h
            .rsplit('.')
            .next()
            .is_some_and(|l| l.chars().all(|c| c.is_ascii_digit()))
        && h.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// Trust badge of an installed addon: from a valid `signature.json` by a
/// project key, otherwise «Сообщество».
pub fn trust(data_dir: &Path, id: &str) -> Trust {
    crate::signing::trust(&dir(data_dir).join(id), id)
}

/// Loads and fully checks an installed addon.
pub fn load(data_dir: &Path, id: &str) -> R<Manifest> {
    if !valid_id(id) {
        return Err("неверный id плагина".into());
    }
    if RESERVED.contains(&id) {
        return Err("id занят встроенными действиями".into());
    }
    let p = file(data_dir, id, "addon.json").ok_or("addon.json не найден")?;
    let bytes = fs::read(&p).map_err(|e| format!("addon.json: {e}"))?;
    if bytes.len() > 64 * 1024 {
        return Err("addon.json: слишком большой файл".into());
    }
    let m: Manifest = serde_json::from_slice(&bytes).map_err(|e| format!("addon.json: {e}"))?;
    if m.id != id {
        return Err("id в addon.json не совпадает с именем папки".into());
    }
    if m.name.trim().is_empty() || m.name.chars().count() > 60 {
        return Err("name: от 1 до 60 символов".into());
    }
    if !safe_rel(&m.entry, &["html"]) || file(data_dir, id, &m.entry).is_none() {
        return Err(format!("entry: {} — нужен .html внутри папки", m.entry));
    }
    let max = trust(data_dir, id).max_tier();
    for p in &m.permissions {
        if *p == Permission::Internal {
            return Err("право internal недоступно плагинам".into());
        }
        if p.tier() > max {
            return Err(format!(
                "право {p:?} доступно только пакетам «Официальный» и «Проверено»"
            ));
        }
    }
    if m.slots.len() > 8 || m.slots.iter().any(|s| !SLOTS.contains(&s.as_str())) {
        return Err("slots: неизвестный слот".into());
    }
    if m.hosts.len() > 16 || m.hosts.iter().any(|h| !valid_host(h)) {
        return Err("hosts: нужны имена хостов без схемы и пути".into());
    }
    if !m.hosts.is_empty() && !m.permissions.contains(&Permission::Network) {
        return Err("hosts требуют права network".into());
    }
    if let Some(p) = &m.account_provider {
        if !m.permissions.contains(&Permission::AccountsProvide) {
            return Err("account_provider требует права accounts_provide".into());
        }
        if !(1..=40).contains(&p.label.trim().chars().count()) {
            return Err("account_provider.label: от 1 до 40 символов".into());
        }
    }
    Ok(m)
}

pub fn installed_ids(data_dir: &Path) -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_id(n))
        .collect();
    ids.sort();
    ids
}

/// Installed addons with their state; `pending` = requested but not yet approved.
pub fn list(data_dir: &Path, st: &State) -> Vec<Value> {
    installed_ids(data_dir)
        .into_iter()
        .map(|id| match load(data_dir, &id) {
            Ok(m) => {
                let e = st.addons.get(&id).cloned().unwrap_or_default();
                let perms: Vec<Value> = m
                    .permissions
                    .iter()
                    .filter(|p| **p != Permission::Public)
                    .map(|p| json!({ "id": p, "tier": p.tier(), "granted": e.granted.contains(p) }))
                    .collect();
                let pending = perms.iter().any(|p| p["granted"] == false);
                json!({
                    "id": m.id, "name": m.name, "version": m.version, "author": m.author,
                    "description": m.description, "trust": trust(data_dir, &id),
                    "signature_error": crate::signing::check(&dir(data_dir).join(&id), &id).err(),
                    "permissions": perms, "slots": m.slots, "hosts": m.hosts,
                    "entry": m.entry, "enabled": e.enabled, "pending": pending,
                    "account_provider": m.account_provider,
                })
            }
            Err(e) => json!({ "id": id, "name": id, "error": e }),
        })
        .collect()
}

/// Caller for a running addon: enabled only, with permissions both approved
/// and still requested by the current manifest.
pub fn caller(data_dir: &Path, st: &State, id: &str) -> Option<Caller> {
    let e = st.addons.get(id).filter(|e| e.enabled)?;
    let m = load(data_dir, id).ok()?;
    let granted: HashSet<Permission> = m.permissions.intersection(&e.granted).copied().collect();
    Some(Caller::Addon {
        id: id.into(),
        granted,
    })
}
