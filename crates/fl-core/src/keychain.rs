//! OS keychain for account secrets (Windows Credential Manager, macOS
//! Keychain). Elsewhere, or when the keychain refuses a write, secrets stay
//! in `accounts.json` with owner-only permissions.

use crate::accounts::Secrets;

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use super::Secrets;

    const SERVICE: &str = "freedom-launcher";
    pub const AVAILABLE: bool = true;

    // Two entries per account: Credential Manager caps one blob at 2.5 KB.
    fn entry(id: &str, part: &str) -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, &format!("{id}.{part}")).ok()
    }

    pub fn set(id: &str, s: &Secrets) -> bool {
        let put = |p: &str, v: &str| entry(id, p).is_some_and(|e| e.set_password(v).is_ok());
        put("refresh", &s.refresh_token)
            && put("mc", &format!("{}:{}", s.mc_expires_at, s.mc_access_token))
    }

    pub fn get(id: &str) -> Option<Secrets> {
        let read = |p: &str| entry(id, p)?.get_password().ok();
        let mc = read("mc")?;
        let (exp, token) = mc.split_once(':')?;
        Some(Secrets {
            refresh_token: read("refresh")?,
            mc_access_token: token.to_owned(),
            mc_expires_at: exp.parse().ok()?,
        })
    }

    pub fn delete(id: &str) {
        for p in ["refresh", "mc"] {
            if let Some(e) = entry(id, p) {
                let _ = e.delete_credential();
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::Secrets;

    pub const AVAILABLE: bool = false;
    pub fn set(_id: &str, _s: &Secrets) -> bool {
        false
    }
    pub fn get(_id: &str) -> Option<Secrets> {
        None
    }
    pub fn delete(_id: &str) {}
}

pub use imp::{delete, get, set, AVAILABLE};
