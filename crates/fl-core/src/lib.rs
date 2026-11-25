//! Freedom Launcher core.
//!
//! Everything the launcher can do is a named **action** (`accounts.list`,
//! `versions.list`, ...). The built-in UI, themes (via `data-action` markup) and
//! addons all go through the same [`Core::dispatch`] entry point, and every call
//! is checked against the caller's permissions.

/// Launcher release version (from `app/tauri.conf.json`).
pub const VERSION: &str = env!("FL_VERSION");

use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Instant,
};

use serde_json::{json, Value};

pub mod accounts;
pub mod actions;
pub mod addon_data;
pub mod addons;
pub mod agent;
pub mod art;
pub mod backups;
pub mod blocked;
mod builtin;
pub mod bundled;
pub mod catalog;
pub mod curseforge;
pub mod dev_plugins;
pub mod doctor;
pub mod error;
pub mod export;
pub mod fonts;
pub mod foreign;
pub mod game;
pub mod instances;
pub mod keychain;
pub mod media;
pub mod modrinth;
pub mod mods;
pub mod mrpack;
pub mod msauth;
pub mod net;
pub mod people;
pub mod perf;
pub mod permissions;
pub mod plugin_store;
pub mod providers;
pub mod relocate;
pub mod rimg;
pub mod servers;
pub mod settings;
pub mod share;
pub mod signing;
pub mod skin_packs;
pub mod skins;
pub mod store;
pub mod themes;
pub mod versions;
pub mod wallpaper;
mod ygg;

pub use actions::{ActionRegistry, ActionSpec, Caller};
pub use error::CoreError;
pub use permissions::{Permission, Tier, Trust};

pub struct Core {
    pub data_dir: PathBuf,
    pub safe_mode: bool,
    started: Instant,
    ready_ms: OnceLock<u64>,
    // Created lazily: no TLS/root-store work on the startup path.
    http: OnceLock<reqwest::Client>,
    registry: ActionRegistry,
    /// Serialises read-modify-write of state files.
    pub(crate) state_lock: tokio::sync::Mutex<()>,
    pub(crate) ms_pending: tokio::sync::Mutex<Option<msauth::Pending>>,
    /// Window sign-in in progress (PKCE verifier + state).
    pub(crate) ms_code: tokio::sync::Mutex<Option<msauth::PendingCode>>,
    /// Memoized `settings.json` and the file's `(mtime, length)` when it was read
    /// (see [`Core::settings`]).
    settings_cache: std::sync::Mutex<Option<(Stamp, Arc<settings::Settings>)>>,
    /// Install progress per instance id, polled by `game.progress`.
    pub(crate) game_progress:
        std::sync::Mutex<std::collections::HashMap<String, Arc<game::download::Progress>>>,
    /// Game processes per instance (`game.running`, no double launches).
    pub(crate) game_running: game::running::Running,
    /// Developer-linked plugin folders: what was last synced (`dev_plugins`).
    pub(crate) dev_links:
        std::sync::Mutex<std::collections::BTreeMap<String, dev_plugins::LinkState>>,
}

impl Core {
    pub fn new(
        data_dir: PathBuf,
        started: Instant,
        safe_mode: bool,
    ) -> Result<Arc<Self>, CoreError> {
        std::fs::create_dir_all(&data_dir)?;
        let mut registry = ActionRegistry::default();
        builtin::register(&mut registry, safe_mode);
        net::register(&mut registry);
        Ok(Arc::new(Self {
            data_dir,
            safe_mode,
            started,
            ready_ms: OnceLock::new(),
            http: OnceLock::new(),
            registry,
            state_lock: Default::default(),
            ms_pending: Default::default(),
            ms_code: Default::default(),
            settings_cache: Default::default(),
            game_progress: Default::default(),
            game_running: Default::default(),
            dev_links: Default::default(),
        }))
    }

    /// Single entry point for every action call.
    pub async fn dispatch(
        self: &Arc<Self>,
        caller: &Caller,
        name: &str,
        args: Value,
    ) -> Result<Value, CoreError> {
        let spec = self
            .registry
            .get(name)
            .ok_or_else(|| CoreError::UnknownAction(name.to_owned()))?;
        caller.check(name, spec)?;
        let args = net::stamp_caller(caller, name, args);
        (spec.handler)(Arc::clone(self), args).await
    }

    /// The current settings, memoized. Hot paths (`instances_root`, and so every
    /// `game_dir`, plus `addon_caller`) used to read and parse the file on each
    /// call; now one metadata probe replaces the read, and only a file that
    /// changed is read again. The stamp is the file's time and size together, so
    /// an outside edit is seen even on a slow-clocking file system (same time,
    /// other length). Writes go through [`Core::save_settings`], which clears the
    /// memoized copy outright. A file that does not parse is not remembered.
    pub fn settings(&self) -> Result<Arc<settings::Settings>, CoreError> {
        let path = self.path(SETTINGS);
        let now = stamp(&path);
        let mut cache = self
            .settings_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some((at, s)) = cache.as_ref() {
            if *at == now {
                return Ok(Arc::clone(s));
            }
        }
        let s = Arc::new(store::load(&path)?);
        *cache = Some((now, Arc::clone(&s)));
        Ok(s)
    }

    /// Writes `settings.json` and drops the memoized copy, so the next reader
    /// sees the new file at once. Every write of the settings goes through here
    /// or through [`Core::invalidate_settings`]: a write that kept the old time
    /// stamp (a slow file system clock, a second write right after the first)
    /// would otherwise leave the old values behind. The caller holds `state_lock`.
    pub(crate) fn save_settings(&self, s: &settings::Settings) -> Result<(), CoreError> {
        store::save(&self.path(SETTINGS), s, false)?;
        self.invalidate_settings();
        Ok(())
    }

    /// Drops the memoized settings after a write that did not go through
    /// [`Core::save_settings`] (the generic `update_json` in `plugin_store`).
    pub(crate) fn invalidate_settings(&self) {
        self.settings_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }

    /// Caller for a running addon, or `None` (unknown, not approved, disabled,
    /// addons switched off, or safe mode).
    pub fn addon_caller(&self, id: &str) -> Option<Caller> {
        if self.safe_mode {
            return None;
        }
        let s = self.settings().ok()?;
        if !s.addons_enabled {
            return None;
        }
        let st: addons::State = store::load(&self.path(addons::STATE)).ok()?;
        addons::caller(&self.data_dir, &st, id)
    }

    pub fn http(&self) -> &reqwest::Client {
        self.http.get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(concat!("FreedomLauncher/", env!("FL_VERSION")))
                .build()
                .expect("failed to build HTTP client")
        })
    }

    pub fn path(&self, file: &str) -> PathBuf {
        self.data_dir.join(file)
    }

    /// Where instance folders live: the user's chosen folder or `<data_dir>/instances`.
    pub fn instances_root(&self) -> PathBuf {
        self.settings()
            .ok()
            .and_then(|s| s.instances_dir.clone())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| self.default_instances_root())
    }

    pub fn default_instances_root(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    /// Records time from process start to first UI paint (first call wins).
    pub fn mark_ready(&self) -> u64 {
        *self
            .ready_ms
            .get_or_init(|| self.started.elapsed().as_millis() as u64)
    }

    pub fn info(&self) -> Value {
        json!({
            "name": "Freedom Launcher",
            "version": env!("FL_VERSION"),
            "safe_mode": self.safe_mode,
            "startup_ms": self.ready_ms.get(),
            "data_dir": self.data_dir,
        })
    }

    pub fn action_list(&self) -> Value {
        Value::Array(
            self.registry
                .iter()
                .map(|(name, s)| {
                    json!({
                        "name": name,
                        "permission": s.permission,
                        "tier": s.permission.tier(),
                        "bindable": s.bindable,
                    })
                })
                .collect(),
        )
    }
}

/// File name of the launcher settings inside the data folder.
pub(crate) const SETTINGS: &str = "settings.json";

/// `(time, size)` of a file; `(None, 0)` when it cannot be read.
type Stamp = (Option<std::time::SystemTime>, u64);

fn stamp(path: &std::path::Path) -> Stamp {
    std::fs::metadata(path)
        .map(|m| (m.modified().ok(), m.len()))
        .unwrap_or((None, 0))
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-core-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The memoized settings follow every way the file can change: a write by the
    /// core (`save_settings`), an outside edit that keeps the size, and deletion.
    #[tokio::test]
    async fn settings_follow_the_file() {
        let dir = tmp();
        let c = Core::new(dir.clone(), Instant::now(), false).unwrap();
        // No file yet: defaults, and the instances root is the default one.
        assert_eq!(c.settings().unwrap().theme, "default");
        assert_eq!(c.instances_root(), c.default_instances_root());

        // A core write is seen by the next reader at once.
        {
            let _g = c.state_lock.lock().await;
            let mut s = c.settings().unwrap().as_ref().clone();
            s.theme = "light".into();
            let custom = dir.join("game");
            std::fs::create_dir_all(&custom).unwrap();
            s.instances_dir = Some(custom.to_string_lossy().into_owned());
            c.save_settings(&s).unwrap();
        }
        assert_eq!(c.settings().unwrap().theme, "light");
        assert_eq!(c.instances_root(), dir.join("game"));

        // An outside edit (same length or not) is picked up too: the stamp is
        // time + size, so a same-size rewrite still shows.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            dir.join("settings.json"),
            br#"{"theme":"mine","instances_dir":null}"#,
        )
        .unwrap();
        assert_eq!(c.settings().unwrap().theme, "mine");
        assert_eq!(c.instances_root(), c.default_instances_root());

        // Deleting the file returns to defaults.
        std::fs::remove_file(dir.join("settings.json")).unwrap();
        assert_eq!(c.settings().unwrap().theme, "default");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An outside rewrite is picked up even inside the file system's time stamp
    /// granularity: the stamp includes the size, which changes with the text.
    #[test]
    fn rewritten_settings_are_seen() {
        let dir = tmp();
        let c = Core::new(dir.clone(), Instant::now(), false).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, br#"{"theme":"aa"}"#).unwrap();
        assert_eq!(c.settings().unwrap().theme, "aa");
        std::fs::write(&path, br#"{"theme":"bbbb"}"#).unwrap();
        assert_eq!(c.settings().unwrap().theme, "bbbb");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that does not parse is an error and is not remembered: fixing it
    /// on disk is enough, no restart needed.
    #[test]
    fn broken_settings_are_not_cached() {
        let dir = tmp();
        let c = Core::new(dir.clone(), Instant::now(), false).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, b"{ nope").unwrap();
        assert!(c.settings().is_err());
        std::fs::write(&path, br#"{"theme":"ok"}"#).unwrap();
        assert_eq!(c.settings().unwrap().theme, "ok");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
