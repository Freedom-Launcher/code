//! Skin images for account heads (chip, account menu).
//!
//! Microsoft accounts: the skin URL comes from the Minecraft profile (with a
//! valid token) or the public session server. The PNG is cached under
//! `<data_dir>/cache/skins/<uuid>.png` and the stale copy is used offline.
//! Offline/provider accounts return `None`; the UI draws the default head.

use std::{fs, time::Duration};

use serde_json::{json, Value};

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
    let png = core
        .http()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !png.starts_with(b"\x89PNG") {
        return Err(CoreError::msg("skin is not a PNG"));
    }
    Ok(Some(png.to_vec()))
}

async fn skin_url(core: &Core, acc: &Account) -> Result<Option<String>, CoreError> {
    let http = core.http();
    if let Some(sec) = acc
        .secrets
        .as_ref()
        .filter(|s| s.mc_expires_at > unix_now() + 60)
    {
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
    let s = skins
        .iter()
        .find(|s| s["state"] == "ACTIVE")
        .or(skins.first())?;
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

// ---------- Changing the skin (official Minecraft services API) ----------

const TEX_HOST: &str = "https://textures.minecraft.net/";

/// A valid Minecraft token for the account, refreshed (and saved) when it is about to expire.
async fn token(core: &Core, id: &str) -> Result<(Account, String), CoreError> {
    let path = core.path("accounts.json");
    let mut s = crate::accounts::load_store(&path)?;
    let acc = s.accounts.iter_mut().find(|a| a.id == id)
        .ok_or_else(|| CoreError::BadArgs("unknown account".into()))?;
    if acc.kind != AccountKind::Microsoft {
        return Err(CoreError::msg("Скин меняется только у аккаунта Microsoft"));
    }
    let sec = acc.secrets.clone().ok_or_else(|| CoreError::msg("Войдите через Microsoft снова"))?;
    if sec.mc_expires_at > unix_now() + 60 {
        return Ok((acc.clone(), sec.mc_access_token));
    }
    let fresh = msauth::refresh(core, &sec.refresh_token).await?;
    acc.name = fresh.name;
    acc.owns_game = fresh.owns_game;
    acc.secrets = fresh.secrets;
    let t = acc.secrets.as_ref().map(|x| x.mc_access_token.clone()).unwrap_or_default();
    let out = acc.clone();
    crate::accounts::save_store(&path, &s)?;
    Ok((out, t))
}

fn data_url(png: &[u8]) -> String { format!("data:image/png;base64,{}", b64_encode(png)) }

async fn png_at(core: &Core, url: &str) -> Option<Vec<u8>> {
    let url = url.replacen("http://", "https://", 1);
    if !url.starts_with(TEX_HOST) { return None; }
    let b = core.http().get(&url).send().await.ok()?.error_for_status().ok()?.bytes().await.ok()?;
    b.starts_with(b"\x89PNG").then(|| b.to_vec())
}

/// Width and height from the PNG header.
pub fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || !b.starts_with(b"\x89PNG\r\n\x1a\n") || &b[12..16] != b"IHDR" { return None; }
    Some((u32::from_be_bytes(b[16..20].try_into().ok()?), u32::from_be_bytes(b[20..24].try_into().ok()?)))
}

pub fn check_skin(b: &[u8]) -> Result<(), CoreError> {
    match png_size(b) {
        Some((64, 64) | (64, 32)) => Ok(()),
        Some(_) => Err(CoreError::msg("Скин должен быть 64×64 или 64×32")),
        None => Err(CoreError::msg("Нужна картинка PNG")),
    }
}

/// `multipart/form-data` body for the upload (reqwest is built without multipart).
pub fn multipart(variant: &str, png: &[u8]) -> (String, Vec<u8>) {
    let b = format!("fl{}", uuid::Uuid::new_v4().simple());
    let mut body = format!("--{b}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{variant}\r\n--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{b}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={b}"), body)
}

/// Saved skins live in `skins/<variant>-<16hex>.png`, like the Modrinth App skin library.
pub fn valid_name(n: &str) -> bool {
    let Some(rest) = n.strip_prefix("classic-").or_else(|| n.strip_prefix("slim-")) else { return false };
    rest.len() == 20 && rest.ends_with(".png") && rest[..16].bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn lib_dir(core: &Core) -> std::path::PathBuf { core.data_dir.join("skins") }

fn keep(core: &Core, variant: &str, png: &[u8]) -> Result<String, CoreError> {
    use sha1::Digest;
    let h: String = sha1::Sha1::digest(png).iter().map(|b| format!("{b:02x}")).collect();
    let name = format!("{variant}-{}.png", &h[..16]);
    let path = lib_dir(core).join(&name);
    if !path.exists() { crate::game::download::write_atomic(&path, png)?; }
    Ok(name)
}

// ---------- Library metadata (skins/index.json) ----------

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Meta {
    /// Manual order of saved skins; the rest follow, newest first.
    #[serde(default)]
    order: Vec<String>,
    /// Cape worn with a saved skin: `Some(id)` or `None` (no cape). Missing = leave the cape alone.
    #[serde(default)]
    capes: std::collections::HashMap<String, Option<String>>,
    /// Mojang texture key the saved skin got when it was last put on (marks it as active).
    #[serde(default)]
    keys: std::collections::HashMap<String, String>,
}

fn meta_path(core: &Core) -> std::path::PathBuf { lib_dir(core).join("index.json") }
fn load_meta(core: &Core) -> Meta {
    fs::read(meta_path(core)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn save_meta(core: &Core, m: &Meta) -> Result<(), CoreError> {
    crate::game::download::write_atomic(&meta_path(core), &serde_json::to_vec(m)?)?;
    Ok(())
}

fn saved(core: &Core) -> Vec<Value> {
    let m = load_meta(core);
    let mut v: Vec<_> = fs::read_dir(lib_dir(core)).into_iter().flatten().flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            let t = e.metadata().and_then(|m| m.modified()).ok()?;
            valid_name(&n).then_some((t, n))
        }).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.sort_by_key(|(_, n)| m.order.iter().position(|o| o == n).unwrap_or(usize::MAX));
    v.into_iter().take(96).filter_map(|(_, n)| {
        let png = fs::read(lib_dir(core).join(&n)).ok()?;
        let cape = m.capes.get(&n).cloned();
        Some(json!({ "name": n, "variant": n.split('-').next(), "img": data_url(&png), "key": m.keys.get(&n),
                     "has_cape": cape.is_some(), "cape": cape.flatten() }))
    }).collect()
}

async fn ok(r: reqwest::Response) -> Result<(), CoreError> {
    match r.status().as_u16() {
        200..=299 => Ok(()),
        429 => Err(CoreError::msg("Слишком много изменений подряд, подождите минуту")),
        401 | 403 => Err(CoreError::msg("Войдите через Microsoft снова")),
        400 => Err(CoreError::msg("Mojang не принял этот скин")),
        s => Err(CoreError::msg(format!("Сервис скинов ответил {s}"))),
    }
}

/// Drops the cached head so the chip and menu pick up the new skin.
fn forget_head(core: &Core, acc: &Account) {
    let _ = fs::remove_file(core.data_dir.join("cache").join("skins").join(format!("{}.png", acc.uuid.replace('-', ""))));
}
fn api(path: &str) -> String { format!("{}{path}", msauth::MC_PROFILE_URL) }

/// Texture key: the last part of a textures.minecraft.net URL.
pub fn tex_key(url: &str) -> Option<String> {
    url.rsplit('/').next().filter(|k| k.len() >= 32 && k.bytes().all(|c| c.is_ascii_hexdigit())).map(str::to_owned)
}

async fn mc_profile(core: &Core, t: &str) -> Result<Value, CoreError> {
    let r = core.http().get(msauth::MC_PROFILE_URL).bearer_auth(t).send().await?;
    if r.status().as_u16() == 404 {
        return Err(CoreError::msg("У этого аккаунта нет профиля Minecraft"));
    }
    let r = r.error_for_status().map_err(|_| CoreError::msg("Войдите через Microsoft снова"))?;
    Ok(r.json().await?)
}

async fn render(core: &Core, acc: &Account, v: &Value) -> Result<Value, CoreError> {
    let skins = v["skins"].as_array().cloned().unwrap_or_default();
    let cur = skins.iter().find(|s| s["state"] == "ACTIVE").or(skins.first());
    let mut skin = Value::Null;
    if let Some(c) = cur {
        let url = c["url"].as_str().unwrap_or_default();
        let img = png_at(core, url).await.map(|p| data_url(&p));
        skin = json!({ "variant": c["variant"].as_str().unwrap_or("CLASSIC").to_lowercase(), "img": img, "key": tex_key(url) });
    }
    let capes = futures_util::future::join_all(v["capes"].as_array().cloned().unwrap_or_default().into_iter().map(|c| async move {
        let img = png_at(core, c["url"].as_str().unwrap_or_default()).await.map(|p| data_url(&p));
        json!({ "id": c["id"], "alias": c["alias"], "active": c["state"] == "ACTIVE", "img": img })
    })).await;
    Ok(json!({ "name": v["name"].as_str().unwrap_or(&acc.name), "skin": skin, "capes": capes, "saved": saved(core) }))
}

/// Current skin, capes and the local skin library for the skins page.
pub async fn profile(core: &Core, id: &str) -> Result<Value, CoreError> {
    let (acc, t) = token(core, id).await?;
    let v = mc_profile(core, &t).await?;
    render(core, &acc, &v).await
}

fn variant(v: &str) -> Result<&'static str, CoreError> {
    match v { "classic" => Ok("classic"), "slim" => Ok("slim"), _ => Err(CoreError::BadArgs("variant".into())) }
}

async fn put_skin(core: &Core, t: &str, var: &str, png: &[u8]) -> Result<(), CoreError> {
    let (ct, body) = multipart(var, png);
    ok(core.http().post(api("/skins")).bearer_auth(t).header("content-type", ct).body(body).send().await?).await
}

async fn put_cape(core: &Core, t: &str, cape: Option<&str>) -> Result<(), CoreError> {
    let h = core.http();
    ok(match cape {
        Some(c) => h.put(api("/capes/active")).bearer_auth(t).json(&json!({ "capeId": c })).send().await?,
        None => h.delete(api("/capes/active")).bearer_auth(t).send().await?,
    }).await
}

/// Keeps the skin the account wears now in the library, so switching never loses it.
async fn preserve(core: &Core, v: &Value) -> Option<()> {
    let skins = v["skins"].as_array()?;
    let cur = skins.iter().find(|s| s["state"] == "ACTIVE")?;
    let url = cur["url"].as_str()?;
    let key = tex_key(url)?;
    let mut m = load_meta(core);
    if crate::skin_packs::find(&key).is_some() || m.keys.values().any(|k| *k == key) { return Some(()); }
    let png = png_at(core, url).await?;
    let var = if cur["variant"].as_str()?.eq_ignore_ascii_case("slim") { "slim" } else { "classic" };
    let name = keep(core, var, &png).ok()?;
    m.keys.insert(name, key);
    save_meta(core, &m).ok()
}

/// Puts a picture on the account; `name` is its library entry (its cape follows it).
async fn wear(core: &Core, id: &str, var: &str, png: &[u8], name: Option<&str>) -> Result<Value, CoreError> {
    let var = variant(var)?;
    check_skin(png)?;
    let (acc, t) = token(core, id).await?;
    if let Ok(v) = mc_profile(core, &t).await { preserve(core, &v).await; }
    put_skin(core, &t, var, png).await?;
    if let Some(c) = name.and_then(|n| load_meta(core).capes.get(n).cloned()) {
        put_cape(core, &t, c.as_deref()).await?;
    }
    forget_head(core, &acc);
    let v = mc_profile(core, &t).await?;
    if let (Some(n), Some(k)) = (name, active_skin(&v).as_deref().and_then(tex_key)) {
        let mut m = load_meta(core);
        m.keys.retain(|_, x| *x != k);
        m.keys.insert(n.into(), k);
        let _ = save_meta(core, &m);
    }
    render(core, &acc, &v).await
}

/// Uploads a new skin, keeps it in the library (with its cape) and puts it on.
pub async fn upload(core: &Core, id: &str, var: &str, png: &[u8], cape: Option<Option<String>>) -> Result<Value, CoreError> {
    let var = variant(var)?;
    check_skin(png)?;
    let name = keep(core, var, png)?;
    if let Some(c) = cape {
        let mut m = load_meta(core);
        m.capes.insert(name.clone(), c);
        save_meta(core, &m)?;
    }
    wear(core, id, var, png, Some(&name)).await
}

fn lib_png(core: &Core, name: &str) -> Result<Vec<u8>, CoreError> {
    if !valid_name(name) { return Err(CoreError::BadArgs("bad skin".into())); }
    fs::read(lib_dir(core).join(name)).map_err(|_| CoreError::msg("Скин не найден"))
}

/// Puts a skin from the library back on, with its cape.
pub async fn use_saved(core: &Core, id: &str, name: &str) -> Result<Value, CoreError> {
    let png = lib_png(core, name)?;
    wear(core, id, name.split('-').next().unwrap_or("classic"), &png, Some(name)).await
}

/// Changes a saved skin: picture, arm model, cape. Re-applies it when it is the one worn now.
pub async fn edit(core: &Core, id: &str, name: &str, var: &str, png: Option<Vec<u8>>, cape: Option<Option<String>>) -> Result<Value, CoreError> {
    let var = variant(var)?;
    let old = lib_png(core, name)?;
    let replaced = png.is_some();
    let png = png.unwrap_or(old);
    check_skin(&png)?;
    let (acc, t) = token(core, id).await?;
    let v = mc_profile(core, &t).await?;
    let now = active_skin(&v).as_deref().and_then(tex_key);
    let mut m = load_meta(core);
    let was_on = now.is_some() && m.keys.get(name) == now.as_ref();
    let new = keep(core, var, &png)?;
    if new != name {
        let _ = fs::remove_file(lib_dir(core).join(name));
        for o in m.order.iter_mut() { if o == name { *o = new.clone(); } }
        if let Some(c) = m.capes.remove(name) { m.capes.insert(new.clone(), c); }
        if let Some(k) = m.keys.remove(name) { if !replaced { m.keys.insert(new.clone(), k); } }
    }
    if let Some(c) = cape { m.capes.insert(new.clone(), c); }
    save_meta(core, &m)?;
    if was_on { return wear(core, id, var, &png, Some(&new)).await; }
    render(core, &acc, &v).await
}

/// Switches the arm model of the current skin (re-uploads the same picture).
pub async fn set_variant(core: &Core, id: &str, var: &str) -> Result<Value, CoreError> {
    let var = variant(var)?;
    let (acc, t) = token(core, id).await?;
    let v = mc_profile(core, &t).await?;
    let url = active_skin(&v).ok_or_else(|| CoreError::msg("Сначала загрузите скин"))?;
    let png = png_at(core, &url).await.ok_or_else(|| CoreError::msg("Не удалось получить текущий скин"))?;
    put_skin(core, &t, var, &png).await?;
    forget_head(core, &acc);
    let v = mc_profile(core, &t).await?;
    render(core, &acc, &v).await
}

/// Back to the default skin (the current one stays in the library).
pub async fn reset(core: &Core, id: &str) -> Result<Value, CoreError> {
    let (acc, t) = token(core, id).await?;
    if let Ok(v) = mc_profile(core, &t).await { preserve(core, &v).await; }
    ok(core.http().delete(api("/skins/active")).bearer_auth(&t).send().await?).await?;
    forget_head(core, &acc);
    let v = mc_profile(core, &t).await?;
    render(core, &acc, &v).await
}

/// Shows one of the account's own capes, or hides it. The worn saved skin remembers the choice.
pub async fn set_cape(core: &Core, id: &str, cape: Option<&str>) -> Result<Value, CoreError> {
    let (acc, t) = token(core, id).await?;
    put_cape(core, &t, cape).await?;
    let v = mc_profile(core, &t).await?;
    if let Some(k) = active_skin(&v).as_deref().and_then(tex_key) {
        let mut m = load_meta(core);
        if let Some(n) = m.keys.iter().find(|(_, x)| **x == k).map(|(n, _)| n.clone()) {
            m.capes.insert(n, cape.map(str::to_owned));
            let _ = save_meta(core, &m);
        }
    }
    render(core, &acc, &v).await
}

pub fn forget_saved(core: &Core, name: &str) -> Result<(), CoreError> {
    if !valid_name(name) { return Err(CoreError::BadArgs("bad skin".into())); }
    let _ = fs::remove_file(lib_dir(core).join(name));
    let mut m = load_meta(core);
    m.order.retain(|o| o != name);
    m.capes.remove(name);
    m.keys.remove(name);
    save_meta(core, &m)
}

/// Manual order of the saved skins.
pub fn reorder(core: &Core, names: Vec<String>) -> Result<(), CoreError> {
    if names.len() > 512 || !names.iter().all(|n| valid_name(n)) { return Err(CoreError::BadArgs("names".into())); }
    let mut m = load_meta(core);
    m.order = names;
    save_meta(core, &m)
}

/// Skins of the vanilla launcher, cached under `cache/skins/default/<key>.png`.
pub async fn defaults(core: &Core) -> Vec<Value> {
    let dir = core.data_dir.join("cache").join("skins").join("default");
    let all = crate::skin_packs::DEFAULTS;
    let mut out = Vec::with_capacity(all.len());
    for chunk in all.chunks(12) {
        let got = futures_util::future::join_all(chunk.iter().map(|d| {
            let path = dir.join(format!("{}.png", d.0));
            async move {
                if let Ok(b) = fs::read(&path) { return Some(b); }
                let b = png_at(core, &format!("{TEX_HOST}texture/{}", d.0)).await?;
                let _ = crate::game::download::write_atomic(&path, &b);
                Some(b)
            }
        })).await;
        for (d, b) in chunk.iter().zip(got) {
            out.push(json!({ "key": d.0, "name": d.1, "section": d.2, "variant": d.3, "img": b.map(|b| data_url(&b)) }));
        }
    }
    out
}

/// Puts on a vanilla skin by its texture URL; falls back to uploading the picture.
pub async fn use_default(core: &Core, id: &str, key: &str) -> Result<Value, CoreError> {
    let d = crate::skin_packs::find(key).ok_or_else(|| CoreError::BadArgs("skin".into()))?;
    let (acc, t) = token(core, id).await?;
    if let Ok(v) = mc_profile(core, &t).await { preserve(core, &v).await; }
    let url = format!("{TEX_HOST}texture/{}", d.0);
    let r = core.http().post(api("/skins")).bearer_auth(&t).json(&json!({ "variant": d.3, "url": url })).send().await?;
    if r.status().as_u16() == 429 || ok(r).await.is_err() {
        let png = png_at(core, &url).await.ok_or_else(|| CoreError::msg("Не удалось получить скин"))?;
        put_skin(core, &t, d.3, &png).await?;
    }
    forget_head(core, &acc);
    let v = mc_profile(core, &t).await?;
    render(core, &acc, &v).await
}

/// A PNG dropped from the file manager, checked as a skin.
pub fn read_png(path: &str) -> Result<Value, CoreError> {
    if !path.to_ascii_lowercase().ends_with(".png") { return Err(CoreError::msg("Нужна картинка PNG")); }
    let len = fs::metadata(path)?.len();
    if len > 2 << 20 { return Err(CoreError::msg("Файл слишком большой для скина")); }
    let b = fs::read(path)?;
    check_skin(&b)?;
    Ok(json!({ "data": data_url(&b) }))
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
    fn skin_checks() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&64u32.to_be_bytes());
        png.extend_from_slice(&32u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((64, 32)));
        assert!(check_skin(&png).is_ok());
        png[23] = 65;
        assert!(check_skin(&png).is_err());
        assert!(check_skin(b"GIF89a").is_err());
        let (ct, body) = multipart("slim", b"PNGDATA");
        let b = ct.split("boundary=").nth(1).unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with(&format!("--{b}\r\n")) && text.ends_with(&format!("--{b}--\r\n")));
        assert!(text.contains("name=\"variant\"\r\n\r\nslim\r\n") && text.contains("PNGDATA"));
        assert!(valid_name("slim-0123456789abcdef.png"));
        assert!(!valid_name("slim-0123456789ABCDEF.png"));
        assert!(!valid_name("../classic-0123456789abcdef.png"));
        assert!(!valid_name("wide-0123456789abcdef.png"));
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
