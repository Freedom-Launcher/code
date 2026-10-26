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
use std::io::Read;

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
pub const RESERVED: [&str; 16] = [
    "app",
    "storage",
    "files",
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
    "servers",
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
    /// Own pages, each a tab in the rail (needs `ui_pages`).
    #[serde(default)]
    pub pages: Vec<Page>,
    #[serde(default)]
    pub actions: Vec<UiAction>,
    #[serde(default)]
    pub panels: Vec<Panel>,
    /// HTML shown full-window while the game starts (needs `ui_overlay`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_overlay: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub title: String,
    /// HTML file of the page, default `<id>.html`.
    #[serde(default)]
    pub entry: String,
}

pub const MAX_PAGES: usize = 4;

/// Native places where an addon action appears as a launcher button or menu item.
/// The launcher draws them in its own style; the addon only gets `{instance, place}`.
pub const ACTION_PLACES: [&str; 6] = [
    "instance.toolbar",
    "menu.instance",
    "menu.ctx.instance",
    "logs.toolbar",
    "crash",
    "palette",
];
/// Places where an addon panel is rendered inline, inside a launcher screen.
pub const PANEL_PLACES: [&str; 1] = ["instance.overview"];
pub const MAX_ACTIONS: usize = 12;
pub const MAX_PANELS: usize = 4;

/// A command the addon adds to the launcher's own buttons and menus.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UiAction {
    pub id: String,
    pub title: String,
    /// Launcher icon name (`share`, `note`, `folder`, …).
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub places: Vec<String>,
}

/// An inline block of the addon inside a launcher screen (gets the instance id).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Panel {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// HTML file of the panel, default `<id>.html`.
    #[serde(default)]
    pub entry: String,
    pub place: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Name of the service shown on the accounts screen.
    pub label: String,
    /// authlib-injector API root of the service (https, host listed in
    /// `hosts`). The game then signs in, and loads skins and capes, there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yggdrasil: Option<String>,
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
    /// Requested permissions the user chose not to give. Not asked again
    /// until the user opens «Права…» themselves.
    pub declined: BTreeSet<Permission>,
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
    let mut m: Manifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("addon.json: {e}"))?;
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
        if *p == Permission::Servers {
            return Err("право servers больше не существует".into());
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
        if let Some(y) = &p.yggdrasil {
            crate::net::check_url(y, &m.hosts)
                .map_err(|e| format!("account_provider.yggdrasil: {e}"))?;
        }
    }
    if !m.pages.is_empty() && !m.permissions.contains(&Permission::UiPages) {
        return Err("pages требуют права ui_pages".into());
    }
    if m.pages.len() > MAX_PAGES {
        return Err(format!("pages: не больше {MAX_PAGES}"));
    }
    let mut seen = HashSet::new();
    for p in &mut m.pages {
        if !valid_id(&p.id) || !seen.insert(p.id.clone()) {
            return Err(format!("pages: неверный или повторный id {}", p.id));
        }
        if !(1..=30).contains(&p.title.trim().chars().count()) {
            return Err("pages.title: от 1 до 30 символов".into());
        }
        if p.entry.is_empty() {
            p.entry = format!("{}.html", p.id);
        }
        if !safe_rel(&p.entry, &["html"]) || file(data_dir, id, &p.entry).is_none() {
            return Err(format!("pages.entry: {} — нужен .html внутри папки", p.entry));
        }
    }
    if (!m.actions.is_empty() || !m.panels.is_empty())
        && !m.permissions.contains(&Permission::UiPages)
    {
        return Err("actions и panels требуют права ui_pages".into());
    }
    if m.actions.len() > MAX_ACTIONS {
        return Err(format!("actions: не больше {MAX_ACTIONS}"));
    }
    if m.panels.len() > MAX_PANELS {
        return Err(format!("panels: не больше {MAX_PANELS}"));
    }
    let mut seen = HashSet::new();
    for a in &m.actions {
        if !valid_id(&a.id) || !seen.insert(a.id.clone()) {
            return Err(format!("actions: неверный или повторный id {}", a.id));
        }
        if !(1..=40).contains(&a.title.trim().chars().count()) {
            return Err("actions.title: от 1 до 40 символов".into());
        }
        if a.icon.len() > 24 || !a.icon.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            return Err(format!("actions.icon: неверное имя {}", a.icon));
        }
        if a.places.is_empty()
            || a.places.len() > ACTION_PLACES.len()
            || a.places.iter().any(|x| !ACTION_PLACES.contains(&x.as_str()))
        {
            return Err(format!("actions.places: неизвестное место у {}", a.id));
        }
    }
    let mut seen = HashSet::new();
    for p in &mut m.panels {
        if !valid_id(&p.id) || !seen.insert(p.id.clone()) {
            return Err(format!("panels: неверный или повторный id {}", p.id));
        }
        if p.title.chars().count() > 40 {
            return Err("panels.title: не больше 40 символов".into());
        }
        if !PANEL_PLACES.contains(&p.place.as_str()) {
            return Err(format!("panels.place: неизвестное место {}", p.place));
        }
        if p.entry.is_empty() {
            p.entry = format!("{}.html", p.id);
        }
        if !safe_rel(&p.entry, &["html"]) || file(data_dir, id, &p.entry).is_none() {
            return Err(format!("panels.entry: {} — нужен .html внутри папки", p.entry));
        }
    }
    if let Some(o) = &m.launch_overlay {
        if !m.permissions.contains(&Permission::UiOverlay) {
            return Err("launch_overlay требует права ui_overlay".into());
        }
        if !safe_rel(o, &["html"]) || file(data_dir, id, o).is_none() {
            return Err(format!("launch_overlay: {o} — нужен .html внутри папки"));
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
                    .map(|p| {
                        json!({ "id": p, "tier": p.tier(),
                                "granted": e.granted.contains(p), "declined": e.declined.contains(p) })
                    })
                    .collect();
                // Something new to decide: requested, neither given nor refused.
                let pending = perms
                    .iter()
                    .any(|p| p["granted"] == false && p["declined"] == false);
                let ok = |q: Permission| m.permissions.contains(&q) && e.granted.contains(&q);
                let pages: Vec<&Page> = if ok(Permission::UiPages) { m.pages.iter().collect() } else { vec![] };
                let ui = ok(Permission::UiPages);
                let actions: Vec<&UiAction> = if ui { m.actions.iter().collect() } else { vec![] };
                let panels: Vec<&Panel> = if ui { m.panels.iter().collect() } else { vec![] };
                let mut places: Vec<&str> = m
                    .actions
                    .iter()
                    .flat_map(|a| a.places.iter().map(String::as_str))
                    .chain(m.panels.iter().map(|p| p.place.as_str()))
                    .collect();
                places.sort_unstable();
                places.dedup();
                let overlay = m.launch_overlay.as_ref().filter(|_| ok(Permission::UiOverlay));
                json!({
                    "id": m.id, "name": m.name, "version": m.version, "author": m.author,
                    "description": m.description, "trust": trust(data_dir, &id),
                    "signature_error": crate::signing::check(&dir(data_dir).join(&id), &id).err(),
                    "permissions": perms, "slots": m.slots, "hosts": m.hosts,
                    "entry": m.entry, "enabled": e.enabled, "pending": pending,
                    "account_provider": m.account_provider,
                    "pages": pages, "launch_overlay": overlay,
                    "actions": actions, "panels": panels, "places": places,
                    "wants_pages": !m.pages.is_empty(),
                    "wants_overlay": m.launch_overlay.is_some(),
                })
            }
            Err(e) => json!({ "id": id, "name": id, "error": e }),
        })
        .collect()
}

/// Files a plugin archive may contain.
const PACKAGE: [&str; 13] = [
    "html", "js", "css", "json", "png", "jpg", "jpeg", "webp", "svg", "woff2", "woff", "md", "txt",
];
const MAX_FILES: usize = 1024;
const MAX_BYTES: u64 = 64 << 20;

/// Installs a plugin from a `.zip`, as published in any repository's releases:
/// `addon.json` at the top of the archive or inside one top folder. An
/// installed copy with the same id is replaced only if the new one loads; its
/// approval stays, and new permissions are asked about again.
pub fn install_zip(data_dir: &Path, zip_path: &Path) -> R<String> {
    let f = fs::File::open(zip_path).map_err(|_| "файл не найден".to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "это не архив .zip".to_string())?;
    if z.len() > MAX_FILES {
        return Err(format!("в архиве больше {MAX_FILES} файлов"));
    }
    let names: Vec<String> = (0..z.len())
        .filter_map(|i| z.by_index(i).ok().map(|e| e.name().replace('\\', "/")))
        .collect();
    let prefix = if names.iter().any(|n| n == "addon.json") {
        String::new()
    } else {
        let top = names.first().and_then(|n| n.split('/').next()).unwrap_or_default();
        let p = format!("{top}/");
        if top.is_empty()
            || !names.iter().all(|n| n.starts_with(&p))
            || !names.iter().any(|n| *n == format!("{p}addon.json"))
        {
            return Err("в архиве нет addon.json".into());
        }
        p
    };
    let mut raw = Vec::new();
    z.by_name(&format!("{prefix}addon.json"))
        .map_err(|_| "в архиве нет addon.json".to_string())?
        .take(64 * 1024 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| format!("addon.json: {e}"))?;
    let id = serde_json::from_slice::<Value>(&raw)
        .ok()
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .filter(|id| valid_id(id) && !RESERVED.contains(&id.as_str()))
        .ok_or("addon.json: неверный id")?;
    let root = dir(data_dir);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let tmp = root.join(format!(".install-{}", uuid::Uuid::new_v4()));
    if let Err(e) = extract(&mut z, &prefix, &tmp) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let dest = root.join(&id);
    let old = root.join(format!(".old-{}", uuid::Uuid::new_v4()));
    let had = dest.exists();
    if had {
        fs::rename(&dest, &old).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&tmp, &dest) {
        if had {
            let _ = fs::rename(&old, &dest);
        }
        let _ = fs::remove_dir_all(&tmp);
        return Err(e.to_string());
    }
    if let Err(e) = load(data_dir, &id) {
        let _ = fs::remove_dir_all(&dest);
        if had {
            let _ = fs::rename(&old, &dest);
        }
        return Err(e);
    }
    let _ = fs::remove_dir_all(&old);
    Ok(id)
}

fn extract(z: &mut zip::ZipArchive<fs::File>, prefix: &str, to: &Path) -> R<()> {
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|_| "архив повреждён".to_string())?;
        let name = e.name().replace('\\', "/");
        let rel = name.strip_prefix(prefix).unwrap_or_default().to_owned();
        if e.is_dir() || rel.is_empty() {
            continue;
        }
        if e.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("ссылки в архиве не допускаются".into());
        }
        if !safe_rel(&rel, &PACKAGE) {
            return Err(format!("{rel}: недопустимое имя или тип файла"));
        }
        let mut data = Vec::new();
        (&mut e)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|_| "архив повреждён".to_string())?;
        total += data.len() as u64;
        if total > MAX_BYTES {
            return Err("плагин больше 64 МБ".into());
        }
        let out = to.join(&rel);
        if let Some(d) = out.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        fs::write(&out, data).map_err(|e| e.to_string())?;
    }
    Ok(())
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

#[cfg(test)]
mod install_tests {
    use super::*;
    use std::io::Write;

    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, b) in files {
            w.start_file(*n, o).unwrap();
            w.write_all(b.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn install_from_zip() {
        let d = std::env::temp_dir().join(format!("fl-inst-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        let z = d.join("p.zip");
        let m = r#"{"id":"ely-by","name":"Ely.by","permissions":["network"],"hosts":["authserver.ely.by"]}"#;
        zip_of(&z, &[("plugin-elyby/addon.json", m), ("plugin-elyby/index.html", "<p>x</p>")]);
        assert_eq!(install_zip(&d, &z).unwrap(), "ely-by");
        assert!(dir(&d).join("ely-by/index.html").is_file());
        // Reinstall replaces in place.
        zip_of(&z, &[("addon.json", m), ("index.html", "<p>y</p>")]);
        assert_eq!(install_zip(&d, &z).unwrap(), "ely-by");
        assert_eq!(fs::read_to_string(dir(&d).join("ely-by/index.html")).unwrap(), "<p>y</p>");
        // Escaping paths and code files are refused; the installed copy stays.
        zip_of(&z, &[("addon.json", m), ("index.html", "<p>z</p>"), ("../evil.html", "x")]);
        assert!(install_zip(&d, &z).is_err());
        zip_of(&z, &[("addon.json", m), ("index.html", "<p>z</p>"), ("run.exe", "x")]);
        assert!(install_zip(&d, &z).is_err());
        // A manifest that does not load is rolled back.
        zip_of(&z, &[("addon.json", r#"{"id":"ely-by","name":""}"#), ("index.html", "<p>z</p>")]);
        assert!(install_zip(&d, &z).is_err());
        assert_eq!(fs::read_to_string(dir(&d).join("ely-by/index.html")).unwrap(), "<p>y</p>");
        // yggdrasil must be https on a listed host.
        let bad = r#"{"id":"ely-by","name":"E","permissions":["network"],"hosts":["authserver.ely.by"],
            "account_provider":{"label":"Ely.by","yggdrasil":"https://evil.example/api"}}"#;
        fs::write(dir(&d).join("ely-by/addon.json"), bad).unwrap();
        assert!(load(&d, "ely-by").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn actions_and_panels() {
        let d = std::env::temp_dir().join(format!("fl-act-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        let z = d.join("p.zip");
        let good = r#"{"id":"demo-x","name":"Demo","permissions":["ui_pages"],
            "actions":[{"id":"go","title":"Go","icon":"share","places":["logs.toolbar","crash"]}],
            "panels":[{"id":"note","place":"instance.overview","entry":"index.html"}]}"#;
        zip_of(&z, &[("addon.json", good), ("index.html", "<p>x</p>")]);
        assert_eq!(install_zip(&d, &z).unwrap(), "demo-x");
        let bad = [
            // unknown place
            r#"{"id":"demo-x","name":"Demo","permissions":["ui_pages"],
                "actions":[{"id":"go","title":"Go","places":["nowhere"]}]}"#,
            // no ui_pages
            r#"{"id":"demo-x","name":"Demo",
                "actions":[{"id":"go","title":"Go","places":["crash"]}]}"#,
            // panel file missing
            r#"{"id":"demo-x","name":"Demo","permissions":["ui_pages"],
                "panels":[{"id":"note","place":"instance.overview","entry":"nope.html"}]}"#,
            // duplicate action id
            r#"{"id":"demo-x","name":"Demo","permissions":["ui_pages"],
                "actions":[{"id":"go","title":"A","places":["crash"]},{"id":"go","title":"B","places":["crash"]}]}"#,
            // bad icon name
            r#"{"id":"demo-x","name":"Demo","permissions":["ui_pages"],
                "actions":[{"id":"go","title":"Go","icon":"../x","places":["crash"]}]}"#,
        ];
        for b in bad {
            zip_of(&z, &[("addon.json", b), ("index.html", "<p>x</p>")]);
            assert!(install_zip(&d, &z).is_err(), "{b}");
        }
        // Panel entry defaults to <id>.html.
        let def = r#"{"id":"demo-x","name":"Demo","permissions":["ui_pages"],
            "panels":[{"id":"note","place":"instance.overview"}]}"#;
        zip_of(&z, &[("addon.json", def), ("index.html", "<p>x</p>"), ("note.html", "<p>n</p>")]);
        assert_eq!(install_zip(&d, &z).unwrap(), "demo-x");
        let _ = fs::remove_dir_all(&d);
    }
}
