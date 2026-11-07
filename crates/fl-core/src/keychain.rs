//! OS keychain for account secrets: Windows Credential Manager, macOS Keychain, and on
//! Linux the Secret Service (GNOME Keyring, KWallet, KeePassXC). Where there is none (a
//! bare window manager, a container), or when the keychain refuses a write, secrets stay
//! in `accounts.json` with owner-only permissions, so nothing is ever lost or blocked.
//! `FL_NO_KEYCHAIN=1` turns the keychain off (headless runs, tests).

use crate::accounts::Secrets;

/// Whether the keychain should be tried at all.
pub fn available() -> bool {
    !disabled(std::env::var("FL_NO_KEYCHAIN").ok().as_deref()) && imp::available()
}

fn disabled(env: Option<&str>) -> bool {
    env.is_some_and(|v| !v.is_empty() && v != "0")
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
mod imp {
    use super::Secrets;

    const SERVICE: &str = "freedom-launcher";

    /// Windows and macOS always have one. On Linux there must be a session bus to talk to
    /// (we never auto-launch one), and the Secret Service has to answer on it.
    pub fn available() -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::sync::OnceLock;
            static OK: OnceLock<bool> = OnceLock::new();
            *OK.get_or_init(|| {
                let bus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
                    || std::env::var_os("XDG_RUNTIME_DIR")
                        .is_some_and(|d| std::path::Path::new(&d).join("bus").exists());
                if !bus {
                    return false;
                }
                // A missing entry is a good answer: the service is there and just has nothing.
                matches!(
                    entry("probe", "probe").map(|e| e.get_password()),
                    Some(Ok(_)) | Some(Err(keyring::Error::NoEntry))
                )
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

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

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod imp {
    use super::Secrets;

    pub fn available() -> bool {
        false
    }
    pub fn set(_id: &str, _s: &Secrets) -> bool {
        false
    }
    pub fn get(_id: &str) -> Option<Secrets> {
        None
    }
    pub fn delete(_id: &str) {}
}

pub fn set(id: &str, s: &Secrets) -> bool {
    available() && imp::set(id, s)
}

pub fn get(id: &str) -> Option<Secrets> {
    if available() {
        imp::get(id)
    } else {
        None
    }
}

pub fn delete(id: &str) {
    if available() {
        imp::delete(id)
    }
}

#[cfg(test)]
mod tests {
    use super::disabled;

    #[test]
    fn the_off_switch() {
        assert!(disabled(Some("1")));
        assert!(disabled(Some("yes")));
        assert!(!disabled(Some("0")));
        assert!(!disabled(Some("")));
        assert!(!disabled(None));
    }
}
