//! Built-in actions. The built-in screens use exactly these, like any addon.

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::modrinth::Kind;

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

/// Worlds in `saves/` (folders with a `level.dat`), last played first.
fn list_worlds(saves: &std::path::Path) -> Vec<Value> {
    let Ok(rd) = std::fs::read_dir(saves) else {
        return Vec::new();
    };
    let mut out: Vec<(u64, String)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e
                .file_name()
                .into_string()
                .ok()
                .filter(|n| valid_world(n))?;
            let m = std::fs::metadata(e.path().join("level.dat")).ok()?;
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            Some((t, name))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    out.into_iter()
        .map(|(t, name)| json!({ "id": name, "name": name, "last_played": t }))
        .collect()
}

/// Downloads a Modrinth modpack version (the newest release by default)
/// and installs it like an imported `.mrpack`.
async fn install_modpack(
    c: &Core,
    project: &str,
    version: Option<&str>,
    token: &str,
) -> Result<Value, CoreError> {
    let versions = modrinth::project_versions(c, project).await?;
    let v = match version {
        Some(id) => versions.iter().find(|v| v.id == id),
        None => versions
            .iter()
            .find(|v| v.version_type == "release")
            .or(versions.first()),
    }
    .ok_or_else(|| CoreError::msg("У сборки нет подходящей версии"))?;
    let is_pack = |f: &&modrinth::VersionFile| f.filename.ends_with(".mrpack");
    let f = v
        .files
        .iter()
        .filter(is_pack)
        .find(|f| f.primary)
        .or_else(|| v.files.iter().find(is_pack))
        .ok_or_else(|| CoreError::msg("В этой версии нет файла сборки"))?;
    if !f.url.starts_with("https://cdn.modrinth.com/") || f.size as usize > mrpack::MAX_PACK {
        return Err(CoreError::msg("Файл сборки не прошёл проверку"));
    }
    let bytes = c
        .http()
        .get(&f.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !f.hashes.sha1.is_empty()
        && !crate::game::download::sha1_hex(&bytes).eq_ignore_ascii_case(&f.hashes.sha1)
    {
        return Err(CoreError::msg("Файл сборки повреждён, попробуйте ещё раз"));
    }
    let staged = mrpack::stage(&c.data_dir, &bytes)?;
    let path = mrpack::staged(&c.data_dir, &staged)?;
    let res = import_mrpack(c, token, &path).await;
    let _ = std::fs::remove_file(&path);
    res
}

fn register_import(r: &mut ActionRegistry) {
    r.register(
        "instances.worlds",
        P::InstancesRead,
        false,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                instance: String,
            }
            let A { instance } = args(a)?;
            let i = instance_by_id(&c, &instance)?;
            Ok(json!(list_worlds(&i.game_dir(&c.data_dir).join("saves"))))
        },
    );
    // Installs a modpack from Modrinth as a new instance. `token` is a fresh
    // uuid from the UI; progress is polled with `game.progress { instance: token }`.
    r.register(
        "modpacks.install",
        P::InstancesManage,
        false,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                project: String,
                token: String,
                #[serde(default)]
                version: Option<String>,
            }
            let a: A = args(a)?;
            uuid::Uuid::parse_str(&a.token).map_err(|_| CoreError::BadArgs("bad token".into()))?;
            service::progress_for(&c, &a.token).set_stage("prepare");
            install_modpack(&c, &a.project, a.version.as_deref(), &a.token).await
        },
    );
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
    #[serde(default)]
    kind: Kind,
    #[serde(default)]
    world: Option<String>,
}

#[derive(Deserialize)]
struct KindArg {
    instance: String,
    #[serde(default)]
    kind: Kind,
    /// World folder under `saves/`, for data packs.
    #[serde(default)]
    world: Option<String>,
}

/// Folder whose `<kind folder>/` holds the content: the instance folder, or
/// for data packs the chosen world inside it.
fn content_root(
    c: &Core,
    i: &Instance,
    kind: Kind,
    world: Option<&str>,
) -> Result<std::path::PathBuf, CoreError> {
    let gd = i.game_dir(&c.data_dir);
    if kind != Kind::Datapack {
        return Ok(gd);
    }
    let w = world
        .filter(|w| valid_world(w))
        .ok_or_else(|| CoreError::BadArgs("Выберите мир для датапака".into()))?;
    let dir = gd.join("saves").join(w);
    if !dir.is_dir() {
        return Err(CoreError::BadArgs("Мир не найден".into()));
    }
    Ok(dir)
}

/// A plain folder name: no separators, no `..`, nothing hidden.
fn valid_world(w: &str) -> bool {
    !w.trim().is_empty()
        && w.len() <= 128
        && !w.starts_with('.')
        && !w.contains(['/', '\\', ':', '\0'])
}

/// Modrinth loaders the instance's content of `kind` must fit, or why it can't have any.
fn content_loaders(i: &Instance, kind: Kind) -> Result<Vec<&'static str>, CoreError> {
    kind.supports(&i.loader, &i.version).ok_or_else(|| {
        CoreError::BadArgs(match kind {
            Kind::Shader => "шейдерам нужна сборка с загрузчиком".into(),
            Kind::Datapack => "датапаки есть только с Minecraft 1.13".into(),
            Kind::Modpack => "сборка устанавливается как новая сборка".into(),
            _ => "в Vanilla моды недоступны".into(),
        })
    })
}

fn idle(c: &Core, i: &Instance) -> Result<(), CoreError> {
    if c.game_running.is_busy(&i.id) {
        return Err(CoreError::msg("Сначала закройте игру"));
    }
    Ok(())
}

fn project_arg(a: Value) -> Result<String, CoreError> {
    #[derive(Deserialize)]
    struct A {
        id: String,
    }
    let A { id } = args(a)?;
    if !modrinth::valid_project_id(&id) {
        return Err(CoreError::msg("Неверный проект"));
    }
    Ok(id)
}

fn register_mods(r: &mut ActionRegistry) {
    // Modrinth search for one content kind, already filtered to the
    // instance's game version and loader.
    r.register("mods.search", P::Network, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            #[serde(default)]
            instance: Option<String>,
            /// Filters when browsing without an instance.
            #[serde(default)]
            loader: Option<String>,
            #[serde(default)]
            mc: Option<String>,
            #[serde(default)]
            kind: Kind,
            #[serde(default)]
            query: String,
            #[serde(default)]
            offset: u32,
            #[serde(default)]
            categories: Vec<String>,
            #[serde(default)]
            sort: String,
        }
        let a: A = args(a)?;
        let (mc, loaders) = match a.instance.as_deref().filter(|s| !s.is_empty()) {
            Some(id) => {
                let i = instance_by_id(&c, id)?;
                let Some(l) = a.kind.supports(&i.loader, &i.version) else {
                    return Ok(json!({ "supported": false, "hits": [], "total_hits": 0 }));
                };
                (i.version, l)
            }
            None => {
                let mc = a.mc.filter(|m| m.len() <= 32).unwrap_or_default();
                let loaders: Vec<&'static str> = match a.kind {
                    Kind::Mod | Kind::Modpack => a
                        .loader
                        .as_deref()
                        .and_then(modrinth::loader_name)
                        .into_iter()
                        .collect(),
                    _ => Vec::new(),
                };
                (mc, loaders)
            }
        };
        let q = modrinth::Query {
            text: &a.query,
            kind: a.kind,
            mc: &mc,
            loaders: &loaders,
            categories: &a.categories,
            sort: &a.sort,
            offset: a.offset,
            limit: 20,
        };
        let page = modrinth::search(&c, &q).await?;
        Ok(json!({ "supported": true, "hits": page.hits, "total_hits": page.total_hits }))
    });
    // Category filters Modrinth offers for a kind, grouped by header.
    // The project page: Modrinth's project and every version, newest first.
    r.register("modrinth.project", P::Network, false, |c, a| async move {
        let id = project_arg(a)?;
        Ok(serde_json::to_value(modrinth::project_page(&c, &id).await?)?)
    });
    r.register("modrinth.versions", P::Network, false, |c, a| async move {
        let id = project_arg(a)?;
        Ok(serde_json::to_value(modrinth::project_versions(&c, &id).await?)?)
    });
    r.register("mods.categories", P::Network, false, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            #[serde(default)]
            kind: Kind,
        }
        let A { kind } = args(a)?;
        let tags: Vec<Value> = c
            .http()
            .get(format!("{}/tag/category", modrinth::API))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let out: Vec<Value> = tags
            .iter()
            .filter(|t| t["project_type"] == kind.category_type())
            .filter_map(|t| {
                let name = t["name"].as_str()?;
                modrinth::valid_category(name)
                    .then(|| json!({ "name": name, "header": t["header"].as_str().unwrap_or("") }))
            })
            .collect();
        Ok(json!(out))
    });
    r.register("mods.list", P::InstancesRead, false, |c, a| async move {
        let KindArg {
            instance,
            kind,
            world,
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        let root = content_root(&c, &i, kind, world.as_deref())?;
        Ok(serde_json::to_value(mods::list(&root, kind)?)?)
    });
    // Installs a project (a given version or the best fit; mods also bring
    // their required dependencies); resolves when done.
    r.register(
        "mods.install",
        P::InstancesManage,
        true,
        |c, a| async move {
            #[derive(Deserialize)]
            struct A {
                instance: String,
                project: String,
                #[serde(default)]
                kind: Kind,
                #[serde(default)]
                version: Option<String>,
                #[serde(default)]
                world: Option<String>,
            }
            let a: A = args(a)?;
            let i = instance_by_id(&c, &a.instance)?;
            let loaders = content_loaders(&i, a.kind)?;
            idle(&c, &i)?;
            let gd = content_root(&c, &i, a.kind, a.world.as_deref())?;
            let t = mods::Target {
                game_dir: &gd,
                kind: a.kind,
                mc: &i.version,
                loaders: &loaders,
            };
            let added = mods::install(&c, t, &a.project, a.version.as_deref()).await?;
            Ok(json!({ "added": added }))
        },
    );
    r.register("mods.toggle", P::InstancesManage, true, |c, a| async move {
        let ModArg {
            instance,
            file,
            enabled,
            kind,
            world,
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        idle(&c, &i)?;
        let root = content_root(&c, &i, kind, world.as_deref())?;
        mods::set_enabled(&root, kind, &file, enabled)?;
        Ok(json!({ "file": file, "enabled": enabled }))
    });
    r.register("mods.remove", P::InstancesManage, true, |c, a| async move {
        let ModArg {
            instance,
            file,
            kind,
            world,
            ..
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        idle(&c, &i)?;
        mods::remove(&content_root(&c, &i, kind, world.as_deref())?, kind, &file)?;
        Ok(json!({ "file": file }))
    });
    // Newer fitting versions of installed Modrinth content of one kind.
    r.register("mods.updates", P::Network, false, |c, a| async move {
        let KindArg {
            instance,
            kind,
            world,
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        let Some(loaders) = kind.supports(&i.loader, &i.version) else {
            return Ok(json!([]));
        };
        let gd = content_root(&c, &i, kind, world.as_deref())?;
        let t = mods::Target {
            game_dir: &gd,
            kind,
            mc: &i.version,
            loaders: &loaders,
        };
        Ok(serde_json::to_value(mods::updates(&c, t).await?)?)
    });
    // Updates one file or, without it, everything of that kind with an update.
    r.register("mods.update", P::InstancesManage, true, |c, a| async move {
        #[derive(Deserialize)]
        struct A {
            instance: String,
            #[serde(default)]
            kind: Kind,
            #[serde(default)]
            file: Option<String>,
            #[serde(default)]
            world: Option<String>,
        }
        let A {
            instance,
            kind,
            file,
            world,
        } = args(a)?;
        let i = instance_by_id(&c, &instance)?;
        let loaders = content_loaders(&i, kind)?;
        idle(&c, &i)?;
        let gd = content_root(&c, &i, kind, world.as_deref())?;
        let t = mods::Target {
            game_dir: &gd,
            kind,
            mc: &i.version,
            loaders: &loaders,
        };
        let done = mods::apply_updates(&c, t, file.as_deref()).await?;
        Ok(json!({ "updated": done }))
    });
}

#[cfg(test)]
mod world_tests {
    use super::*;

    #[test]
    fn worlds_are_plain_folders_with_level_dat() {
        assert!(valid_world("New World") && valid_world("Мир 2"));
        assert!(!valid_world("../x") && !valid_world(".hidden") && !valid_world("a/b"));
        assert!(!valid_world("C:x") && !valid_world("  "));
        let d = std::env::temp_dir().join(format!("fl-worlds-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("A")).unwrap();
        std::fs::write(d.join("A/level.dat"), b"x").unwrap();
        std::fs::create_dir_all(d.join("not-a-world")).unwrap();
        let w = list_worlds(&d);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0]["id"], "A");
        assert!(list_worlds(&d.join("missing")).is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
