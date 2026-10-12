//! The game actions' logic: install an instance's version (with Java) and
//! launch it with the selected account.

use std::sync::Arc;

use serde_json::{json, Value};

use super::{
    download::Progress,
    install::{self, Layout},
    java,
    launch::{self, LaunchAuth, LaunchOptions},
    version::Env,
};
use crate::{
    accounts::{AccountKind, AccountStore},
    instances::{Instance, InstanceStore},
    store, unix_now, Core, CoreError,
};

pub(crate) const ACCOUNTS: &str = "accounts.json";
pub(crate) const INSTANCES: &str = "instances.json";

fn find_instance(core: &Core, id: &str) -> Result<Instance, CoreError> {
    let s: InstanceStore = store::load(&core.path(INSTANCES))?;
    s.instances
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {id}")))
}

fn progress_for(core: &Core, instance: &str) -> Arc<Progress> {
    let mut map = core.game_progress.lock().expect("progress lock");
    let p = Arc::new(Progress::default());
    map.insert(instance.to_owned(), Arc::clone(&p));
    p
}

/// Current install progress of an instance (`null` if none ran).
pub fn progress(core: &Core, instance: &str) -> Value {
    let map = core.game_progress.lock().expect("progress lock");
    map.get(instance)
        .map(|p| p.snapshot())
        .unwrap_or(Value::Null)
}

/// Installs the game files and the matching Java runtime.
pub async fn install_instance(core: &Core, instance: &str) -> Result<Value, CoreError> {
    let inst = find_instance(core, instance)?;
    let p = progress_for(core, instance);
    p.set_stage("game");
    let v = install::install(core, &inst.version, Arc::clone(&p)).await?;
    p.set_stage("java");
    let exe = java::ensure(core, v.java_version.as_ref(), Arc::clone(&p)).await?;
    p.set_stage("done");
    Ok(json!({ "instance": instance, "version": v.id, "java": exe }))
}

fn launch_auth(core: &Core) -> Result<LaunchAuth, CoreError> {
    let s: AccountStore = store::load(&core.path(ACCOUNTS))?;
    let acc = s
        .selected
        .as_deref()
        .and_then(|id| s.accounts.iter().find(|a| a.id == id))
        .ok_or_else(|| CoreError::msg("choose an account first"))?;
    match &acc.kind {
        AccountKind::Microsoft => {
            let sec = acc
                .secrets
                .as_ref()
                .ok_or_else(|| CoreError::msg("sign in to Microsoft again"))?;
            if sec.mc_expires_at <= unix_now() + 60 {
                return Err(CoreError::msg(
                    "session expired, sign in to Microsoft again",
                ));
            }
            Ok(LaunchAuth {
                name: acc.name.clone(),
                uuid: acc.uuid.replace('-', ""),
                access_token: sec.mc_access_token.clone(),
                xuid: String::new(),
                user_type: "msa".into(),
            })
        }
        AccountKind::Offline => {
            if !s.offline_available() {
                return Err(CoreError::msg(
                    "local profiles need a Microsoft account with the game",
                ));
            }
            Ok(LaunchAuth {
                name: acc.name.clone(),
                uuid: acc.uuid.replace('-', ""),
                access_token: "0".into(),
                xuid: String::new(),
                user_type: "legacy".into(),
            })
        }
        AccountKind::Provider { provider } => Err(CoreError::msg(format!(
            "accounts from {provider} are launched by their addon"
        ))),
    }
}

/// Makes sure everything is installed, then starts the game.
pub async fn launch_instance(core: &Core, instance: &str) -> Result<Value, CoreError> {
    let auth = launch_auth(core)?;
    let inst = find_instance(core, instance)?;
    let layout = Layout::new(core);
    // Full verification is `game.install`; a launch only installs when needed.
    if !layout.installed_marker(&inst.version).exists() {
        install_instance(core, instance).await?;
    }
    let v = install::load_version(core, &layout, &inst.version).await?;
    let java = java::ensure(core, v.java_version.as_ref(), Arc::new(Progress::default())).await?;
    let opts = LaunchOptions {
        java: java.clone(),
        game_dir: core.data_dir.join("instances").join(&inst.id),
        memory_mb: launch::default_memory_mb(launch::total_ram_mb()),
        extra_jvm_args: Vec::new(),
        client_id: std::env::var("FREEDOM_MSA_CLIENT_ID").unwrap_or_default(),
    };
    let args = launch::build_args(&v, &Env::current(), &layout, &auth, &opts);
    let child = launch::spawn(&java, &args, &opts.game_dir)?;
    Ok(json!({ "instance": instance, "pid": child.id(), "memory_mb": opts.memory_mb }))
}
