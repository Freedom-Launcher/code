//! Account providers: addons that add sign-in through another service. Any
//! package may be one (the badge says how far to trust it, not whether it may);
//! the contract is deliberately narrow:
//!
//! - only an enabled addon with an approved `accounts_provide` grant and an
//!   `account_provider` block in its manifest is a provider; a package whose
//!   signature is broken (files changed after it was signed) is not, because a
//!   provider handles credentials;
//! - a provider can save, list and remove only its own accounts; signing in
//!   through it needs no Microsoft account (Microsoft stays the built-in default);
//! - its accounts launch only while all of the above still hold.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    accounts::{self, Account, AccountKind, AccountStore, Secrets},
    actions::ActionRegistry,
    addons, net, unix_now, Caller, Core, CoreError, Permission as P,
};

const ACCOUNTS: &str = "accounts.json";
pub const PREFIX: &str = "accounts.provider.";
const MAX_TOKEN: usize = 8192;
const MAX_PER_PROVIDER: usize = 16;

/// Display label when `id` is an active provider.
pub fn active(core: &Core, id: &str) -> Option<String> {
    let Some(Caller::Addon { granted, .. }) = core.addon_caller(id) else {
        return None;
    };
    if !granted.contains(&P::AccountsProvide) {
        return None;
    }
    // Unsigned is fine (the user approved it knowing it is «Сообщество»); a signature
    // that no longer matches the files is not.
    if crate::signing::check(&addons::dir(&core.data_dir).join(id), id).is_err() {
        return None;
    }
    addons::load(&core.data_dir, id)
        .ok()?
        .account_provider
        .map(|p| p.label)
}

/// authlib-injector API root of an active provider, if it declares one.
pub fn yggdrasil(core: &Core, id: &str) -> Option<String> {
    active(core, id)?;
    addons::load(&core.data_dir, id)
        .ok()?
        .account_provider?
        .yggdrasil
}

/// Active providers for the accounts screen.
pub fn list(core: &Core) -> Vec<Value> {
    addons::installed_ids(&core.data_dir)
        .into_iter()
        .filter_map(|id| {
            let label = active(core, &id)?;
            Some(json!({ "id": id, "label": label, "trust": addons::trust(&core.data_dir, &id) }))
        })
        .collect()
}

fn me(core: &Core, a: &Value) -> Result<String, CoreError> {
    let id = a
        .get(net::CALLER_KEY)
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::Denied("только для плагинов-провайдеров".into()))?;
    active(core, id)
        .ok_or_else(|| CoreError::Denied(format!("{id} не является провайдером аккаунтов")))?;
    Ok(id.to_owned())
}

fn own(s: &AccountStore, id: &str) -> Value {
    let v = s.public_view();
    let mine: Vec<Value> = v["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["kind"]["provider"] == id)
        .cloned()
        .collect();
    json!({ "accounts": mine })
}

fn is_mine(a: &Account, id: &str) -> bool {
    matches!(&a.kind, AccountKind::Provider { provider } if provider == id)
}

pub(crate) fn register(r: &mut ActionRegistry) {
    r.register(
        "accounts.provider.save",
        P::AccountsProvide,
        false,
        |c, a| async move {
            let id = me(&c, &a)?;
            #[derive(Deserialize)]
            struct A {
                name: String,
                uuid: String,
                token: String,
                expires_at: u64,
                /// The service's `clientToken`. With it the core validates and refreshes the
                /// token itself before each launch (`ygg`); without it the date rules.
                #[serde(default)]
                client_token: String,
            }
            let A {
                name,
                uuid,
                token,
                expires_at,
                client_token,
            } = serde_json::from_value(a).map_err(|e| CoreError::BadArgs(e.to_string()))?;
            accounts::validate_profile_name(&name)?;
            let uuid = uuid::Uuid::parse_str(&uuid)
                .ok()
                .filter(|u| !u.is_nil())
                .ok_or_else(|| CoreError::BadArgs("uuid: нужен UUID профиля".into()))?;
            if token.is_empty()
                || token.len() > MAX_TOKEN
                || !token.bytes().all(|b| b.is_ascii_graphic())
            {
                return Err(CoreError::BadArgs(
                    "token: от 1 до 8192 печатных ASCII-символов".into(),
                ));
            }
            if client_token.len() > 256 || !client_token.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(CoreError::BadArgs(
                    "client_token: до 256 печатных ASCII-символов".into(),
                ));
            }
            if expires_at <= unix_now() + 60 {
                return Err(CoreError::BadArgs("expires_at: токен уже истёк".into()));
            }
            let _g = c.state_lock.lock().await;
            let path = c.path(ACCOUNTS);
            let mut s = accounts::load_store(&path)?;
            let uuid = uuid.hyphenated().to_string();
            let exists = s.accounts.iter().any(|x| is_mine(x, &id) && x.uuid == uuid);
            if !exists && s.accounts.iter().filter(|x| is_mine(x, &id)).count() >= MAX_PER_PROVIDER
            {
                return Err(CoreError::BadArgs(
                    "слишком много аккаунтов у одного провайдера".into(),
                ));
            }
            s.upsert(Account {
                id: uuid::Uuid::new_v4().to_string(),
                kind: AccountKind::Provider {
                    provider: id.clone(),
                },
                name,
                uuid,
                owns_game: false,
                secrets: Some(Secrets {
                    refresh_token: client_token,
                    mc_access_token: token,
                    mc_expires_at: expires_at,
                }),
            });
            accounts::save_store(&path, &s)?;
            Ok(own(&s, &id))
        },
    );
    r.register(
        "accounts.provider.list",
        P::AccountsProvide,
        false,
        |c, a| async move {
            let id = me(&c, &a)?;
            Ok(own(&accounts::load_store(&c.path(ACCOUNTS))?, &id))
        },
    );
    r.register(
        "accounts.provider.remove",
        P::AccountsProvide,
        false,
        |c, a| async move {
            let id = me(&c, &a)?;
            let acc = a
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let _g = c.state_lock.lock().await;
            let path = c.path(ACCOUNTS);
            let mut s = accounts::load_store(&path)?;
            if !s.accounts.iter().any(|x| x.id == acc && is_mine(x, &id)) {
                return Err(CoreError::BadArgs(
                    "нет такого аккаунта у этого провайдера".into(),
                ));
            }
            s.remove(&acc);
            accounts::save_store(&path, &s)?;
            Ok(own(&s, &id))
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Trust;
    use std::{fs, time::Instant};

    #[tokio::test]
    async fn provider_contract() {
        let dir = std::env::temp_dir().join(format!("fl-prov-{}", uuid::Uuid::new_v4()));
        let c = Core::new(dir, Instant::now(), false).unwrap();
        let d = c.data_dir.join("addons").join("auth");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("index.html"), "<p>x</p>").unwrap();
        let m = json!({"id": "auth", "name": "Auth", "permissions": ["accounts_provide"],
            "account_provider": {"label": "Сервис X"}});
        fs::write(d.join("addon.json"), m.to_string()).unwrap();
        let st = json!({"addons": {"auth": {"enabled": true, "granted": ["accounts_provide"]}}});
        fs::write(c.path(addons::STATE), st.to_string()).unwrap();
        let addon = Caller::Addon {
            id: "auth".into(),
            granted: [P::AccountsProvide].into(),
        };
        let exp = unix_now() + 3600;
        let acc = json!({"name": "Steve_2", "uuid": "0f1e2d3c-4b5a-4978-8a69-5a4b3c2d1e0f",
            "token": "abc.def", "expires_at": exp});
        let save = || c.dispatch(&addon, "accounts.provider.save", acc.clone());
        let err = |r: Result<Value, CoreError>| r.unwrap_err().to_string();

        // Unsigned is fine: the badge is trust, not permission.
        assert!(active(&c, "auth").is_some());
        assert_eq!(addons::trust(&c.data_dir, "auth"), Trust::Community);
        let sig = crate::signing::sign(
            &d,
            "auth",
            Trust::Verified,
            "test",
            &crate::signing::TEST_SEED,
        )
        .unwrap();
        fs::write(d.join("signature.json"), serde_json::to_vec(&sig).unwrap()).unwrap();
        assert!(active(&c, "auth").is_some());

        // Signed: signing in through the provider needs no Microsoft account.
        assert!(!accounts::load_store(&c.path(ACCOUNTS))
            .map(|s| s.offline_available())
            .unwrap_or(false));
        let v = save().await.unwrap();
        assert_eq!(v["accounts"].as_array().unwrap().len(), 1);
        assert!(!v.to_string().contains("abc.def"), "token leaked: {v}");
        // Saving the same profile again updates it in place.
        assert_eq!(
            save().await.unwrap()["accounts"].as_array().unwrap().len(),
            1
        );
        // The service's client token is stored for the core's own refresh and never comes back.
        let mut with_client = acc.clone();
        with_client["client_token"] = json!("client-123");
        let v = c
            .dispatch(&addon, "accounts.provider.save", with_client)
            .await
            .unwrap();
        assert!(
            !v.to_string().contains("client-123"),
            "client token leaked: {v}"
        );
        let st = accounts::load_store(&c.path(ACCOUNTS)).unwrap();
        assert_eq!(
            st.accounts[0].secrets.as_ref().unwrap().refresh_token,
            "client-123"
        );
        let mut bad = acc.clone();
        bad["client_token"] = json!("has space");
        assert!(c
            .dispatch(&addon, "accounts.provider.save", bad)
            .await
            .is_err());
        // A Microsoft account added later is not the provider's to see or remove.
        let mut s = accounts::load_store(&c.path(ACCOUNTS)).unwrap();
        s.upsert(Account {
            id: "ms".into(),
            kind: AccountKind::Microsoft,
            name: "Owner".into(),
            uuid: "11111111-2222-4333-8444-555555555555".into(),
            owns_game: true,
            secrets: None,
        });
        accounts::save_store(&c.path(ACCOUNTS), &s).unwrap();
        assert_eq!(
            c.dispatch(&addon, "accounts.provider.list", json!({}))
                .await
                .unwrap()["accounts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        // Other callers cannot act as the provider.
        let mut spoof = acc.clone();
        spoof["$addon"] = "auth".into();
        let ui = c
            .dispatch(&Caller::BuiltinUi, "accounts.provider.save", spoof)
            .await;
        assert!(err(ui).contains("только для плагинов"));
        let bare = Caller::Addon {
            id: "auth".into(),
            granted: Default::default(),
        };
        assert!(c
            .dispatch(&bare, "accounts.provider.list", json!({}))
            .await
            .is_err());

        // A provider sees and removes only its own accounts.
        let all = c
            .dispatch(&Caller::BuiltinUi, "accounts.list", json!(null))
            .await
            .unwrap();
        assert_eq!(all["accounts"].as_array().unwrap().len(), 2);
        assert!(!all.to_string().contains("abc.def"));
        let ms = json!({"id": "ms"});
        assert!(c
            .dispatch(&addon, "accounts.provider.remove", ms)
            .await
            .is_err());
        let p = c
            .dispatch(&Caller::BuiltinUi, "accounts.providers.list", json!(null))
            .await
            .unwrap();
        assert_eq!(p["providers"][0]["label"], "Сервис X");

        // Any changed file breaks the signature and the provider goes away.
        fs::write(d.join("index.html"), "<p>y</p>").unwrap();
        assert!(active(&c, "auth").is_none());
        assert!(err(save().await).contains("не является провайдером"));
        let _ = fs::remove_dir_all(&c.data_dir);
    }
}
