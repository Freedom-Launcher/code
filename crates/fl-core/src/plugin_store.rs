//! Official plugin catalog. The list lives in `plugins/catalog.json` of the
//! Freedom-Launcher/releases repository; every entry names a signed `.zip` and its SHA-256.
//! Live numbers come from the same repository: downloads from the assets of the `plugins`
//! release, likes from the reactions (👍 and ❤) on one issue per plugin. Both are optional:
//! when GitHub is unreachable the last known numbers are kept, and the catalog itself falls
//! back to the cached copy.

use std::collections::HashSet;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{addons, Core, CoreError};

pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Freedom-Launcher/releases/main/plugins/catalog.json";
const API: &str = "https://api.github.com/repos/Freedom-Launcher/releases";
const ISSUES: &str = "https://github.com/Freedom-Launcher/releases/issues/";
const CACHE: &str = "addons-catalog.json";
const TTL: u64 = 600;
const MAX_ZIP: usize = 64 << 20;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub hue: Option<u16>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    /// File name in the `plugins` release: its download counter.
    #[serde(default)]
    pub asset: String,
    /// Issue number whose reactions are the likes.
    #[serde(default)]
    pub issue: u64,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub likes: u64,
    #[serde(default)]
    pub like_url: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    at: u64,
    plugins: Vec<Entry>,
}

#[derive(Deserialize)]
struct Feed {
    #[serde(default)]
    plugins: Vec<Entry>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Only our own GitHub files: the raw catalog repository or its release downloads.
pub fn url_ok(u: &str) -> bool {
    (u.starts_with("https://raw.githubusercontent.com/Freedom-Launcher/")
        || u.starts_with("https://github.com/Freedom-Launcher/"))
        && u.len() <= 512
        && !u.contains("..")
        && u.bytes().all(|b| b > 0x20 && b < 0x7f && !matches!(b, b'"' | b'\\' | b'<' | b'>'))
}

fn valid(e: &Entry) -> bool {
    (1..=64).contains(&e.id.len())
        && e
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !addons::RESERVED.contains(&e.id.as_str())
        && !e.name.trim().is_empty()
        && e.sha256.len() == 64
        && e.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        && url_ok(&e.url)
}

fn likes(r: &Value) -> u64 {
    r["+1"].as_u64().unwrap_or(0) + r["heart"].as_u64().unwrap_or(0)
}

/// Catalog entries with live numbers; `old` supplies numbers GitHub did not return.
fn merge(list: Vec<Entry>, issues: Option<&Value>, rel: Option<&Value>, old: &[Entry]) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let issues = issues.and_then(Value::as_array);
    let assets = rel.and_then(|r| r["assets"].as_array());
    list.into_iter()
        .filter(valid)
        .filter(|e| seen.insert(e.id.clone()))
        .map(|mut e| {
            let prev = old.iter().find(|o| o.id == e.id);
            e.likes = match issues {
                Some(a) if e.issue > 0 => a
                    .iter()
                    .find(|i| i["number"].as_u64() == Some(e.issue))
                    .map_or(0, |i| likes(&i["reactions"])),
                Some(_) => 0,
                None => prev.map_or(0, |p| p.likes),
            };
            e.like_url = if e.issue > 0 { format!("{ISSUES}{}", e.issue) } else { String::new() };
            e.downloads = match assets {
                Some(a) => match a.iter().find(|x| !e.asset.is_empty() && x["name"].as_str() == Some(e.asset.as_str())) {
                    Some(x) => {
                        if let Some(u) = x["browser_download_url"].as_str().filter(|u| url_ok(u)) {
                            e.url = u.to_owned();
                        }
                        x["download_count"].as_u64().unwrap_or(0)
                    }
                    None => 0,
                },
                None => prev.map_or(0, |p| p.downloads),
            };
            e
        })
        .collect()
}

async fn get_json(c: &Core, url: &str) -> Result<Value, CoreError> {
    let r = c
        .http()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?
        .error_for_status()?;
    Ok(r.json().await?)
}

/// The catalog, cached for ten minutes. `offline` = the cached copy is shown because
/// the catalog could not be fetched.
pub async fn catalog(c: &Core, refresh: bool) -> Result<(Vec<Entry>, bool), CoreError> {
    let path = c.path(CACHE);
    let cached: Cache = crate::store::load(&path).unwrap_or_default();
    let t = now();
    if !refresh && !cached.plugins.is_empty() && t.saturating_sub(cached.at) < TTL {
        return Ok((cached.plugins, false));
    }
    let issues_url = format!("{API}/issues?labels=plugin&state=all&per_page=100");
    let rel_url = format!("{API}/releases/tags/plugins");
    let (feed, issues, rel) = futures_util::future::join3(
        get_json(c, CATALOG_URL),
        get_json(c, &issues_url),
        get_json(c, &rel_url),
    )
    .await;
    let feed: Feed = match feed.and_then(|v| Ok(serde_json::from_value(v)?)) {
        Ok(f) => f,
        Err(_) if !cached.plugins.is_empty() => return Ok((cached.plugins, true)),
        Err(e) => return Err(CoreError::msg(format!("Каталог недоступен: {e}"))),
    };
    let plugins = merge(feed.plugins, issues.ok().as_ref(), rel.ok().as_ref(), &cached.plugins);
    if let Ok(b) = serde_json::to_vec(&Cache { at: t, plugins: plugins.clone() }) {
        let _ = fs::write(&path, b);
    }
    Ok((plugins, false))
}

/// Downloads a catalog plugin, checks its SHA-256 and installs it like a picked `.zip`.
pub async fn install(c: &Core, id: &str) -> Result<String, CoreError> {
    let (list, _) = catalog(c, false).await?;
    let e = list
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| CoreError::msg("Этого плагина нет в каталоге"))?;
    if !url_ok(&e.url) {
        return Err(CoreError::msg("Адрес плагина не прошёл проверку"));
    }
    let mut resp = c.http().get(&e.url).send().await?.error_for_status()?;
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if buf.len() + chunk.len() > MAX_ZIP {
            return Err(CoreError::msg("Плагин слишком большой"));
        }
        buf.extend_from_slice(&chunk);
    }
    let got = crate::signing::hex(&Sha256::digest(&buf));
    if !got.eq_ignore_ascii_case(&e.sha256) {
        // The catalog may have moved on since it was cached.
        let _ = fs::remove_file(c.path(CACHE));
        return Err(CoreError::msg("Файл плагина не совпал с каталогом. Обновите список и попробуйте ещё раз"));
    }
    fs::create_dir_all(&c.data_dir)?;
    let tmp = c.data_dir.join(format!("addon-dl-{}.zip", uuid::Uuid::new_v4()));
    fs::write(&tmp, &buf)?;
    let r = addons::install_zip(&c.data_dir, &tmp);
    let _ = fs::remove_file(&tmp);
    let got_id = r.map_err(CoreError::BadArgs)?;
    if got_id != e.id {
        return Err(CoreError::msg("В архиве другой плагин"));
    }
    Ok(got_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn e(id: &str) -> Entry {
        Entry {
            id: id.into(),
            name: "N".into(),
            version: "1.0.0".into(),
            url: format!("https://raw.githubusercontent.com/Freedom-Launcher/releases/main/plugins/{id}.zip"),
            sha256: "a".repeat(64),
            asset: format!("{id}.zip"),
            issue: 3,
            ..Default::default()
        }
    }

    #[test]
    fn urls() {
        assert!(url_ok("https://github.com/Freedom-Launcher/releases/releases/download/plugins/a.zip"));
        assert!(!url_ok("http://github.com/Freedom-Launcher/x.zip"));
        assert!(!url_ok("https://github.com/someone/x.zip"));
        assert!(!url_ok("https://raw.githubusercontent.com/Freedom-Launcher/../x/y.zip"));
        assert!(!url_ok("https://github.com.evil.io/Freedom-Launcher/x.zip"));
    }

    #[test]
    fn merges_numbers() {
        let mut bad = e("Bad Id");
        bad.name = "x".into();
        let list = vec![e("mclogs"), e("mclogs"), bad, e("addons")];
        let issues = json!([{ "number": 3, "reactions": { "+1": 4, "heart": 2, "laugh": 9 } }]);
        let rel = json!({ "assets": [{ "name": "mclogs.zip", "download_count": 17,
            "browser_download_url": "https://github.com/Freedom-Launcher/releases/releases/download/plugins/mclogs.zip" }] });
        let out = merge(list, Some(&issues), Some(&rel), &[]);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].likes, out[0].downloads), (6, 17));
        assert!(out[0].url.contains("/releases/download/"));
        assert_eq!(out[0].like_url, "https://github.com/Freedom-Launcher/releases/issues/3");
        // GitHub unreachable: last known numbers stay.
        let again = merge(vec![e("mclogs")], None, None, &out);
        assert_eq!((again[0].likes, again[0].downloads), (6, 17));
    }
}
#[cfg(test)]
mod official {
    /// Every plugin in `plugins/` of the code repository loads as an addon.
    #[test]
    fn official_plugins_load() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let d = std::env::temp_dir().join(format!("fl-off-{}", uuid::Uuid::new_v4()));
        let mut n = 0;
        for e in std::fs::read_dir(&src).unwrap().flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            let to = crate::addons::dir(&d).join(&id);
            std::fs::create_dir_all(&to).unwrap();
            for f in std::fs::read_dir(e.path()).unwrap().flatten() {
                std::fs::copy(f.path(), to.join(f.file_name())).unwrap();
            }
            crate::addons::load(&d, &id).unwrap_or_else(|err| panic!("{id}: {err}"));
            n += 1;
        }
        assert!(n >= 2);
        let _ = std::fs::remove_dir_all(&d);
    }
}
