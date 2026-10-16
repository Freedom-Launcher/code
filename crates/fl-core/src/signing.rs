//! Package signatures (M3): the «Официальный» and «Проверено» badges. A package
//! folder (addon, later theme) carries `signature.json`: an Ed25519 signature by
//! a project key over the SHA-256 of every file in the folder plus the badge.
//! Any changed, added or removed file breaks it, and the package drops back to
//! «Сообщество». Private keys never live in the repository.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Trust;

pub const FILE: &str = "signature.json";
const DOMAIN: &str = "freedom-launcher/package-signature/v1";
const MAX_FILES: usize = 1024;
const MAX_BYTES: u64 = 64 << 20;

/// Project signing keys: (key id, Ed25519 public key in hex).
pub const KEYS: [(&str, &str); 1] = [(
    "fl-2026",
    "c2136aa3ffc0142d9417078d1b728046fe18dab565b0ba036e25eec24c3d47e3",
)];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sig {
    pub v: u32,
    pub key: String,
    pub badge: Trust,
    pub sig: String,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    let s = s.trim();
    if s.len() != 2 * N || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

type Listing = Vec<(String, u64, Option<SystemTime>)>;

/// Every regular file under `root` (relative, `/`-separated, sorted). `None`
/// for symlinks, odd entries, non-UTF-8 names or too many files.
fn files(root: &Path) -> Option<Listing> {
    let mut out = Vec::new();
    let mut stack = vec![String::new()];
    while let Some(rel) = stack.pop() {
        for e in fs::read_dir(root.join(&rel)).ok()? {
            let e = e.ok()?;
            let md = e.metadata().ok()?;
            let name = e.file_name().into_string().ok()?;
            let r = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if md.is_dir() {
                stack.push(r);
            } else if md.is_file() {
                out.push((r, md.len(), md.modified().ok()));
            } else {
                return None;
            }
            if out.len() > MAX_FILES {
                return None;
            }
        }
    }
    out.sort();
    Some(out)
}

fn badge_name(b: Trust) -> &'static str {
    match b {
        Trust::Official => "official",
        Trust::Verified => "verified",
        Trust::Community => "community",
    }
}

/// The exact bytes that get signed.
pub fn message(root: &Path, id: &str, badge: Trust) -> Option<Vec<u8>> {
    let list = files(root)?;
    if list.iter().map(|f| f.1).sum::<u64>() > MAX_BYTES {
        return None;
    }
    let mut m = format!("{DOMAIN}\n{}\n{id}\n", badge_name(badge)).into_bytes();
    for (rel, _, _) in list.iter().filter(|f| f.0 != FILE) {
        let d = Sha256::digest(fs::read(root.join(rel)).ok()?);
        m.extend_from_slice(format!("{}  {rel}\n", hex(&d)).as_bytes());
    }
    Some(m)
}

pub fn sign(
    root: &Path,
    id: &str,
    badge: Trust,
    key: &str,
    seed: &[u8; 32],
) -> Result<Sig, String> {
    if badge == Trust::Community {
        return Err("«Сообщество» не подписывается".into());
    }
    let msg = message(root, id, badge)
        .ok_or("папку нельзя подписать: ссылки, больше 1024 файлов или больше 64 МБ")?;
    let sig = SigningKey::from_bytes(seed).sign(&msg);
    Ok(Sig {
        v: 1,
        key: key.into(),
        badge,
        sig: hex(&sig.to_bytes()),
    })
}

/// `Ok(None)` unsigned, `Ok(Some(badge))` valid, `Err` broken signature.
pub fn check_with(root: &Path, id: &str, keys: &[(&str, &str)]) -> Result<Option<Trust>, String> {
    let Ok(raw) = fs::read(root.join(FILE)) else {
        return Ok(None);
    };
    let bad = || "signature.json: неверный формат".to_string();
    if raw.len() > 4096 {
        return Err(bad());
    }
    let s: Sig = serde_json::from_slice(&raw).map_err(|_| bad())?;
    if s.v != 1 || s.badge == Trust::Community {
        return Err(bad());
    }
    let (_, pk) = keys
        .iter()
        .find(|(k, _)| *k == s.key)
        .ok_or("неизвестный ключ подписи")?;
    let vk = unhex::<32>(pk)
        .and_then(|b| VerifyingKey::from_bytes(&b).ok())
        .ok_or("ключ повреждён")?;
    let sig = Signature::from_bytes(&unhex::<64>(&s.sig).ok_or_else(bad)?);
    let msg = message(root, id, s.badge).ok_or("файлы пакета нельзя проверить")?;
    vk.verify_strict(&msg, &sig)
        .map_err(|_| "подпись не совпадает с файлами".to_string())?;
    Ok(Some(s.badge))
}

/// Same as `check_with` against the project keys, cached while no file changes.
pub fn check(root: &Path, id: &str) -> Result<Option<Trust>, String> {
    type Cache = HashMap<PathBuf, (Listing, Result<Option<Trust>, String>)>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let Some(list) = files(root) else {
        return check_with(root, id, &KEYS);
    };
    let cache = CACHE.get_or_init(Default::default);
    if let Some((l, r)) = cache.lock().unwrap().get(root) {
        if *l == list {
            return r.clone();
        }
    }
    let r = check_with(root, id, &KEYS);
    cache
        .lock()
        .unwrap()
        .insert(root.to_path_buf(), (list, r.clone()));
    r
}

pub fn trust(root: &Path, id: &str) -> Trust {
    check(root, id).ok().flatten().unwrap_or(Trust::Community)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_tamper() {
        let d = std::env::temp_dir().join(format!("fl-sig-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(d.join("img")).unwrap();
        fs::write(d.join("addon.json"), r#"{"id":"demo","name":"Demo"}"#).unwrap();
        fs::write(d.join("img/a.png"), [1, 2, 3]).unwrap();
        let seed = [7u8; 32];
        let pk = hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes());
        let keys = [("test", pk.as_str())];
        assert_eq!(check_with(&d, "demo", &keys), Ok(None));
        let write = |s: &Sig| fs::write(d.join(FILE), serde_json::to_vec(s).unwrap()).unwrap();
        let s = sign(&d, "demo", Trust::Verified, "test", &seed).unwrap();
        write(&s);
        assert_eq!(check_with(&d, "demo", &keys), Ok(Some(Trust::Verified)));
        // Same files under another id, unknown key, upgraded badge: all broken.
        assert!(check_with(&d, "other", &keys).is_err());
        assert!(check_with(&d, "demo", &KEYS).is_err());
        write(&Sig {
            badge: Trust::Official,
            ..s.clone()
        });
        assert!(check_with(&d, "demo", &keys).is_err());
        write(&s);
        fs::write(d.join("img/b.js"), "x").unwrap();
        assert!(check_with(&d, "demo", &keys).is_err());
        fs::remove_file(d.join("img/b.js")).unwrap();
        assert!(check_with(&d, "demo", &keys).is_ok());
        fs::write(d.join("img/a.png"), [1, 2, 4]).unwrap();
        assert_eq!(
            check_with(&d, "demo", &keys),
            Err("подпись не совпадает с файлами".into())
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(unhex::<2>("0aFf"), Some([10, 255]));
        assert_eq!(unhex::<2>("+aff"), None);
        assert_eq!(unhex::<2>("0a"), None);
        assert!(unhex::<32>(KEYS[0].1).is_some());
    }
}
