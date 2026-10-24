//! Freedom Launcher core.
//!
//! Everything the launcher can do is a named **action** (`accounts.list`,
//! `versions.list`, ...). The built-in UI, themes (via `data-action` markup) and
//! addons all go through the same [`Core::dispatch`] entry point, and every call
//! is checked against the caller's permissions.

use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Instant,
};

use serde_json::{json, Value};

pub mod accounts;
pub mod agent;
pub mod art;
pub mod blocked;
pub mod bundled;
pub mod actions;
pub mod addon_data;
pub mod addons;
mod builtin;
pub mod catalog;
pub mod curseforge;
pub mod diag;
pub mod error;
pub mod export;
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
pub mod permissions;
pub mod providers;
pub mod relocate;
pub mod rimg;
pub mod settings;
pub mod signing;
pub mod skin_packs;
pub mod skins;
pub mod store;
pub mod themes;
pub mod versions;
pub mod wallpaper;
pub mod fonts;

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
    /// Install progress per instance id, polled by `game.progress`.
    pub(crate) game_progress:
        std::sync::Mutex<std::collections::HashMap<String, Arc<game::download::Progress>>>,
    /// Game processes per instance (`game.running`, no double launches).
    pub(crate) game_running: game::running::Running,
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
            game_progress: Default::default(),
            game_running: Default::default(),
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

    /// Caller for a running addon, or `None` (unknown, not approved, disabled,
    /// addons switched off, or safe mode).
    pub fn addon_caller(&self, id: &str) -> Option<Caller> {
        if self.safe_mode {
            return None;
        }
        let s: settings::Settings = store::load(&self.path("settings.json")).ok()?;
        if !s.addons_enabled {
            return None;
        }
        let st: addons::State = store::load(&self.path(addons::STATE)).ok()?;
        addons::caller(&self.data_dir, &st, id)
    }

    pub fn http(&self) -> &reqwest::Client {
        self.http.get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(concat!("FreedomLauncher/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("failed to build HTTP client")
        })
    }

    pub fn path(&self, file: &str) -> PathBuf {
        self.data_dir.join(file)
    }

    /// Where instance folders live: the user's chosen folder or `<data_dir>/instances`.
    pub fn instances_root(&self) -> PathBuf {
        store::load::<settings::Settings>(&self.path("settings.json"))
            .ok()
            .and_then(|s| s.instances_dir)
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
            "version": env!("CARGO_PKG_VERSION"),
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

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
