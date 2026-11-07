//! Keeps a provider account's Yggdrasil token alive, inside the core.
//!
//! The plugin saves the token once (`accounts.provider.save`, with the service's
//! `client_token`) and never sees it again. Before a launch the core asks the
//! service whether the token still works (`/authserver/validate`) and, if not,
//! swaps it for a fresh one (`/authserver/refresh`), so the player is not sent
//! back to the plugin's sign-in every few days. Requests go only to the API root
//! the plugin's manifest declared (its host is in `hosts`), https only, public
//! addresses only (`net::post_json`).

use std::future::Future;

use reqwest::Url;
use serde_json::{json, Value};

use crate::{accounts::Account, addons, net, unix_now, Core, CoreError};

/// How long a token we just refreshed is trusted if the service is unreachable
/// next time. While it is reachable the service decides, not this number.
const REFRESHED_FOR: u64 = 24 * 3600;

pub(crate) trait Transport {
    fn post(
        &self,
        url: &Url,
        body: &Value,
    ) -> impl Future<Output = Result<(u16, Value), String>> + Send;
}

pub(crate) struct Http;

impl Transport for Http {
    async fn post(&self, url: &Url, body: &Value) -> Result<(u16, Value), String> {
        net::post_json(url, body).await
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Check {
    /// The service says the token works.
    Valid,
    /// It did not, and the service gave a new one.
    Refreshed(String),
    /// The service could not be asked (network down, 5xx): use what we have if it is still in date.
    Unreachable,
    /// The service refused: the player has to sign in again.
    SignInAgain,
}

fn endpoint(root: &str, op: &str, hosts: &[String]) -> Option<Url> {
    net::check_url(
        &format!("{}/authserver/{op}", root.trim_end_matches('/')),
        hosts,
    )
    .ok()
}

pub(crate) async fn check<T: Transport>(
    t: &T,
    root: &str,
    hosts: &[String],
    client_token: &str,
    access: &str,
) -> Check {
    let (Some(validate), Some(refresh)) = (
        endpoint(root, "validate", hosts),
        endpoint(root, "refresh", hosts),
    ) else {
        return Check::Unreachable;
    };
    let pair = json!({ "accessToken": access, "clientToken": client_token });
    match t.post(&validate, &pair).await {
        Ok((200 | 204, _)) => return Check::Valid,
        Ok((s, _)) if s >= 500 => return Check::Unreachable,
        Ok(_) => {}
        Err(_) => return Check::Unreachable,
    }
    let ask = json!({ "accessToken": access, "clientToken": client_token, "requestUser": false });
    match t.post(&refresh, &ask).await {
        Ok((200, body)) => match body["accessToken"].as_str() {
            Some(n)
                if !n.is_empty() && n.len() <= 8192 && n.bytes().all(|b| b.is_ascii_graphic()) =>
            {
                Check::Refreshed(n.to_owned())
            }
            _ => Check::SignInAgain,
        },
        Ok((s, _)) if s >= 500 => Check::Unreachable,
        Ok(_) => Check::SignInAgain,
        Err(_) => Check::Unreachable,
    }
}

/// The token to launch with for a provider account; saves a refreshed one.
pub(crate) async fn session<T: Transport>(
    core: &Core,
    t: &T,
    provider: &str,
    acc: &Account,
) -> Result<String, CoreError> {
    let sec = acc
        .secrets
        .as_ref()
        .ok_or_else(|| CoreError::msg("session expired, sign in through the addon again"))?;
    let in_date = sec.mc_expires_at > unix_now() + 60;
    let root = crate::providers::yggdrasil(core, provider);
    // Without a service root or a client token there is nothing to ask: the saved date rules.
    let (Some(root), false) = (root, sec.refresh_token.is_empty()) else {
        return if in_date {
            Ok(sec.mc_access_token.clone())
        } else {
            Err(CoreError::msg(
                "session expired, sign in through the addon again",
            ))
        };
    };
    let hosts = addons::load(&core.data_dir, provider)
        .map(|m| m.hosts)
        .unwrap_or_default();
    match check(t, &root, &hosts, &sec.refresh_token, &sec.mc_access_token).await {
        Check::Valid => Ok(sec.mc_access_token.clone()),
        Check::Refreshed(new) => {
            let _g = core.state_lock.lock().await;
            let path = core.path("accounts.json");
            let mut s = crate::accounts::load_store(&path)?;
            if let Some(a) = s.accounts.iter_mut().find(|a| a.id == acc.id) {
                if let Some(sec) = a.secrets.as_mut() {
                    sec.mc_access_token = new.clone();
                    sec.mc_expires_at = unix_now() + REFRESHED_FOR;
                }
                crate::accounts::save_store(&path, &s)?;
            }
            Ok(new)
        }
        Check::Unreachable if in_date => Ok(sec.mc_access_token.clone()),
        Check::Unreachable => Err(CoreError::msg(
            "cannot reach the sign-in service and the saved session has expired",
        )),
        Check::SignInAgain => Err(CoreError::msg(
            "session expired, sign in through the addon again",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{AccountKind, AccountStore, Secrets};
    use crate::{signing, Trust};
    use std::{collections::VecDeque, fs, sync::Mutex, time::Instant};

    /// Scripted answers, in order; records the URLs it was asked.
    struct Fake {
        script: Mutex<VecDeque<Result<(u16, Value), String>>>,
        seen: Mutex<Vec<String>>,
    }
    impl Fake {
        fn new(s: Vec<Result<(u16, Value), String>>) -> Self {
            Fake {
                script: Mutex::new(s.into()),
                seen: Mutex::new(vec![]),
            }
        }
        fn seen(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }
    impl Transport for Fake {
        async fn post(&self, url: &Url, _b: &Value) -> Result<(u16, Value), String> {
            self.seen.lock().unwrap().push(url.path().to_owned());
            self.script
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err("script ended".into()))
        }
    }

    const ROOT: &str = "https://auth.example.org/api/authlib-injector";
    fn hosts() -> Vec<String> {
        vec!["auth.example.org".into()]
    }
    fn ok(v: Value) -> Result<(u16, Value), String> {
        Ok((200, v))
    }

    #[tokio::test]
    async fn valid_token_is_kept_and_costs_one_request() {
        let t = Fake::new(vec![Ok((204, Value::Null))]);
        assert_eq!(check(&t, ROOT, &hosts(), "c", "a").await, Check::Valid);
        assert_eq!(t.seen(), vec!["/api/authlib-injector/authserver/validate"]);
    }

    #[tokio::test]
    async fn rejected_token_is_swapped_for_a_fresh_one() {
        let t = Fake::new(vec![
            Ok((403, Value::Null)),
            ok(json!({"accessToken": "NEW", "clientToken": "c"})),
        ]);
        assert_eq!(
            check(&t, ROOT, &hosts(), "c", "a").await,
            Check::Refreshed("NEW".into())
        );
        assert_eq!(t.seen().len(), 2);
        assert!(t.seen()[1].ends_with("/authserver/refresh"));
    }

    #[tokio::test]
    async fn refused_refresh_means_sign_in_again() {
        let t = Fake::new(vec![
            Ok((403, Value::Null)),
            Ok((403, json!({"error": "ForbiddenOperationException"}))),
        ]);
        assert_eq!(
            check(&t, ROOT, &hosts(), "c", "a").await,
            Check::SignInAgain
        );
    }

    #[tokio::test]
    async fn unreachable_service_is_not_a_refusal() {
        let down = Fake::new(vec![Err("dns".into())]);
        assert_eq!(
            check(&down, ROOT, &hosts(), "c", "a").await,
            Check::Unreachable
        );
        let five = Fake::new(vec![Ok((503, Value::Null))]);
        assert_eq!(
            check(&five, ROOT, &hosts(), "c", "a").await,
            Check::Unreachable
        );
        let mid = Fake::new(vec![Ok((403, Value::Null)), Ok((502, Value::Null))]);
        assert_eq!(
            check(&mid, ROOT, &hosts(), "c", "a").await,
            Check::Unreachable
        );
    }

    #[tokio::test]
    async fn a_garbage_new_token_is_not_accepted() {
        for bad in [
            json!({"accessToken": ""}),
            json!({"accessToken": "a b"}),
            json!({"nope": 1}),
            json!({"accessToken": "ü"}),
        ] {
            let t = Fake::new(vec![Ok((403, Value::Null)), ok(bad)]);
            assert_eq!(
                check(&t, ROOT, &hosts(), "c", "a").await,
                Check::SignInAgain
            );
        }
    }

    #[tokio::test]
    async fn nothing_is_sent_to_a_host_the_plugin_did_not_declare() {
        let t = Fake::new(vec![Ok((204, Value::Null))]);
        let other = vec!["elsewhere.example.org".to_string()];
        assert_eq!(check(&t, ROOT, &other, "c", "a").await, Check::Unreachable);
        assert!(
            t.seen().is_empty(),
            "the token must not leave for an undeclared host"
        );
        // And never over plain http.
        assert_eq!(
            check(&t, "http://auth.example.org/x", &hosts(), "c", "a").await,
            Check::Unreachable
        );
        assert!(t.seen().is_empty());
    }

    // ---- through `session`: the stored account really changes ----

    fn core_with_ygg_provider() -> std::sync::Arc<Core> {
        let dir = std::env::temp_dir().join(format!("fl-ygg-{}", uuid::Uuid::new_v4()));
        let c = Core::new(dir, Instant::now(), false).unwrap();
        let d = c.data_dir.join("addons").join("ely");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("index.html"), "<p>x</p>").unwrap();
        let m = json!({"id": "ely", "name": "Ely", "permissions": ["accounts_provide", "network"],
            "hosts": ["auth.example.org"],
            "account_provider": {"label": "Ely", "yggdrasil": ROOT}});
        fs::write(d.join("addon.json"), m.to_string()).unwrap();
        let st = json!({"addons": {"ely": {"enabled": true, "granted": ["accounts_provide", "network"]}}});
        fs::write(c.path(crate::addons::STATE), st.to_string()).unwrap();
        let sig = signing::sign(&d, "ely", Trust::Verified, "test", &signing::TEST_SEED).unwrap();
        fs::write(d.join("signature.json"), serde_json::to_vec(&sig).unwrap()).unwrap();
        c
    }

    fn account(client_token: &str, expires_in: i64) -> Account {
        Account {
            id: "a1".into(),
            kind: AccountKind::Provider {
                provider: "ely".into(),
            },
            name: "Steve".into(),
            uuid: "0f1e2d3c-4b5a-4978-8a69-5a4b3c2d1e0f".into(),
            owns_game: false,
            secrets: Some(Secrets {
                refresh_token: client_token.into(),
                mc_access_token: "OLD".into(),
                mc_expires_at: (unix_now() as i64 + expires_in) as u64,
            }),
        }
    }

    fn store(c: &Core, a: &Account) {
        let mut s = AccountStore::default();
        s.upsert(a.clone());
        crate::accounts::save_store(&c.path("accounts.json"), &s).unwrap();
    }

    #[tokio::test]
    async fn expired_saved_date_no_longer_forces_a_new_sign_in() {
        let c = core_with_ygg_provider();
        let a = account("client", -3600);
        store(&c, &a);
        let t = Fake::new(vec![
            Ok((403, Value::Null)),
            ok(json!({"accessToken": "NEW"})),
        ]);
        assert_eq!(session(&c, &t, "ely", &a).await.unwrap(), "NEW");
        // The fresh token is what is stored now, with a new date.
        let s = crate::accounts::load_store(&c.path("accounts.json")).unwrap();
        let sec = s.accounts[0].secrets.as_ref().unwrap();
        assert_eq!(sec.mc_access_token, "NEW");
        assert!(sec.mc_expires_at > unix_now() + 3600);
        assert_eq!(sec.refresh_token, "client", "the client token is kept");
        let _ = fs::remove_dir_all(&c.data_dir);
    }

    #[tokio::test]
    async fn offline_with_an_expired_session_asks_to_sign_in_again() {
        let c = core_with_ygg_provider();
        let a = account("client", -3600);
        store(&c, &a);
        let t = Fake::new(vec![Err("offline".into())]);
        assert!(session(&c, &t, "ely", &a).await.is_err());
        // Offline but still in date: launch with what we have.
        let a = account("client", 3600);
        store(&c, &a);
        let t = Fake::new(vec![Err("offline".into())]);
        assert_eq!(session(&c, &t, "ely", &a).await.unwrap(), "OLD");
        let _ = fs::remove_dir_all(&c.data_dir);
    }

    #[tokio::test]
    async fn without_a_client_token_nothing_is_asked_and_the_date_rules() {
        let c = core_with_ygg_provider();
        let a = account("", 3600);
        store(&c, &a);
        let t = Fake::new(vec![]);
        assert_eq!(session(&c, &t, "ely", &a).await.unwrap(), "OLD");
        assert!(t.seen().is_empty());
        let a = account("", -10);
        assert!(session(&c, &t, "ely", &a).await.is_err());
        let _ = fs::remove_dir_all(&c.data_dir);
    }

    #[tokio::test]
    async fn refused_session_is_not_stored_or_used() {
        let c = core_with_ygg_provider();
        let a = account("client", 3600);
        store(&c, &a);
        let t = Fake::new(vec![Ok((403, Value::Null)), Ok((403, Value::Null))]);
        assert!(session(&c, &t, "ely", &a).await.is_err());
        let s = crate::accounts::load_store(&c.path("accounts.json")).unwrap();
        assert_eq!(
            s.accounts[0].secrets.as_ref().unwrap().mc_access_token,
            "OLD"
        );
        let _ = fs::remove_dir_all(&c.data_dir);
    }
}
