//! The game actions' logic: install an instance's version (with Java) and
//! launch it with the selected account.

use std::sync::Arc;

use serde_json::{json, Value};

use super::{
    download::Progress,
    install::{self, Layout},
    java,
    launch::{self, LaunchAuth, LaunchOptions},
    loader,
    version::Env,
};
use crate::{
    accounts::{AccountKind, AccountStore},
    instances::{Instance, InstanceStore},
    store, unix_now, Core, CoreError,
};

pub(crate) const ACCOUNTS: &str = "accounts.json";
pub(crate) const INSTANCES: &str = "instances.json";

/// Records the launch time («Играли 2 ч назад», order of «Продолжить»).
async fn mark_played(core: &Core, id: &str) -> Result<(), CoreError> {
    let _g = core.state_lock.lock().await;
    let mut s: InstanceStore = store::load(&core.path(INSTANCES))?;
    if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
        i.last_played = crate::unix_now();
        store::save(&core.path(INSTANCES), &s, false)?;
    }
    Ok(())
}

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

/// Already installed (marker + local version JSON + Java)? Checked without
/// network or hashing, so pressing «Играть» again is instant.
fn ready(core: &Core, layout: &Layout, inst: &Instance) -> Option<(String, std::path::PathBuf)> {
    if !layout.installed_marker(&inst.version_id()).exists() {
        return None;
    }
    let v = loader::resolve_local(layout, inst)?;
    let exe = java::installed(core, v.java_version.as_ref())?;
    Some((v.id, exe))
}

/// Installs the game files and the matching Java runtime. With `verify`
/// every file is re-hashed (the «Проверить файлы» menu item); otherwise an
/// installed version returns at once.
pub async fn install_instance(
    core: &Core,
    instance: &str,
    verify: bool,
) -> Result<Value, CoreError> {
    let inst = find_instance(core, instance)?;
    let p = progress_for(core, instance);
    if !verify {
        if let Some((id, exe)) = ready(core, &Layout::new(core), &inst) {
            p.set_stage("done");
            return Ok(json!({ "instance": instance, "version": id, "java": exe, "skipped": true }));
        }
    }
    p.set_stage("game");
    let v = loader::resolve(core, &Layout::new(core), &inst).await?;
    let v = install::install(core, v, Arc::clone(&p)).await?;
    if matches!(inst.loader.as_str(), "forge" | "neoforge") {
        // The marker is only valid once the processors have patched the jar.
        p.set_stage("loader");
        let layout = Layout::new(core);
        if let Err(e) = super::forge::post_install(core, &layout, &inst, &v, Arc::clone(&p)).await {
            let _ = std::fs::remove_file(layout.installed_marker(&v.id));
            return Err(e);
        }
    }
    p.set_stage("java");
    let exe = java::ensure(core, v.java_version.as_ref(), Arc::clone(&p)).await?;
    p.set_stage("done");
    Ok(json!({ "instance": instance, "version": v.id, "java": exe }))
}

/// Refreshes an expired Microsoft session in place (same account id).
async fn refresh_selected(core: &Core) -> Result<(), CoreError> {
    let path = core.path(ACCOUNTS);
    let mut s: AccountStore = store::load(&path)?;
    let Some(acc) = s
        .selected
        .clone()
        .and_then(|id| s.accounts.iter_mut().find(|a| a.id == id))
    else {
        return Ok(());
    };
    let Some(sec) = acc.secrets.as_ref() else { return Ok(()) };
    if !matches!(acc.kind, AccountKind::Microsoft) || sec.mc_expires_at > unix_now() + 60 {
        return Ok(());
    }
    let fresh = crate::msauth::refresh(core, &sec.refresh_token).await?;
    acc.name = fresh.name;
    acc.owns_game = fresh.owns_game;
    acc.secrets = fresh.secrets;
    store::save(&path, &s, true)
}

async fn launch_auth(core: &Core) -> Result<LaunchAuth, CoreError> {
    refresh_selected(core).await?;
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
            if !acc.owns_game {
                return Err(CoreError::msg(
                    "this Microsoft account does not own Minecraft: Java Edition",
                ));
            }
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
    let auth = launch_auth(core).await?;
    let inst = find_instance(core, instance)?;
    if !core.game_running.begin(instance) {
        return Ok(json!({ "instance": instance, "already_running": true }));
    }
    let res = start(core, &inst, auth).await;
    if res.is_err() {
        core.game_running.cancel(instance);
    }
    res
}

async fn start(core: &Core, inst: &Instance, auth: LaunchAuth) -> Result<Value, CoreError> {
    let layout = Layout::new(core);
    // Full verification is `game.install`; a launch only installs when needed.
    if !layout.installed_marker(&inst.version_id()).exists() {
        install_instance(core, &inst.id, false).await?;
    }
    let v = loader::resolve(core, &layout, inst).await?;
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
    let pid = child.id();
    core.game_running.started(&inst.id, child);
    mark_played(core, &inst.id).await?;
    Ok(json!({ "instance": inst.id, "pid": pid, "memory_mb": opts.memory_mb }))
}

/// Instances whose game is running, and those still being prepared.
pub fn running(core: &Core) -> Value {
    let (running, starting) = core.game_running.list();
    json!({ "running": running, "starting": starting })
}

/// Closes the instance's game.
pub fn stop(core: &Core, instance: &str) -> Value {
    json!({ "instance": instance, "stopped": core.game_running.stop(instance) })
}
