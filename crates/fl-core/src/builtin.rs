//! Built-in actions. The built-in screens use exactly these, like any addon.

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::{
    accounts::{self, Account, AccountKind, AccountStore},
    actions::ActionRegistry,
    game::service,
    instances::{self, Instance, InstanceStore},
    msauth,
    settings::Settings,
    store, unix_now, versions, Core, CoreError, Permission as P,
};

#[derive(Deserialize)]
struct InstanceArg {
    instance: String,
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
    r.register("safe.restore_theme", P::Public, true, |c, _| async move {
        update_settings(&c, |s| s.theme = "default".into()).await
    });
    r.register("safe.disable_addons", P::Public, true, |c, _| async move {
        update_settings(&c, |s| s.addons_enabled = false).await
    });
    r.register("safe.reset_settings", P::Public, true, |c, _| async move {
        update_settings(&c, |s| *s = Settings::default()).await
    });
    if safe_mode {
        return;
    }

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
        let s: AccountStore = store::load(&c.path(ACCOUNTS))?;
        Ok(s.public_view())
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
            let mut s: AccountStore = store::load(&c.path(ACCOUNTS))?;
            if !s.accounts.iter().any(|x| x.id == id) {
                return Err(CoreError::BadArgs("unknown account".into()));
            }
            s.selected = Some(id);
            store::save(&c.path(ACCOUNTS), &s, true)?;
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
            let mut s: AccountStore = store::load(&c.path(ACCOUNTS))?;
            s.remove(&id);
            store::save(&c.path(ACCOUNTS), &s, true)?;
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
            let mut s: AccountStore = store::load(&c.path(ACCOUNTS))?;
            let id = s.upsert(acc);
            s.selected = Some(id);
            store::save(&c.path(ACCOUNTS), &s, true)?;
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
        let mut s: AccountStore = store::load(&c.path(ACCOUNTS))?;
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
        store::save(&c.path(ACCOUNTS), &s, true)?;
        Ok(s.public_view())
    });
    // Account providers are contributed by addons (Extended tier). The core has none.
    r.register("accounts.providers.list", P::AccountsRead, true, |_, _| async move {
        Ok(json!({ "providers": [], "catalog_category": "account-providers" }))
    });

    r.register(
        "instances.list",
        P::InstancesRead,
        true,
        |c, _| async move {
            let s: InstanceStore = store::load(&c.path(INSTANCES))?;
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
            }
            let A { name, version } = args(a)?;
            instances::validate(&name, &version)?;
            let _g = c.state_lock.lock().await;
            let mut s: InstanceStore = store::load(&c.path(INSTANCES))?;
            let inst = Instance {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.trim().to_owned(),
                version,
                created_at: unix_now(),
            };
            std::fs::create_dir_all(c.data_dir.join("instances").join(&inst.id))?;
            s.instances.push(inst.clone());
            store::save(&c.path(INSTANCES), &s, false)?;
            Ok(serde_json::to_value(inst)?)
        },
    );
    register_game(r);
}

fn register_game(r: &mut ActionRegistry) {
    // `game.install` resolves when done; the UI polls `game.progress` meanwhile.
    r.register("game.install", P::GameLaunch, true, |c, a| async move {
        let InstanceArg { instance } = args(a)?;
        service::install_instance(&c, &instance).await
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
}

async fn update_settings(c: &Core, f: impl FnOnce(&mut Settings)) -> Result<Value, CoreError> {
    let _g = c.state_lock.lock().await;
    let mut s: Settings = store::load(&c.path(SETTINGS))?;
    f(&mut s);
    store::save(&c.path(SETTINGS), &s, false)?;
    Ok(serde_json::to_value(s)?)
}
