//! Built-in actions. The built-in screens use exactly these, like any addon.

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::{
    accounts::{self, Account, AccountKind, AccountStore},
    actions::ActionRegistry,
    addons,
    game::service,
    instances::{self, Instance, InstanceStore},
    modrinth, mods, mrpack, msauth,
    settings::Settings,
    store, themes, unix_now, versions, Core, CoreError, Permission as P,
};

#[derive(Deserialize)]
struct InstanceArg {
    instance: String,
}

#[derive(Deserialize)]
struct IdArg {
    id: String,
}

#[derive(Deserialize)]
struct InstallArg {
    instance: String,
    #[serde(default)]
    verify: bool,
}

fn args<T: DeserializeOwned>(v: Value) -> Result<T, CoreError> {
    let v = if v.is_null() { json!({}) } else { v };
    serde_json::from_value(v).map_err(|e| CoreError::BadArgs(e.to_string()))
}

const ACCOUNTS: &str = "accounts.json";
const INSTANCES: &str = "instances.json";
const SETTINGS: &str = "settings.json";

pub(crate) fn register(r: &mut ActionRegistry, safe_mode: bool) {
    r.register(
        "app.info",
        P::Public,
        true,
        |c, _| async move { Ok(c.info()) },
    );
    r.register("app.ready", P::Public, false, |c, _| async move {
        Ok(json!({ "startup_ms": c.mark_ready() }))
    });
    r.register("app.actions", P::Public, false, |c, _| async move {
        Ok(c.action_list())
    });

    // Safe mode: settings recovery only. No themes, no addons, no game.
    r.register("settings.get", P::Public, false, |c, _| async move {
        let s: Settings = store::load(&c.path(SETTINGS))?;
        Ok(serde_json::to_value(s)?)
    });
    r.register("safe.restore_theme", P::Internal, true, |c, _| async move {
        update_settings(&c, |s| s.theme = "default".into()).await
    });
    r.register(
        "safe.disable_addons",
        P::Internal,
        true,
        |c, _| async move { update_settings(&c, |s| s.addons_enabled = false).await },
    );
    r.register(
        "safe.reset_settings",
        P::Internal,
        true,
        |c, _| async move { update_settings(&c, |s| *s = Settings::default()).await },
    );
    if safe_mode {
        return;
    }

    // Shell look (wallpaper, glass, cover, palette) and motion. `look` is a partial patch.
    r.register("settings.set", P::Internal, false, |c, a| async move {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct A {
            look: Option<Value>,
            reduce_motion: Option<bool>,
            onboarded: Option<bool>,
        }
        let A {
            look,
            reduce_motion,
            onboarded,
        } = args(a)?;
        let _g = c.state_lock.lock().await;
        let mut s: Settings = store::load(&c.path(SETTINGS))?;
        if let Some(p) = look {
            let base = s.look.clone().unwrap_or_default();
            s.look = Some(base.patch(&p).map_err(CoreError::BadArgs)?);
        }
        if let Some(m) = reduce_motion {
            s.reduce_motion = m;
        }
        if let Some(o) = onboarded {
            s.onboarded = o;
        }
        store::save(&c.path(SETTINGS), &s, false)?;
        Ok(serde_json::to_value(s)?)
    });

    // Themes (M2): tokens + CSS + templates, checked in `themes.rs`. Not available in safe mode.
    r.register("themes.list", P::Public, false, |c, _| async move {
        Ok(json!(themes::list(&c.data_dir)))
    });
    r.register("themes.current", P::Public, false, |c, _| async move {
        let s: Settings = store::load(&c.path(SETTINGS))?;
        Ok(themes::current(&c.data_dir, &s.theme))
    });
    r.register("themes.set", P::Internal, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            theme: String,
        }
        let A { theme } = args(a)?;
        if themes::builtin(&theme).is_none() {
            themes::load(&c.data_dir, &theme).map_err(CoreError::BadArgs)?;
        }
        update_settings(&c, |s| s.theme = theme).await?;
        let s: Settings = store::load(&c.path(SETTINGS))?;
        Ok(themes::current(&c.data_dir, &s.theme))
    });
    r.register("themes.template", P::Public, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            screen: String,
        }
        let A { screen } = args(a)?;
        let s: Settings = store::load(&c.path(SETTINGS))?;
        // A broken template must never break the screen: fall back to the built-in one.
        let html = themes::template(&c.data_dir, &s.theme, &screen).unwrap_or(None);
        Ok(json!({ "screen": screen, "html": html }))
    });
    r.register("themes.folder", P::Internal, false, |c, _| async move {
        let d = themes::dir(&c.data_dir);
        std::fs::create_dir_all(&d)?;
        Ok(json!({ "path": d }))
    });

    // Addons (M3): checked in `addons.rs`; the user approves permissions here.
    r.register("addons.list", P::Internal, false, |c, _| async move {
        let st: addons::State = store::load(&c.path(addons::STATE))?;
        Ok(json!(addons::list(&c.data_dir, &st)))
    });
    r.register("addons.approve", P::Internal, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        let m = addons::load(&c.data_dir, &id).map_err(CoreError::BadArgs)?;
        update_addons(&c, |st| {
            let e = st.addons.entry(id).or_default();
            e.granted = m
                .permissions
                .iter()
                .copied()
                .filter(|p| *p != P::Public)
                .collect();
            e.enabled = true;
        })
        .await
    });
    r.register(
        "addons.set_enabled",
        P::Internal,
        false,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                id: String,
                enabled: bool,
            }
            let A { id, enabled } = args(a)?;
            update_addons(&c, |st| {
                if let Some(e) = st.addons.get_mut(&id) {
                    e.enabled = enabled;
                }
            })
            .await
        },
    );
    r.register("addons.revoke", P::Internal, false, |c, a| async move {
        let IdArg { id } = args(a)?;
        update_addons(&c, |st| {
            st.addons.remove(&id);
        })
        .await
    });
    r.register("addons.folder", P::Internal, false, |c, _| async move {
        let d = addons::dir(&c.data_dir);
        std::fs::create_dir_all(&d)?;
        Ok(json!({ "path": d }))
    });

    r.register("versions.list", P::Public, true, |c, a| async move {
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct A {
            snapshots: bool,
        }
        let A { snapshots } = args(a)?;
        versions::list(&c, snapshots).await
    });

    r.register("accounts.list", P::AccountsRead, true, |c, _| async move {
        let s: AccountStore = crate::accounts::load_store(&c.path(ACCOUNTS))?;
        Ok(s.public_view())
    });
    // Skin PNG of an account as a data URL (`null` → UI draws the default head).
    r.register("accounts.skin", P::AccountsRead, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            id: String,
        }
        let A { id } = args(a)?;
        let s: AccountStore = crate::accounts::load_store(&c.path(ACCOUNTS))?;
        let acc = s
            .accounts
            .iter()
            .find(|x| x.id == id)
            .ok_or_else(|| CoreError::BadArgs("unknown account".into()))?;
        let png = crate::skins::skin_png(&c, acc).await?;
        Ok(json!({
            "data_url": png.map(|p| format!("data:image/png;base64,{}", crate::skins::b64_encode(&p)))
        }))
    });
    r.register(
        "accounts.select",
        P::AccountsManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                id: String,
            }
            let A { id } = args(a)?;
            let _g = c.state_lock.lock().await;
            let mut s: AccountStore = crate::accounts::load_store(&c.path(ACCOUNTS))?;
            if !s.accounts.iter().any(|x| x.id == id) {
                return Err(CoreError::BadArgs("unknown account".into()));
            }
            s.selected = Some(id);
            crate::accounts::save_store(&c.path(ACCOUNTS), &s)?;
            Ok(s.public_view())
        },
    );
    r.register(
        "accounts.remove",
        P::AccountsManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                id: String,
            }
            let A { id } = args(a)?;
            let _g = c.state_lock.lock().await;
            let mut s: AccountStore = crate::accounts::load_store(&c.path(ACCOUNTS))?;
            s.remove(&id);
            crate::accounts::save_store(&c.path(ACCOUNTS), &s)?;
            Ok(s.public_view())
        },
    );
    r.register(
        "accounts.microsoft.begin",
        P::AccountsManage,
        true,
        |c, _| async move { msauth::begin(&c).await },
    );
    r.register(
        "accounts.microsoft.wait",
        P::AccountsManage,
        false,
        |c, _| async move {
            let acc = msauth::wait(&c).await?;
            let _g = c.state_lock.lock().await;
            let mut s: AccountStore = crate::accounts::load_store(&c.path(ACCOUNTS))?;
            let id = s.upsert(acc);
            s.selected = Some(id);
            crate::accounts::save_store(&c.path(ACCOUNTS), &s)?;
            Ok(s.public_view())
        },
    );
    r.register("accounts.offline.create", P::AccountsManage, true, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            name: String,
        }
        let A { name } = args(a)?;
        accounts::validate_profile_name(&name)?;
        let _g = c.state_lock.lock().await;
        let mut s: AccountStore = crate::accounts::load_store(&c.path(ACCOUNTS))?;
        if !s.offline_available() {
            return Err(CoreError::Denied(
                "offline profiles become available after signing in with a Microsoft account that has Minecraft: Java Edition".into(),
            ));
        }
        let id = s.upsert(Account {
            id: uuid::Uuid::new_v4().to_string(),
            kind: AccountKind::Offline,
            uuid: accounts::offline_uuid(&name),
            name,
            owns_game: false,
            secrets: None,
        });
        s.selected = Some(id);
        crate::accounts::save_store(&c.path(ACCOUNTS), &s)?;
        Ok(s.public_view())
    });
    // Account providers are signed addons (Extended tier), see `providers`.
    r.register("accounts.providers.list", P::AccountsRead, true, |c, _| async move {
        Ok(json!({ "providers": crate::providers::list(&c), "catalog_category": "account-providers" }))
    });
    crate::providers::register(r);

    r.register(
        "instances.list",
        P::InstancesRead,
        true,
        |c, _| async move {
            let _g = c.state_lock.lock().await;
            let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
            if migrate_dirs(&c, &mut s) {
                store::save(&c.path(INSTANCES), &s, false)?;
            }
            Ok(serde_json::to_value(s.instances)?)
        },
    );
    r.register(
        "instances.create",
        P::InstancesManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                name: String,
                version: String,
                #[serde(default)]
                loader: Option<String>,
                #[serde(default)]
                loader_version: Option<String>,
            }
            let A {
                name,
                version,
                loader,
                loader_version,
            } = args(a)?;
            instances::validate(&name, &version)?;
            let loader = loader.unwrap_or_else(|| "vanilla".into());
            if !instances::LOADERS.contains(&loader.as_str()) {
                return Err(CoreError::BadArgs(format!("unknown loader: {loader}")));
            }
            let loader_version = match (loader.as_str(), loader_version) {
                ("vanilla", _) => String::new(),
                (_, Some(v)) if !v.trim().is_empty() => v,
                _ => crate::game::loader::latest(&c, &loader, &version).await?,
            };
            let _g = c.state_lock.lock().await;
            let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
            let name = name.trim().to_owned();
            let inst = Instance {
                id: uuid::Uuid::new_v4().to_string(),
                dir: free_dir(&c, &s, &name, None),
                name,
                version,
                loader,
                loader_version,
                created_at: unix_now(),
                last_played: 0,
                playtime_secs: 0,
                settings: Default::default(),
            };
            std::fs::create_dir_all(inst.game_dir(&c.data_dir))?;
            s.instances.push(inst.clone());
            store::save(&c.path(INSTANCES), &s, false)?;
            Ok(serde_json::to_value(inst)?)
        },
    );
    register_instance_manage(r);
    register_game(r);
    register_mods(r);
    register_import(r);
}

fn register_import(r: &mut ActionRegistry) {
    // Creates an instance from a pack staged by the app (`stage_mrpack`).
    // Progress is polled with `game.progress { instance: <token> }`.
    r.register(
        "instances.import_mrpack",
        P::InstancesManage,
        false,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                token: String,
            }
            let A { token } = args(a)?;
            let path = mrpack::staged(&c.data_dir, &token)?;
            let res = import_mrpack(&c, &token, &path).await;
            let _ = std::fs::remove_file(&path);
            res
        },
    );
}

async fn import_mrpack(c: &Core, token: &str, path: &std::path::Path) -> Result<Value, CoreError> {
    let file = std::fs::File::open(path)
        .map_err(|_| CoreError::BadArgs("Файл сборки не найден, выберите его снова".into()))?;
    let mut z = zip::ZipArchive::new(file)
        .map_err(|_| CoreError::BadArgs("Это не сборка Modrinth (.mrpack)".into()))?;
    let p = service::progress_for(c, token);
    p.set_stage("prepare");
    // Paths stay relative until the folder is known.
    let mut plan = mrpack::plan(mrpack::read_index(&mut z)?, std::path::Path::new(""))?;
    instances::validate(&plan.name, &plan.mc)?;
    if plan.loader != "vanilla" && plan.loader_version.is_empty() {
        plan.loader_version = crate::game::loader::latest(c, &plan.loader, &plan.mc).await?;
    }
    let dir = {
        let _g = c.state_lock.lock().await;
        let s: InstanceStore = store::load(&c.path(INSTANCES))?;
        let dir = free_dir(c, &s, &plan.name, None);
        std::fs::create_dir_all(c.data_dir.join("instances").join(&dir))?;
        dir
    };
    let game_dir = c.data_dir.join("instances").join(&dir);
    if let Err(e) = fill_from_pack(c, &mut z, &mut plan, &game_dir, &p).await {
        p.set_stage("error");
        let _ = std::fs::remove_dir_all(&game_dir);
        return Err(e);
    }
    let _g = c.state_lock.lock().await;
    let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
    let inst = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        dir,
        name: plan.name,
        version: plan.mc,
        loader: plan.loader,
        loader_version: plan.loader_version,
        created_at: unix_now(),
        last_played: 0,
        playtime_secs: 0,
        settings: Default::default(),
    };
    s.instances.push(inst.clone());
    store::save(&c.path(INSTANCES), &s, false)?;
    p.set_stage("done");
    Ok(serde_json::to_value(inst)?)
}

async fn fill_from_pack(
    c: &Core,
    z: &mut zip::ZipArchive<std::fs::File>,
    plan: &mut mrpack::Plan,
    game_dir: &std::path::Path,
    p: &std::sync::Arc<crate::game::download::Progress>,
) -> Result<(), CoreError> {
    let mut jars = Vec::new();
    let tasks: Vec<_> = std::mem::take(&mut plan.tasks)
        .into_iter()
        .map(|mut t| {
            let name = t.path.file_name().map(|n| n.to_string_lossy().into_owned());
            if let Some(name) = name.filter(|n| {
                t.path.parent() == Some(std::path::Path::new("mods")) && mods::check_name(n).is_ok()
            }) {
                jars.push((name, t.sha1.clone()));
            }
            t.path = game_dir.join(&t.path);
            t
        })
        .collect();
    p.set_stage("files");
    crate::game::download::fetch_all(c.http(), tasks, std::sync::Arc::clone(p)).await?;
    p.set_stage("overrides");
    mrpack::extract_overrides(z, game_dir)?;
    // Best effort: without it the mods still work, just without icons/updates.
    let _ = mods::index_known(c, game_dir, &jars).await;
    Ok(())
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            std::fs::copy(e.path(), dest)?;
        }
    }
    Ok(())
}

/// A readable folder name for `name` that no other instance or existing folder uses.
fn free_dir(c: &Core, s: &InstanceStore, name: &str, own: Option<&str>) -> String {
    let root = c.data_dir.join("instances");
    let mut taken: Vec<String> = s
        .instances
        .iter()
        .filter(|i| Some(i.id.as_str()) != own)
        .map(|i| i.dir_name().to_lowercase())
        .collect();
    if let Ok(rd) = std::fs::read_dir(&root) {
        let own_dir = own
            .and_then(|id| s.instances.iter().find(|i| i.id == id))
            .map(|i| i.dir_name().to_lowercase());
        taken.extend(
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .filter(|n| Some(n) != own_dir.as_ref()),
        );
    }
    // Windows folders are case-insensitive, so compare lowercased.
    instances::unique_folder(&instances::folder_base(name), |d| {
        taken.contains(&d.to_lowercase())
    })
}

/// Gives a folder its new name; on failure (folder open elsewhere) keeps the old one.
fn move_dir(c: &Core, inst: &mut Instance, new_dir: String) {
    if new_dir == inst.dir_name() {
        return;
    }
    let from = inst.game_dir(&c.data_dir);
    let to = c.data_dir.join("instances").join(&new_dir);
    if !from.exists() || std::fs::rename(&from, &to).is_ok() {
        inst.dir = new_dir;
    }
}

/// Older instances live in `instances/<uuid>`; move them to readable names once.
fn migrate_dirs(c: &Core, s: &mut InstanceStore) -> bool {
    let mut changed = false;
    for k in 0..s.instances.len() {
        if !s.instances[k].dir.is_empty() || c.game_running.is_busy(&s.instances[k].id) {
            continue;
        }
        let (id, name) = (s.instances[k].id.clone(), s.instances[k].name.clone());
        let dir = free_dir(c, s, &name, Some(&id));
        move_dir(c, &mut s.instances[k], dir);
        changed |= !s.instances[k].dir.is_empty();
    }
    changed
}

fn busy(c: &Core, id: &str) -> Result<(), CoreError> {
    if c.game_running.is_busy(id) {
        return Err(CoreError::msg("close the game first"));
    }
    Ok(())
}

fn register_instance_manage(r: &mut ActionRegistry) {
    // Rename and/or replace per-instance settings.
    r.register(
        "instances.update",
        P::InstancesManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                instance: String,
                #[serde(default)]
                name: Option<String>,
                #[serde(default)]
                settings: Option<instances::InstanceSettings>,
            }
            let A {
                instance,
                name,
                settings,
            } = args(a)?;
            if let Some(s) = &settings {
                s.validate()?;
            }
            let _g = c.state_lock.lock().await;
            let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
            let i = s
                .instances
                .iter_mut()
                .find(|i| i.id == instance)
                .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {instance}")))?;
            if let Some(n) = &name {
                instances::validate(n, &i.version)?;
                i.name = n.trim().to_owned();
            }
            if let Some(set) = settings {
                i.settings = set;
            }
            let k = s
                .instances
                .iter()
                .position(|i| i.id == instance)
                .expect("found above");
            // Renaming also renames the folder, unless the game is using it right now.
            if name.is_some() && !c.game_running.is_busy(&instance) {
                let dir = free_dir(&c, &s, &s.instances[k].name.clone(), Some(&instance));
                move_dir(&c, &mut s.instances[k], dir);
            }
            let out = serde_json::to_value(&s.instances[k])?;
            store::save(&c.path(INSTANCES), &s, false)?;
            Ok(out)
        },
    );
    r.register(
        "instances.delete",
        P::InstancesManage,
        false,
        |c, a| async move {
            let InstanceArg { instance } = args(a)?;
            busy(&c, &instance)?;
            let _g = c.state_lock.lock().await;
            let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
            let k = s
                .instances
                .iter()
                .position(|i| i.id == instance)
                .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {instance}")))?;
            let dir = s.instances[k].game_dir(&c.data_dir);
            // Move the folder aside first: if a file is locked, nothing is lost and the
            // instance stays. Only then forget it and remove the files.
            let trash = c
                .data_dir
                .join("instances")
                .join(format!(".deleting-{instance}"));
            if dir.exists() {
                std::fs::rename(&dir, &trash).map_err(|e| {
                    CoreError::msg(format!(
                        "can't delete the folder (is it open in another program?): {e}"
                    ))
                })?;
            }
            s.instances.remove(k);
            store::save(&c.path(INSTANCES), &s, false)?;
            let cleaned = !trash.exists() || std::fs::remove_dir_all(&trash).is_ok();
            Ok(json!({ "instance": instance, "deleted": true, "files_removed": cleaned }))
        },
    );
    // Copies settings and the game folder (worlds, mods, configs).
    r.register(
        "instances.duplicate",
        P::InstancesManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                instance: String,
                #[serde(default)]
                name: Option<String>,
            }
            let A { instance, name } = args(a)?;
            busy(&c, &instance)?;
            let _g = c.state_lock.lock().await;
            let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
            let src = s
                .instances
                .iter()
                .find(|i| i.id == instance)
                .cloned()
                .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {instance}")))?;
            let src_dir = src.game_dir(&c.data_dir);
            let name = name.unwrap_or_else(|| format!("{} (2)", src.name));
            instances::validate(&name, &src.version)?;
            let name = name.trim().to_owned();
            let copy = Instance {
                id: uuid::Uuid::new_v4().to_string(),
                dir: free_dir(&c, &s, &name, None),
                name,
                created_at: unix_now(),
                last_played: 0,
                playtime_secs: 0,
                ..src
            };
            let from = src_dir;
            if from.is_dir() {
                copy_dir(&from, &copy.game_dir(&c.data_dir))?;
            } else {
                std::fs::create_dir_all(copy.game_dir(&c.data_dir))?;
            }
            s.instances.push(copy.clone());
            store::save(&c.path(INSTANCES), &s, false)?;
            Ok(serde_json::to_value(copy)?)
        },
    );
    // The UI opens it with the shell (Tauri opener).
    r.register(
        "instances.folder",
        P::InstancesRead,
        true,
        |c, a| async move {
            let InstanceArg { instance } = args(a)?;
            let s: InstanceStore = store::load(&c.path(INSTANCES))?;
            let dir = s
                .instances
                .iter()
                .find(|i| i.id == instance)
                .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {instance}")))?
                .game_dir(&c.data_dir);
            std::fs::create_dir_all(&dir)?;
            Ok(json!({ "instance": instance, "path": dir }))
        },
    );
}

fn register_game(r: &mut ActionRegistry) {
    // `game.install` resolves when done; the UI polls `game.progress` meanwhile.
    r.register("game.install", P::GameLaunch, true, |c, a| async move {
        let InstallArg { instance, verify } = args(a)?;
        service::install_instance(&c, &instance, verify).await
    });
    r.register(
        "game.progress",
        P::InstancesRead,
        false,
        |c, a| async move {
            let InstanceArg { instance } = args(a)?;
            Ok(service::progress(&c, &instance))
        },
    );
    r.register("game.launch", P::GameLaunch, true, |c, a| async move {
        let InstanceArg { instance } = args(a)?;
        service::launch_instance(&c, &instance).await
    });
    r.register(
        "game.running",
        P::InstancesRead,
        false,
        |c, _a| async move { Ok(service::running(&c).await) },
    );
    r.register("game.stop", P::GameLaunch, true, |c, a| async move {
        let InstanceArg { instance } = args(a)?;
        Ok(service::stop(&c, &instance).await)
    });
}

async fn update_settings(c: &Core, f: impl FnOnce(&mut Settings)) -> Result<Value, CoreError> {
    let _g = c.state_lock.lock().await;
    let mut s: Settings = store::load(&c.path(SETTINGS))?;
    f(&mut s);
    store::save(&c.path(SETTINGS), &s, false)?;
    Ok(serde_json::to_value(s)?)
}

async fn update_addons(c: &Core, f: impl FnOnce(&mut addons::State)) -> Result<Value, CoreError> {
    let _g = c.state_lock.lock().await;
    let mut st: addons::State = store::load(&c.path(addons::STATE))?;
    f(&mut st);
    store::save(&c.path(addons::STATE), &st, false)?;
    Ok(json!(addons::list(&c.data_dir, &st)))
}

fn instance_by_id(c: &Core, id: &str) -> Result<Instance, CoreError> {
    let s: InstanceStore = store::load(&c.path(INSTANCES))?;
    s.instances
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| CoreError::BadArgs(format!("no such instance: {id}")))
}

#[derive(Deserialize)]
struct ModArg {
    instance: String,
    file: String,
    #[serde(default)]
    enabled: bool,
}

fn register_mods(r: &mut ActionRegistry) {
    // Modrinth search, already filtered to the instance's game version and loader.
    r.register("mods.search", P::Network, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            instance: String,
            #[serde(default)]
            query: String,
            #[serde(default)]
            offset: u32,
        }
        let A {
            instance,
            query,
            offset,
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        let Some(loader) = modrinth::loader_name(&i.loader) else {
            return Ok(json!({ "supported": false, "hits": [], "total_hits": 0 }));
        };
        let page = modrinth::search(&c, &query, &i.version, loader, offset, 20).await?;
        Ok(json!({ "supported": true, "hits": page.hits, "total_hits": page.total_hits }))
    });
    r.register("mods.list", P::InstancesRead, false, |c, a| async move {
        let InstanceArg { instance } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        Ok(serde_json::to_value(mods::list(&i.game_dir(&c.data_dir))?)?)
    });
    // Installs a project plus its required dependencies; resolves when done.
    r.register(
        "mods.install",
        P::InstancesManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                instance: String,
                project: String,
            }
            let A { instance, project } = args(a)?;
            let i = instance_by_id(&c, &instance)?;
            let loader = modrinth::loader_name(&i.loader)
                .ok_or_else(|| CoreError::BadArgs("vanilla instances take no mods".into()))?;
            if c.game_running.is_busy(&i.id) {
                return Err(CoreError::msg("Сначала закройте игру"));
            }
            let added =
                mods::install(&c, &i.game_dir(&c.data_dir), &i.version, loader, &project).await?;
            Ok(json!({ "added": added }))
        },
    );
    r.register("mods.toggle", P::InstancesManage, true, |c, a| async move {
        let ModArg {
            instance,
            file,
            enabled,
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        if c.game_running.is_busy(&i.id) {
            return Err(CoreError::msg("Сначала закройте игру"));
        }
        mods::set_enabled(&i.game_dir(&c.data_dir), &file, enabled)?;
        Ok(json!({ "file": file, "enabled": enabled }))
    });
    r.register("mods.remove", P::InstancesManage, true, |c, a| async move {
        let ModArg { instance, file, .. } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        if c.game_running.is_busy(&i.id) {
            return Err(CoreError::msg("Сначала закройте игру"));
        }
        mods::remove(&i.game_dir(&c.data_dir), &file)?;
        Ok(json!({ "file": file }))
    });
    // Newer fitting versions of installed Modrinth mods.
    r.register("mods.updates", P::Network, false, |c, a| async move {
        let InstanceArg { instance } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        let Some(loader) = modrinth::loader_name(&i.loader) else {
            return Ok(json!([]));
        };
        let u = mods::updates(&c, &i.game_dir(&c.data_dir), &i.version, loader).await?;
        Ok(serde_json::to_value(u)?)
    });
    // Updates one mod (`file`) or, without it, every mod that has an update.
    r.register("mods.update", P::InstancesManage, true, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            instance: String,
            #[serde(default)]
            file: Option<String>,
        }
        let A { instance, file } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        let loader = modrinth::loader_name(&i.loader)
            .ok_or_else(|| CoreError::BadArgs("vanilla instances take no mods".into()))?;
        if c.game_running.is_busy(&i.id) {
            return Err(CoreError::msg("Сначала закройте игру"));
        }
        let gd = i.game_dir(&c.data_dir);
        let done = mods::apply_updates(&c, &gd, &i.version, loader, file.as_deref()).await?;
        Ok(json!({ "updated": done }))
    });
}
