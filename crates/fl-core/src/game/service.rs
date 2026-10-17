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

/// Adds ended game sessions to each instance's playtime.
async fn record_playtime(core: &Core) -> Result<(), CoreError> {
    let done = core.game_running.take_finished();
    if done.is_empty() {
        return Ok(());
    }
    let _g = core.state_lock.lock().await;
    let mut s: InstanceStore = store::load(&core.path(INSTANCES))?;
    for (id, secs) in done {
        if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
            i.playtime_secs += secs;
        }
    }
    store::save(&core.path(INSTANCES), &s, false)
}

fn find_instance(core: &Core, id: &str) -> Result<Instance, CoreError> {
    let s: InstanceStore = store::load(&core.path(INSTANCES))?;
    s.instances
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {id}")))
}

pub(crate) fn progress_for(core: &Core, instance: &str) -> Arc<Progress> {
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
            return Ok(
                json!({ "instance": instance, "version": id, "java": exe, "skipped": true }),
            );
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
    let mut s: AccountStore = crate::accounts::load_store(&path)?;
    let Some(acc) = s
        .selected
        .clone()
        .and_then(|id| s.accounts.iter_mut().find(|a| a.id == id))
    else {
        return Ok(());
    };
    let Some(sec) = acc.secrets.as_ref() else {
        return Ok(());
    };
    if !matches!(acc.kind, AccountKind::Microsoft) || sec.mc_expires_at > unix_now() + 60 {
        return Ok(());
    }
    let fresh = crate::msauth::refresh(core, &sec.refresh_token).await?;
    acc.name = fresh.name;
    acc.owns_game = fresh.owns_game;
    acc.secrets = fresh.secrets;
    crate::accounts::save_store(&path, &s)
}

async fn launch_auth(core: &Core) -> Result<LaunchAuth, CoreError> {
    refresh_selected(core).await?;
    let s: AccountStore = crate::accounts::load_store(&core.path(ACCOUNTS))?;
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
        AccountKind::Provider { provider } => {
            if !s.offline_available() {
                return Err(CoreError::msg(
                    "service accounts need a Microsoft account with the game",
                ));
            }
            if crate::providers::active(core, provider).is_none() {
                return Err(CoreError::msg(format!(
                    "the {provider} addon is off or can no longer provide accounts"
                )));
            }
            let sec = acc
                .secrets
                .as_ref()
                .filter(|sec| sec.mc_expires_at > unix_now() + 60)
                .ok_or_else(|| {
                    CoreError::msg("session expired, sign in through the addon again")
                })?;
            Ok(LaunchAuth {
                name: acc.name.clone(),
                uuid: acc.uuid.replace('-', ""),
                access_token: sec.mc_access_token.clone(),
                xuid: String::new(),
                user_type: "mojang".into(),
            })
        }
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
    let global: crate::settings::Settings = store::load(&core.path("settings.json"))?;
    let merged = inst.settings.over(&global.game);
    let set = &merged;
    let java = match set
        .java_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(p) if std::path::Path::new(p).is_file() => std::path::PathBuf::from(p),
        Some(p) => return Err(CoreError::msg(format!("Java not found: {p}"))),
        None => java::ensure(core, v.java_version.as_ref(), Arc::new(Progress::default())).await?,
    };
    let opts = LaunchOptions {
        java: java.clone(),
        game_dir: inst.game_dir(&core.data_dir),
        memory_mb: set
            .memory_mb
            .unwrap_or_else(|| launch::default_memory_mb(launch::total_ram_mb())),
        extra_jvm_args: set
            .jvm_args
            .as_deref()
            .map(|a| a.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
        client_id: crate::msauth::client_id().unwrap_or_default(),
        resolution: set.resolution(),
    };
    let mut env = Env::current();
    if opts.resolution.is_some() {
        env.features.insert("has_custom_resolution".into(), true);
    }
    let mut args = launch::build_args(&v, &env, &layout, &auth, &opts);
    if set.fullscreen == Some(true) {
        args.push("--fullscreen".into());
    }
    // Variables like Prism/Modrinth give hooks, plus the user's own.
    let mut vars: std::collections::BTreeMap<String, String> = [
        ("INST_ID", inst.id.clone()),
        ("INST_NAME", inst.name.clone()),
        ("INST_MC_VER", inst.version.clone()),
        ("INST_DIR", opts.game_dir.to_string_lossy().into_owned()),
        ("INST_JAVA", java.to_string_lossy().into_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    vars.extend(set.env.clone().unwrap_or_default());
    let line = |o: &Option<String>| {
        o.as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
    };
    if let Some(pre) = line(&set.hooks.pre_launch) {
        let (dir, vars2) = (opts.game_dir.clone(), vars.clone());
        std::fs::create_dir_all(&dir)?;
        let ok = tokio::task::spawn_blocking(move || {
            launch::shell(&pre).envs(&vars2).current_dir(&dir).status()
        })
        .await
        .map_err(|e| CoreError::msg(e.to_string()))?
        .map_err(|e| CoreError::msg(format!("Команда перед запуском не запустилась: {e}")))?;
        if !ok.success() {
            return Err(CoreError::msg(format!(
                "Команда перед запуском завершилась с кодом {}",
                ok.code().unwrap_or(-1)
            )));
        }
    }
    let wrapper = line(&set.hooks.wrapper);
    let child = launch::spawn(&java, &args, &opts.game_dir, wrapper.as_deref(), &vars)?;
    if let Some(post) = line(&set.hooks.post_exit) {
        let mut c = launch::shell(&post);
        c.envs(&vars).current_dir(&opts.game_dir);
        core.game_running.on_exit(&inst.id, c);
    }
    let pid = child.id();
    core.game_running.started(&inst.id, child);
    mark_played(core, &inst.id).await?;
    Ok(json!({
        "instance": inst.id,
        "pid": pid,
        "memory_mb": opts.memory_mb,
        "after_launch": set.after_launch.as_deref().unwrap_or("keep"),
    }))
}

/// Instances whose game is running, and those still being prepared.
pub async fn running(core: &Core) -> Value {
    let (running, starting) = core.game_running.list();
    let _ = record_playtime(core).await;
    json!({ "running": running, "starting": starting })
}

/// Closes the instance's game.
pub async fn stop(core: &Core, instance: &str) -> Value {
    let stopped = core.game_running.stop(instance);
    let _ = record_playtime(core).await;
    json!({ "instance": instance, "stopped": stopped })
}
