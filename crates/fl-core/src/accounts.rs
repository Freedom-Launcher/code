use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::CoreError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountKind {
    Microsoft,
    /// Local profile. Only available once a Microsoft account with
    /// Minecraft: Java Edition is signed in.
    Offline,
    /// Provided by an account-provider addon.
    Provider {
        provider: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub kind: AccountKind,
    pub name: String,
    pub uuid: String,
    #[serde(default)]
    pub owns_game: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<Secrets>,
}

/// Never leaves the core. Kept in the OS keychain where available (see `keychain`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Secrets {
    pub refresh_token: String,
    pub mc_access_token: String,
    pub mc_expires_at: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AccountStore {
    pub selected: Option<String>,
    pub accounts: Vec<Account>,
}

impl AccountStore {
    pub fn offline_available(&self) -> bool {
        self.accounts
            .iter()
            .any(|a| a.kind == AccountKind::Microsoft && a.owns_game)
    }

    pub fn upsert(&mut self, acc: Account) -> String {
        let id = if let Some(existing) = self
            .accounts
            .iter_mut()
            .find(|a| a.kind == acc.kind && a.uuid == acc.uuid)
        {
            let id = existing.id.clone();
            *existing = Account {
                id: id.clone(),
                ..acc
            };
            id
        } else {
            let id = acc.id.clone();
            self.accounts.push(acc);
            id
        };
        self.selected.get_or_insert_with(|| id.clone());
        id
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.accounts.len();
        self.accounts.retain(|a| a.id != id);
        crate::keychain::delete(id);
        if self.selected.as_deref() == Some(id) {
            self.selected = self.accounts.first().map(|a| a.id.clone());
        }
        before != self.accounts.len()
    }

    /// What the UI and addons may see: no secrets.
    pub fn public_view(&self) -> Value {
        json!({
            "selected": self.selected,
            "offline_available": self.offline_available(),
            "accounts": self.accounts.iter().map(|a| json!({
                "id": a.id,
                "kind": a.kind,
                "name": a.name,
                "uuid": a.uuid,
                "owns_game": a.owns_game,
            })).collect::<Vec<_>>(),
        })
    }
}

pub fn validate_profile_name(name: &str) -> Result<(), CoreError> {
    let ok = (3..=16).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(CoreError::BadArgs(
            "name must be 3-16 characters: latin letters, digits, _".into(),
        ))
    }
}

/// The UUID the game itself derives for a local profile name
/// (Java `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`).
pub fn offline_uuid(name: &str) -> String {
    let mut b = md5::compute(format!("OfflinePlayer:{name}")).0;
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(b).hyphenated().to_string()
}

/// Loads accounts, filling secrets from the keychain; secrets still in the
/// file (older versions) are moved there.
pub fn load_store(path: &std::path::Path) -> Result<AccountStore, crate::CoreError> {
    let mut s: AccountStore = crate::store::load(path)?;
    let migrate = crate::keychain::AVAILABLE && s.accounts.iter().any(|a| a.secrets.is_some());
    for a in &mut s.accounts {
        if a.secrets.is_none() {
            a.secrets = crate::keychain::get(&a.id);
        }
    }
    if migrate {
        save_store(path, &s)?;
    }
    Ok(s)
}

/// Saves accounts; secrets go to the keychain, the file keeps the rest.
pub fn save_store(path: &std::path::Path, s: &AccountStore) -> Result<(), crate::CoreError> {
    let mut disk = s.clone();
    for a in &mut disk.accounts {
        if a.secrets.as_ref().is_some_and(|sec| crate::keychain::set(&a.id, sec)) {
            a.secrets = None;
        }
    }
    crate::store::save(path, &disk, true)
}
