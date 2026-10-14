//! Skin images for account heads (chip, account menu).
//!
//! Microsoft accounts: the skin URL comes from the Minecraft profile (with a
//! valid token) or the public session server. The PNG is cached under
//! `<data_dir>/cache/skins/<uuid>.png` and the stale copy is used offline.
//! Offline/provider accounts return `None`; the UI draws the default head.

use std::{fs, time::Duration};

use serde_json::Value;

use crate::{
    accounts::{Account, AccountKind},
    msauth, unix_now, Core, CoreError,
};

const TTL: Duration = Duration::from_secs(6 * 3600);
const SESSION_URL: &str = "https://sessionserver.mojang.com/session/minecraft/profile";

/// The account's skin PNG, or `None` when it has no online skin.
pub async fn skin_png(core: &Core, acc: &Account) -> Result<Option<Vec<u8>>, CoreError> {
    if acc.kind != AccountKind::Microsoft || acc.uuid.is_empty() {
        return Ok(None);
    }
    let path = core
        .data_dir
        .join("cache")
        .join("skins")
        .join(format!("{}.png", acc.uuid.replace('-', "")));
    let fresh = fs::metadata(&path)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().unwrap_or_default() < TTL)
        .unwrap_or(false);
    if fresh {
        return Ok(Some(fs::read(&path)?));
    }
    match fetch(core, acc).await {
        Ok(Some(png)) => {
            crate::game::download::write_atomic(&path, &png)?;
            Ok(Some(png))
        }
        Ok(None) => Ok(None),
        Err(_) if path.exists() => Ok(Some(fs::read(&path)?)),
        Err(e) => Err(e),
    }
}

async fn fetch(core: &Core, acc: &Account) -> Result<Option<Vec<u8>>, CoreError> {
    let Some(url) = skin_url(core, acc).await? else {
        return Ok(None);
    };
    let url = url.replacen("http://", "https://", 1);
    let png = core.http().get(&url).send().await?.error_for_status()?.bytes().await?;
    if !png.starts_with(b"\x89PNG") {
        return Err(CoreError::msg("skin is not a PNG"));
    }
    Ok(Some(png.to_vec()))
}

async fn skin_url(core: &Core, acc: &Account) -> Result<Option<String>, CoreError> {
    let http = core.http();
    if let Some(sec) = acc.secrets.as_ref().filter(|s| s.mc_expires_at > unix_now() + 60) {
        let r = http
            .get(msauth::MC_PROFILE_URL)
            .bearer_auth(&sec.mc_access_token)
            .send()
            .await?;
        if r.status().is_success() {
            return Ok(active_skin(&r.json().await?));
        }
    }
    let id = acc.uuid.replace('-', "");
    let v: Value = http
        .get(format!("{SESSION_URL}/{id}"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(session_skin(&v))
}

/// `skins[]` entry with `state: ACTIVE` from the Minecraft profile.
pub fn active_skin(profile: &Value) -> Option<String> {
    let skins = profile["skins"].as_array()?;
    let s = skins.iter().find(|s| s["state"] == "ACTIVE").or(skins.first())?;
    s["url"].as_str().map(str::to_owned)
}

/// Skin URL from the session server's base64 `textures` property.
pub fn session_skin(v: &Value) -> Option<String> {
    let prop = v["properties"]
        .as_array()?
        .iter()
        .find(|p| p["name"] == "textures")?;
    let json = b64_decode(prop["value"].as_str()?)?;
    let t: Value = serde_json::from_slice(&json).ok()?;
    t["textures"]["SKIN"]["url"].as_str().map(str::to_owned)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for ch in s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=') {
        let v = B64.iter().position(|&b| b == ch)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn base64_roundtrip() {
        for s in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            assert_eq!(b64_decode(&b64_encode(s.as_bytes())).unwrap(), s.as_bytes());
        }
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
    }

    #[test]
    fn skin_urls() {
        let p = json!({"skins": [
            {"state": "INACTIVE", "url": "http://t/a"},
            {"state": "ACTIVE", "url": "http://t/b"}]});
        assert_eq!(active_skin(&p).as_deref(), Some("http://t/b"));
        let tex = b64_encode(br#"{"textures":{"SKIN":{"url":"http://t/c"}}}"#);
        let s = json!({"properties": [{"name": "textures", "value": tex}]});
        assert_eq!(session_skin(&s).as_deref(), Some("http://t/c"));
    }
}