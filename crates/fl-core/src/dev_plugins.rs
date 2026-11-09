//! Developer mode for plugins. Link a plugin folder and the launcher keeps an installed copy
//! in step with it, picked up within a second of a change, so a plugin can be built and tried
//! without zipping and installing it again after every edit.
//!
//! The copy is made exactly like a normal install (the folder is packed and goes through
//! `addons::install_zip`), so a linked folder follows every rule an archive does: allowed file
//! types, size limits, valid manifest, reserved ids and rights. A link can only replace a plugin
//! that is itself a developer link or hand-made, never a signed one or one from a catalog
//! source. Linked plugins are ordinary community plugins: they still need the player's approval
//! for every permission. Everything here is off until `developer_mode` is switched on.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{addons, plugin_store, settings::Settings, unix_now, Core, CoreError};

const SETTINGS_FILE: &str = "settings.json";
const MAX_FILES: usize = 2000;
const MAX_BYTES: u64 = 64 << 20;
const MAX_LINKS: usize = 8;
const SOURCE: &str = "dev";

/// What was last synced from one linked folder.
#[derive(Clone, Default)]
pub(crate) struct LinkState {
    id: String,
    name: String,
    stamp: String,
    rev: u64,
    synced_at: u64,
    error: Option<String>,
}

/// One linked folder as the UI sees it. `rev` goes up each time the installed copy was refreshed.
#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub path: String,
    pub id: String,
    pub name: String,
    pub rev: u64,
    pub synced_at: u64,
    pub error: Option<String>,
}

fn settings(c: &Core) -> Settings {
    crate::store::load(&c.path(SETTINGS_FILE)).unwrap_or_default()
}

fn require_dev(c: &Core) -> Result<Settings, CoreError> {
    let s = settings(c);
    if s.developer_mode {
        Ok(s)
    } else {
        Err(CoreError::msg(
            "Режим разработчика выключен (Настройки → Плагины)",
        ))
    }
}

/// Names that are never part of a plugin: version control, dependencies, build output.
fn ignored(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "__pycache__")
}

struct File {
    rel: String,
    path: PathBuf,
    len: u64,
    mtime: u128,
}

/// Every file of the folder worth shipping, in a stable order. Symlinks are skipped, never followed.
fn walk(root: &Path) -> Result<Vec<File>, CoreError> {
    fn go(dir: &Path, rel: &str, out: &mut Vec<File>, total: &mut u64) -> Result<(), CoreError> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            if ignored(&name) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(e.path()) else {
                continue;
            };
            let rel = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if meta.is_dir() {
                go(&e.path(), &rel, out, total)?;
            } else if meta.is_file() {
                *total += meta.len();
                if out.len() >= MAX_FILES || *total > MAX_BYTES {
                    return Err(CoreError::msg(format!(
                        "В папке слишком много файлов или она слишком большая (до {MAX_FILES} файлов и {} МБ)",
                        MAX_BYTES >> 20
                    )));
                }
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                out.push(File {
                    rel,
                    path: e.path(),
                    len: meta.len(),
                    mtime,
                });
            }
        }
        Ok(())
    }
    let (mut out, mut total) = (Vec::new(), 0);
    go(root, "", &mut out, &mut total)?;
    Ok(out)
}

fn stamp(files: &[File]) -> String {
    let mut h = Sha256::new();
    for f in files {
        h.update(format!("{}|{}|{}\n", f.rel, f.len, f.mtime).as_bytes());
    }
    crate::signing::hex(&h.finalize())
}

fn pack(files: &[File], out: &Path) -> Result<(), CoreError> {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(fs::File::create(out)?);
    let o = zip::write::SimpleFileOptions::default();
    for f in files {
        w.start_file(&f.rel, o)
            .map_err(|e| CoreError::msg(e.to_string()))?;
        w.write_all(&fs::read(&f.path)?)?;
    }
    w.finish().map_err(|e| CoreError::msg(e.to_string()))?;
    Ok(())
}

fn status(path: &str, s: &LinkState) -> Status {
    Status {
        path: path.to_owned(),
        id: s.id.clone(),
        name: s.name.clone(),
        rev: s.rev,
        synced_at: s.synced_at,
        error: s.error.clone(),
    }
}

/// Brings the installed copy in step with the folder if it changed (or `force`). Any failure is
/// recorded as the link's error and kept until the files change again.
async fn sync_one(c: &Core, path: &str, force: bool) -> Result<(), CoreError> {
    let root = Path::new(path);
    let old = c
        .dev_links
        .lock()
        .unwrap()
        .get(path)
        .cloned()
        .unwrap_or_default();
    let attempt = async {
        if !root.is_dir() {
            return Err((String::new(), CoreError::msg("Папка не найдена")));
        }
        let files = walk(root).map_err(|e| (String::new(), e))?;
        let st = stamp(&files);
        if !force && !old.stamp.is_empty() && st == old.stamp {
            return Ok(None);
        }
        let tmp = c.data_dir.join(format!("dev-{}.zip", uuid::Uuid::new_v4()));
        let result = async {
            fs::create_dir_all(&c.data_dir)?;
            pack(&files, &tmp)?;
            let (id, _) = addons::zip_manifest(&tmp).map_err(CoreError::BadArgs)?;
            let state: addons::State =
                crate::store::load(&c.path(addons::STATE)).unwrap_or_default();
            let have = addons::installed_ids(&c.data_dir).contains(&id);
            let have_source = state
                .addons
                .get(&id)
                .map(|e| e.source.clone())
                .unwrap_or_default();
            let signed = addons::trust(&c.data_dir, &id) != crate::Trust::Community;
            plugin_store::may_replace(have, &have_source, signed, SOURCE)
                .map_err(CoreError::msg)?;
            let id = addons::install_zip(&c.data_dir, &tmp).map_err(CoreError::BadArgs)?;
            let key = id.clone();
            plugin_store::update_json(c, addons::STATE, move |s: &mut addons::State| {
                s.addons.entry(key).or_default().source = SOURCE.into();
            })
            .await?;
            let name = addons::load(&c.data_dir, &id)
                .map(|m| m.name)
                .unwrap_or_default();
            Ok::<_, CoreError>((id, name))
        }
        .await;
        let _ = fs::remove_file(&tmp);
        match result {
            Ok((id, name)) => Ok(Some((st, id, name))),
            Err(e) => Err((st, e)),
        }
    }
    .await;
    let mut links = c.dev_links.lock().unwrap();
    let entry = links.entry(path.to_owned()).or_default();
    match attempt {
        Ok(None) => {}
        Ok(Some((st, id, name))) => {
            *entry = LinkState {
                id,
                name,
                stamp: st,
                rev: old.rev + 1,
                synced_at: unix_now(),
                error: None,
            };
        }
        Err((st, e)) => {
            // Keep the stamp of what failed: no retry loop until the files change again.
            entry.error = Some(e.to_string());
            entry.synced_at = unix_now();
            if !st.is_empty() {
                entry.stamp = st;
            }
        }
    }
    Ok(())
}

fn statuses(c: &Core, s: &Settings) -> Vec<Status> {
    let links = c.dev_links.lock().unwrap();
    s.dev_plugins
        .iter()
        .map(|p| status(p, &links.get(p).cloned().unwrap_or_default()))
        .collect()
}

/// The linked folders; also picks up changes (the UI calls this about once a second).
pub async fn state(c: &Core) -> Result<Vec<Status>, CoreError> {
    let s = require_dev(c)?;
    for p in &s.dev_plugins {
        sync_one(c, p, false).await?;
    }
    Ok(statuses(c, &s))
}

fn valid_dir(c: &Core, input: &str) -> Result<String, CoreError> {
    let p = Path::new(input.trim());
    if !p.is_absolute() {
        return Err(CoreError::BadArgs("Нужен полный путь к папке".into()));
    }
    let real = p
        .canonicalize()
        .map_err(|_| CoreError::BadArgs("Папка не найдена".into()))?;
    if !real.is_dir() {
        return Err(CoreError::BadArgs("Это не папка".into()));
    }
    let data = c
        .data_dir
        .canonicalize()
        .unwrap_or_else(|_| c.data_dir.clone());
    if real.starts_with(&data) || data.starts_with(&real) {
        return Err(CoreError::BadArgs(
            "Нельзя подключить папку данных лаунчера или папку, в которой она лежит".into(),
        ));
    }
    Ok(real.to_string_lossy().into_owned())
}

/// Links a folder and syncs it at once. A folder with a problem is still linked: the problem is
/// shown next to it, and fixing the files is enough.
pub async fn link(c: &Core, input: &str) -> Result<Vec<Status>, CoreError> {
    let s = require_dev(c)?;
    let path = valid_dir(c, input)?;
    if !s.dev_plugins.contains(&path) {
        if s.dev_plugins.len() >= MAX_LINKS {
            return Err(CoreError::BadArgs(format!("Не больше {MAX_LINKS} папок")));
        }
        let p = path.clone();
        plugin_store::update_json(c, SETTINGS_FILE, move |s: &mut Settings| {
            s.dev_plugins.push(p)
        })
        .await?;
    }
    sync_one(c, &path, true).await?;
    Ok(statuses(c, &settings(c)))
}

/// Stops following a folder. The installed copy stays (remove it like any plugin).
pub async fn unlink(c: &Core, path: &str) -> Result<Vec<Status>, CoreError> {
    require_dev(c)?;
    let p = path.to_owned();
    plugin_store::update_json(c, SETTINGS_FILE, move |s: &mut Settings| {
        s.dev_plugins.retain(|x| *x != p)
    })
    .await?;
    c.dev_links.lock().unwrap().remove(path);
    Ok(statuses(c, &settings(c)))
}

/// A linked folder's path, for opening it: only folders that are linked.
pub fn folder(c: &Core, path: &str) -> Result<String, CoreError> {
    let s = require_dev(c)?;
    s.dev_plugins
        .iter()
        .find(|p| p.as_str() == path)
        .cloned()
        .ok_or_else(|| CoreError::BadArgs("Эта папка не подключена".into()))
}

/// Makes `<parent>/<id>` from the example plugin and links it.
pub async fn new_plugin(
    c: &Core,
    parent: &str,
    id: &str,
    name: &str,
) -> Result<Vec<Status>, CoreError> {
    require_dev(c)?;
    if !(1..=32).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || id.starts_with('-')
        || addons::RESERVED.contains(&id)
    {
        return Err(CoreError::BadArgs(
            "Идентификатор: 1–32 символа a-z, 0-9 и «-» (не зарезервированный)".into(),
        ));
    }
    let name = name.trim();
    if !(1..=60).contains(&name.len()) || name.chars().any(char::is_control) {
        return Err(CoreError::BadArgs("Название: 1–60 символов".into()));
    }
    let parent = valid_dir(c, parent)?;
    let dir = Path::new(&parent).join(id);
    if dir.exists() {
        return Err(CoreError::BadArgs("Такая папка уже есть".into()));
    }
    fs::create_dir_all(&dir)?;
    let mut m: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/addons/example/addon.json"))?;
    m["id"] = id.into();
    m["name"] = name.into();
    m["author"] = "".into();
    m["description"] = "Новый плагин".into();
    fs::write(
        dir.join("addon.json"),
        serde_json::to_string_pretty(&m)? + "\n",
    )?;
    fs::write(
        dir.join("index.html"),
        include_str!("../../../docs/addons/example/index.html"),
    )?;
    fs::write(
        dir.join("main.js"),
        include_str!("../../../docs/addons/example/main.js"),
    )?;
    fs::write(
        dir.join("style.css"),
        include_str!("../../../docs/addons/example/style.css"),
    )?;
    link(c, &dir.to_string_lossy()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{signing, Trust};
    use std::sync::Arc;
    use std::time::Instant;

    fn core() -> Arc<Core> {
        let d = std::env::temp_dir().join(format!("fl-dev-{}", uuid::Uuid::new_v4()));
        Core::new(d, Instant::now(), false).unwrap()
    }

    async fn dev_on(c: &Core) {
        plugin_store::update_json(c, SETTINGS_FILE, |s: &mut Settings| s.developer_mode = true)
            .await
            .unwrap();
    }

    /// A plugin folder outside the data dir, in its own temp location.
    fn folder(id: &str, version: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("fl-devsrc-{}", uuid::Uuid::new_v4()))
            .join(id);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("addon.json"),
            format!(r#"{{"id":"{id}","name":"Dev {id}","version":"{version}","permissions":["instances_read"]}}"#),
        )
        .unwrap();
        fs::write(d.join("index.html"), "<p>one</p>").unwrap();
        d
    }

    fn installed_html(c: &Core, id: &str) -> String {
        fs::read_to_string(addons::dir(&c.data_dir).join(id).join("index.html")).unwrap()
    }

    /// Make sure a rewrite is seen even on filesystems with coarse timestamps.
    fn rewrite(p: &Path, text: &str) {
        fs::write(p, text).unwrap();
    }

    #[tokio::test]
    async fn everything_is_off_until_developer_mode_is_on() {
        let c = core();
        let d = folder("demo", "0.1.0");
        for r in [
            state(&c).await.map(|_| ()),
            link(&c, &d.to_string_lossy()).await.map(|_| ()),
            unlink(&c, "x").await.map(|_| ()),
            new_plugin(&c, &d.to_string_lossy(), "x", "X")
                .await
                .map(|_| ()),
            folder_of(&c, "x"),
        ] {
            assert!(r
                .unwrap_err()
                .to_string()
                .contains("Режим разработчика выключен"));
        }
        assert!(!addons::dir(&c.data_dir).exists(), "nothing was installed");
    }

    fn folder_of(c: &Core, p: &str) -> Result<(), CoreError> {
        super::folder(c, p).map(|_| ())
    }

    #[tokio::test]
    async fn a_change_in_the_folder_reaches_the_installed_copy() {
        let c = core();
        dev_on(&c).await;
        let d = folder("demo", "0.1.0");
        let st = link(&c, &d.to_string_lossy()).await.unwrap();
        assert_eq!(
            (st[0].id.as_str(), st[0].rev, st[0].error.is_none()),
            ("demo", 1, true)
        );
        assert_eq!(installed_html(&c, "demo"), "<p>one</p>");
        let sstate: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap();
        assert_eq!(sstate.addons["demo"].source, "dev");
        // Not enabled, nothing granted: a linked plugin still needs the player's approval.
        assert!(!sstate.addons["demo"].enabled && sstate.addons["demo"].granted.is_empty());

        // Nothing changed: nothing happens (rev stays).
        assert_eq!(state(&c).await.unwrap()[0].rev, 1);
        // An edit is picked up on the next look.
        rewrite(&d.join("index.html"), "<p>two, longer</p>");
        let st = state(&c).await.unwrap();
        assert_eq!(st[0].rev, 2);
        assert_eq!(installed_html(&c, "demo"), "<p>two, longer</p>");
        // A new file is picked up too, and a deleted one goes away.
        fs::write(d.join("extra.css"), "a{}").unwrap();
        assert_eq!(state(&c).await.unwrap()[0].rev, 3);
        assert!(addons::dir(&c.data_dir).join("demo/extra.css").exists());
        fs::remove_file(d.join("extra.css")).unwrap();
        assert_eq!(state(&c).await.unwrap()[0].rev, 4);
        assert!(!addons::dir(&c.data_dir).join("demo/extra.css").exists());
    }

    #[tokio::test]
    async fn a_broken_folder_shows_its_error_and_recovers_when_fixed() {
        let c = core();
        dev_on(&c).await;
        let d = folder("demo", "0.1.0");
        link(&c, &d.to_string_lossy()).await.unwrap();
        fs::write(d.join("addon.json"), "{ not json").unwrap();
        let st = state(&c).await.unwrap();
        assert!(st[0].error.is_some() && st[0].rev == 1, "{:?}", st[0].error);
        // The last good copy keeps running meanwhile.
        assert_eq!(installed_html(&c, "demo"), "<p>one</p>");
        // The same broken files are not retried over and over: the error just stays.
        let again = state(&c).await.unwrap();
        assert_eq!(again[0].error, st[0].error);
        // A file type that is not allowed is refused like in any archive.
        fs::write(
            d.join("addon.json"),
            r#"{"id":"demo","name":"D","version":"0.1.0","permissions":[]}"#,
        )
        .unwrap();
        fs::write(d.join("run.exe"), "MZ").unwrap();
        assert!(
            state(&c).await.unwrap()[0].error.is_some(),
            ".exe must be refused"
        );
        fs::remove_file(d.join("run.exe")).unwrap();
        let ok = state(&c).await.unwrap();
        assert!(ok[0].error.is_none() && ok[0].rev == 2, "{:?}", ok[0].error);
    }

    #[tokio::test]
    async fn version_control_and_dependencies_are_not_shipped() {
        let c = core();
        dev_on(&c).await;
        let d = folder("demo", "0.1.0");
        for dir in [".git", "node_modules", "target"] {
            fs::create_dir_all(d.join(dir)).unwrap();
            fs::write(d.join(dir).join("big.exe"), "MZ").unwrap(); // would be refused if it were packed
        }
        fs::write(d.join(".env"), "SECRET=1").unwrap();
        let st = link(&c, &d.to_string_lossy()).await.unwrap();
        assert!(st[0].error.is_none(), "{:?}", st[0].error);
        let inst = addons::dir(&c.data_dir).join("demo");
        assert!(
            !inst.join(".git").exists()
                && !inst.join("node_modules").exists()
                && !inst.join(".env").exists()
        );
    }

    #[tokio::test]
    async fn a_link_cannot_replace_a_signed_or_catalog_plugin() {
        let c = core();
        dev_on(&c).await;
        // A signed plugin is installed.
        let signed = addons::dir(&c.data_dir).join("demo");
        fs::create_dir_all(&signed).unwrap();
        fs::write(
            signed.join("addon.json"),
            r#"{"id":"demo","name":"Official","version":"1.0.0","permissions":[]}"#,
        )
        .unwrap();
        fs::write(signed.join("index.html"), "<p>official</p>").unwrap();
        let sig = signing::sign(
            &signed,
            "demo",
            Trust::Official,
            "test",
            &signing::TEST_SEED,
        )
        .unwrap();
        fs::write(
            signed.join("signature.json"),
            serde_json::to_vec(&sig).unwrap(),
        )
        .unwrap();
        let d = folder("demo", "0.1.0");
        let st = link(&c, &d.to_string_lossy()).await.unwrap();
        assert!(
            st[0].error.as_deref().unwrap_or("").contains("подпис"),
            "{:?}",
            st[0].error
        );
        assert_eq!(
            installed_html(&c, "demo"),
            "<p>official</p>",
            "the official plugin is untouched"
        );

        // Same for a plugin that came from a catalog source.
        let other = folder("other", "0.1.0");
        let oi = addons::dir(&c.data_dir).join("other");
        fs::create_dir_all(&oi).unwrap();
        fs::write(
            oi.join("addon.json"),
            r#"{"id":"other","name":"S","version":"1.0.0","permissions":[]}"#,
        )
        .unwrap();
        fs::write(oi.join("index.html"), "<p>from a source</p>").unwrap();
        plugin_store::update_json(&c, addons::STATE, |s: &mut addons::State| {
            s.addons.entry("other".into()).or_default().source =
                "https://example.org/l.json".into();
        })
        .await
        .unwrap();
        let st = link(&c, &other.to_string_lossy()).await.unwrap();
        let last = st.last().unwrap();
        assert!(
            last.error.as_deref().unwrap_or("").contains("источника"),
            "{:?}",
            last.error
        );
        assert_eq!(installed_html(&c, "other"), "<p>from a source</p>");
    }

    #[tokio::test]
    async fn only_real_folders_outside_the_launcher_data_can_be_linked() {
        let c = core();
        dev_on(&c).await;
        fs::create_dir_all(&c.data_dir).unwrap();
        let file = std::env::temp_dir().join(format!("fl-devfile-{}", uuid::Uuid::new_v4()));
        fs::write(&file, "x").unwrap();
        for bad in [
            "relative/path".to_string(),
            "/definitely/not/here".to_string(),
            file.to_string_lossy().into_owned(),
            c.data_dir.to_string_lossy().into_owned(),
            addons::dir(&c.data_dir).to_string_lossy().into_owned(),
            c.data_dir.parent().unwrap().to_string_lossy().into_owned(), // contains the data dir
        ] {
            assert!(link(&c, &bad).await.is_err(), "{bad:?} must be refused");
        }
        assert!(settings(&c).dev_plugins.is_empty());
    }

    #[tokio::test]
    async fn unlink_stops_following_but_keeps_the_installed_copy() {
        let c = core();
        dev_on(&c).await;
        let d = folder("demo", "0.1.0");
        let path = link(&c, &d.to_string_lossy()).await.unwrap()[0]
            .path
            .clone();
        assert_eq!(super::folder(&c, &path).unwrap(), path);
        assert!(unlink(&c, &path).await.unwrap().is_empty());
        assert!(
            super::folder(&c, &path).is_err(),
            "no longer linked, so it cannot be opened through us"
        );
        rewrite(&d.join("index.html"), "<p>changed after unlink</p>");
        assert!(state(&c).await.unwrap().is_empty());
        assert_eq!(installed_html(&c, "demo"), "<p>one</p>");
    }

    #[tokio::test]
    async fn a_new_plugin_is_made_from_the_example_and_works_at_once() {
        let c = core();
        dev_on(&c).await;
        let parent = std::env::temp_dir().join(format!("fl-devnew-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&parent).unwrap();
        let st = new_plugin(&c, &parent.to_string_lossy(), "my-plugin", "Мой плагин")
            .await
            .unwrap();
        assert_eq!(
            (st[0].id.as_str(), st[0].error.is_none()),
            ("my-plugin", true)
        );
        assert!(
            parent.join("my-plugin/addon.json").exists()
                && parent.join("my-plugin/main.js").exists()
        );
        let m = addons::load(&c.data_dir, "my-plugin").unwrap();
        assert_eq!(m.name, "Мой плагин");
        // Bad ids and an existing folder are refused, nothing half-made is left.
        for id in ["Bad Id", "", "-x", "accounts", &"a".repeat(33)] {
            assert!(
                new_plugin(&c, &parent.to_string_lossy(), id, "X")
                    .await
                    .is_err(),
                "{id:?}"
            );
        }
        assert!(new_plugin(&c, &parent.to_string_lossy(), "my-plugin", "X")
            .await
            .is_err());
    }
}
