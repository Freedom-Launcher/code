//! Plugin catalogs. The official one lives in `plugins/catalog.json` of the
//! Freedom-Launcher/releases repository; every entry names a signed `.zip` and its SHA-256.
//! Anyone can publish another: a `freedom-plugins.json` in any repository (or any https URL)
//! with the same entries, and a player adds that source once, after which its plugins show up
//! in the same catalog (see «Plugin sources» in docs/ADDONS.md). Nothing from a source is trusted
//! beyond what the checks below prove: the archive matches its SHA-256, holds the plugin the
//! entry names, asks for no permission the entry did not list, cannot replace a plugin that came
//! from elsewhere, and still goes through the normal permission prompt.
//! Live numbers come from the same repository: downloads from the assets of the `plugins`
//! release, likes from the reactions (👍 and ❤) on one issue per plugin. Both are optional:
//! when GitHub is unreachable the last known numbers are kept, and the catalog itself falls
//! back to the cached copy.

use std::collections::{BTreeMap, HashSet};
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
    /// Which source listed it (filled in here, never read from a feed).
    #[serde(default)]
    pub source: String,
    /// The source's catalog URL; empty for the official catalog.
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub official: bool,
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
        && u.bytes()
            .all(|b| b > 0x20 && b < 0x7f && !matches!(b, b'"' | b'\\' | b'<' | b'>'))
}

fn valid(e: &Entry) -> bool {
    (1..=64).contains(&e.id.len())
        && e.id
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
fn merge(
    list: Vec<Entry>,
    issues: Option<&Value>,
    rel: Option<&Value>,
    old: &[Entry],
) -> Vec<Entry> {
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
            e.like_url = if e.issue > 0 {
                format!("{ISSUES}{}", e.issue)
            } else {
                String::new()
            };
            e.downloads = match assets {
                Some(a) => match a
                    .iter()
                    .find(|x| !e.asset.is_empty() && x["name"].as_str() == Some(e.asset.as_str()))
                {
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

/// The official catalog, cached for ten minutes. `offline` = the cached copy is shown because
/// the catalog could not be fetched.
async fn official_catalog(c: &Core, refresh: bool) -> Result<(Vec<Entry>, bool), CoreError> {
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
    let mut plugins = merge(
        feed.plugins,
        issues.ok().as_ref(),
        rel.ok().as_ref(),
        &cached.plugins,
    );
    for e in &mut plugins {
        e.official = true;
        e.source = "Freedom Launcher".into();
    }
    if let Ok(b) = serde_json::to_vec(&Cache {
        at: t,
        plugins: plugins.clone(),
    }) {
        let _ = fs::write(&path, b);
    }
    Ok((plugins, false))
}

// ---------------------------------------------------------------- sources

/// Where files from sources come from. The real one is `net::fetch_limited`; tests use a fake.
pub(crate) trait Fetch {
    fn get(
        &self,
        url: &str,
        max: usize,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send;
}

pub(crate) struct Net;

impl Fetch for Net {
    async fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
        crate::net::fetch_limited(url, max).await
    }
}

pub const SOURCE_FILE: &str = "freedom-plugins.json";
const MAX_SOURCES: usize = 16;
const MAX_FEED: usize = 1 << 20;
const MAX_PER_SOURCE: usize = 200;
const SRC_CACHE: &str = "addons-sources.json";
const SETTINGS_FILE: &str = "settings.json";

/// Read-change-write of one of our JSON state files under the core lock.
async fn update_json<T: Serialize + serde::de::DeserializeOwned + Default>(
    c: &Core,
    file: &str,
    f: impl FnOnce(&mut T),
) -> Result<(), CoreError> {
    let _g = c.state_lock.lock().await;
    let mut v: T = crate::store::load(&c.path(file))?;
    f(&mut v);
    crate::store::save(&c.path(file), &v, false)
}

#[derive(Default, Serialize, Deserialize)]
struct SrcCache {
    #[serde(default)]
    feeds: BTreeMap<String, CachedFeed>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct CachedFeed {
    at: u64,
    name: String,
    plugins: Vec<Entry>,
}

/// What a source's file looks like: an optional name and the plugin entries (`Entry`, without
/// the numbers GitHub gives the official catalog).
#[derive(Deserialize)]
struct SourceFeed {
    #[serde(default)]
    name: String,
    #[serde(default)]
    plugins: Vec<Entry>,
}

/// How one source did in the last look: shown next to the catalog.
#[derive(Clone, Debug, Serialize)]
pub struct SourceStatus {
    pub url: String,
    pub name: String,
    pub official: bool,
    pub count: usize,
    pub error: Option<String>,
}

pub struct Catalog {
    pub plugins: Vec<Entry>,
    pub offline: bool,
    pub sources: Vec<SourceStatus>,
}

fn printable(s: &str, max: usize) -> bool {
    s.len() <= max && s.chars().all(|c| !c.is_control())
}

/// An https URL of a host name on the default port without credentials: what we are willing to
/// fetch from a source the user added (the connection itself also refuses private addresses).
pub fn https_url_ok(u: &str) -> bool {
    let Ok(p) = reqwest::Url::parse(u) else {
        return false;
    };
    u.len() <= 512
        && printable(u, 512)
        && p.scheme() == "https"
        && p.domain().is_some()
        && p.username().is_empty()
        && p.password().is_none()
        && matches!(p.port(), None | Some(443))
}

/// What a player types to add a source, as the URL of its catalog file: `owner/repo`, a GitHub
/// repository address (a `/tree/<branch>` is honoured), or the https URL of the JSON itself.
pub fn resolve_source(input: &str) -> Result<String, CoreError> {
    let t = input.trim();
    let bad = || {
        CoreError::BadArgs("Не получилось понять адрес. Нужен репозиторий GitHub (владелец/репозиторий) или ссылка https на файл со списком плагинов".into())
    };
    let name_ok = |s: &str, max: usize| {
        (1..=max).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            && !s.starts_with('.')
            && !s.contains("..")
    };
    let raw = |o: &str, r: &str, rf: &str| {
        format!("https://raw.githubusercontent.com/{o}/{r}/{rf}/{SOURCE_FILE}")
    };
    if let Some((o, r)) = t.split_once('/') {
        if !t.contains(':') && !r.contains('/') && name_ok(o, 39) && name_ok(r, 100) {
            return Ok(raw(o, r.trim_end_matches(".git"), "HEAD"));
        }
    }
    if !https_url_ok(t) {
        return Err(bad());
    }
    let u = reqwest::Url::parse(t).map_err(|_| bad())?;
    if u.host_str() == Some("github.com") {
        let seg: Vec<&str> = u
            .path_segments()
            .map(|s| s.filter(|x| !x.is_empty()).collect())
            .unwrap_or_default();
        if seg.len() >= 2 && name_ok(seg[0], 39) && name_ok(seg[1].trim_end_matches(".git"), 100) {
            let rf = if seg.len() >= 4 && seg[2] == "tree" && name_ok(seg[3], 100) {
                seg[3]
            } else {
                "HEAD"
            };
            return Ok(raw(seg[0], seg[1].trim_end_matches(".git"), rf));
        }
        return Err(bad());
    }
    Ok(t.to_owned())
}

/// Entries of a third-party feed, with everything a feed must not decide for itself overwritten:
/// it cannot call itself official, claim downloads or likes, or name another source.
fn third_party_entries(feed: Vec<Entry>, url: &str, source: &str) -> Vec<Entry> {
    let mut seen = HashSet::new();
    feed.into_iter()
        .filter(|e| {
            (1..=64).contains(&e.id.len())
                && e.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && !addons::RESERVED.contains(&e.id.as_str())
                && (1..=60).contains(&e.name.trim().len())
                && printable(&e.name, 60)
                && printable(&e.author, 60)
                && printable(&e.description, 400)
                && (1..=32).contains(&e.version.len())
                && printable(&e.version, 32)
                && e.permissions.len() <= 16
                && e.permissions.iter().all(|p| printable(p, 32))
                && e.sha256.len() == 64
                && e.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                && https_url_ok(&e.url)
        })
        .filter(|e| seen.insert(e.id.clone()))
        .take(MAX_PER_SOURCE)
        .map(|mut e| {
            e.sha256 = e.sha256.to_ascii_lowercase();
            e.official = false;
            e.source = source.to_owned();
            e.source_url = url.to_owned();
            e.downloads = 0;
            e.likes = 0;
            e.like_url = String::new();
            e.asset = String::new();
            e.issue = 0;
            e.hue = e.hue.map(|h| h % 360);
            e
        })
        .collect()
}

fn feed_name(declared: &str, url: &str) -> String {
    let d = declared.trim();
    if (1..=40).contains(&d.len()) && printable(d, 40) {
        return d.to_owned();
    }
    reqwest::Url::parse(url)
        .ok()
        .map(|u| {
            let host = u.host_str().unwrap_or_default().to_owned();
            match (
                host.as_str(),
                u.path_segments()
                    .and_then(|mut s| Some((s.next()?.to_owned(), s.next()?.to_owned()))),
            ) {
                ("raw.githubusercontent.com", Some((o, r))) => format!("{o}/{r}"),
                _ => host,
            }
        })
        .unwrap_or_default()
}

/// One source's feed: fresh (within the cache time), fetched, or the cached copy when it cannot be.
async fn source_feed<F: Fetch>(
    c: &Core,
    f: &F,
    s: &crate::settings::PluginSource,
    refresh: bool,
) -> (CachedFeed, Option<String>) {
    let path = c.path(SRC_CACHE);
    let mut cache: SrcCache = crate::store::load(&path).unwrap_or_default();
    let old = cache.feeds.get(&s.url).cloned();
    let t = now();
    if let Some(o) = &old {
        if !refresh && t.saturating_sub(o.at) < TTL {
            return (o.clone(), None);
        }
    }
    let got = async {
        let bytes = f.get(&s.url, MAX_FEED).await.map_err(CoreError::msg)?;
        let feed: SourceFeed = serde_json::from_slice(&bytes).map_err(|_| {
            CoreError::msg("это не список плагинов (ожидается JSON с полем plugins)")
        })?;
        let name = if s.name.is_empty() {
            feed_name(&feed.name, &s.url)
        } else {
            s.name.clone()
        };
        let plugins = third_party_entries(feed.plugins, &s.url, &name);
        Ok::<_, CoreError>(CachedFeed {
            at: t,
            name,
            plugins,
        })
    }
    .await;
    match got {
        Ok(feed) => {
            cache.feeds.insert(s.url.clone(), feed.clone());
            if let Ok(b) = serde_json::to_vec(&cache) {
                let _ = fs::write(&path, b);
            }
            (feed, None)
        }
        Err(e) => {
            let mut feed = old.unwrap_or_default();
            if feed.name.is_empty() {
                feed.name = if s.name.is_empty() {
                    feed_name("", &s.url)
                } else {
                    s.name.clone()
                };
            }
            (feed, Some(e.to_string()))
        }
    }
}

/// Every catalog the player has: the official one first, then the sources they added. A source
/// that cannot be read is reported next to the rest and does not hide the others.
pub async fn catalog(c: &Core, refresh: bool) -> Result<Catalog, CoreError> {
    catalog_with(c, &Net, refresh, true).await
}

async fn catalog_with<F: Fetch>(
    c: &Core,
    f: &F,
    refresh: bool,
    official: bool,
) -> Result<Catalog, CoreError> {
    let settings: crate::settings::Settings =
        crate::store::load(&c.path(SETTINGS_FILE)).unwrap_or_default();
    let off = if official {
        official_catalog(c, refresh).await
    } else {
        Ok((Vec::new(), false))
    };
    let mut out = Catalog {
        plugins: Vec::new(),
        offline: false,
        sources: Vec::new(),
    };
    let mut official_err = None;
    match off {
        Ok((p, offline)) => {
            out.offline = offline;
            out.sources.push(SourceStatus {
                url: String::new(),
                name: "Freedom Launcher".into(),
                official: true,
                count: p.len(),
                error: None,
            });
            out.plugins.extend(p);
        }
        Err(e) => {
            official_err = Some(e);
            out.sources.push(SourceStatus {
                url: String::new(),
                name: "Freedom Launcher".into(),
                official: true,
                count: 0,
                error: official_err.as_ref().map(|e| e.to_string()),
            });
        }
    }
    for s in &settings.plugin_sources {
        let (feed, err) = source_feed(c, f, s, refresh).await;
        out.sources.push(SourceStatus {
            url: s.url.clone(),
            name: feed.name.clone(),
            official: false,
            count: feed.plugins.len(),
            error: err,
        });
        out.plugins.extend(feed.plugins);
    }
    match official_err {
        Some(e) if out.plugins.is_empty() => Err(e),
        _ => Ok(out),
    }
}

/// Adds a source: resolves what the player typed, reads it once (so a wrong address fails now,
/// not later), and keeps it. Returns the saved list.
pub async fn add_source(
    c: &Core,
    input: &str,
) -> Result<Vec<crate::settings::PluginSource>, CoreError> {
    add_source_with(c, &Net, input).await
}

async fn add_source_with<F: Fetch>(
    c: &Core,
    f: &F,
    input: &str,
) -> Result<Vec<crate::settings::PluginSource>, CoreError> {
    let url = resolve_source(input)?;
    let mut st: crate::settings::Settings =
        crate::store::load(&c.path(SETTINGS_FILE)).unwrap_or_default();
    if st.plugin_sources.iter().any(|s| s.url == url) {
        return Ok(st.plugin_sources);
    }
    if st.plugin_sources.len() >= MAX_SOURCES {
        return Err(CoreError::BadArgs(format!(
            "Источников не больше {MAX_SOURCES}"
        )));
    }
    let bytes = f
        .get(&url, MAX_FEED)
        .await
        .map_err(|e| CoreError::msg(format!("Не удалось прочитать список: {e}")))?;
    let feed: SourceFeed = serde_json::from_slice(&bytes)
        .map_err(|_| CoreError::msg("Это не список плагинов: ожидается JSON с полем plugins"))?;
    let name = feed_name(&feed.name, &url);
    let src = crate::settings::PluginSource { url, name };
    let saved = src.clone();
    update_json(c, SETTINGS_FILE, |s: &mut crate::settings::Settings| {
        s.plugin_sources.push(src)
    })
    .await?;
    st.plugin_sources.push(saved);
    Ok(st.plugin_sources)
}

pub async fn remove_source(
    c: &Core,
    url: &str,
) -> Result<Vec<crate::settings::PluginSource>, CoreError> {
    let mut left = Vec::new();
    update_json(c, SETTINGS_FILE, |s: &mut crate::settings::Settings| {
        s.plugin_sources.retain(|x| x.url != url);
        left = s.plugin_sources.clone();
    })
    .await?;
    let path = c.path(SRC_CACHE);
    let mut cache: SrcCache = crate::store::load(&path).unwrap_or_default();
    cache.feeds.remove(url);
    if let Ok(b) = serde_json::to_vec(&cache) {
        let _ = fs::write(&path, b);
    }
    Ok(left)
}

// ---------------------------------------------------------------- updates

/// Is the catalog's `cat` version newer than the installed `have`? Dotted numbers compare
/// numerically (missing parts are 0), a pre-release (`1.2.0-beta.1`) is older than its release,
/// build metadata (`+abc`) is ignored. Anything unreadable is "not newer": a version we cannot
/// parse must never trigger an update.
pub fn is_newer(cat: &str, have: &str) -> bool {
    type Parsed = (Vec<u64>, Option<Vec<String>>);
    fn parse(v: &str) -> Option<Parsed> {
        let v = v.trim().trim_start_matches(['v', 'V']);
        let v = v.split('+').next()?;
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p.split('.').map(str::to_owned).collect())),
            None => (v, None),
        };
        let nums: Option<Vec<u64>> = core.split('.').map(|x| x.parse().ok()).collect();
        let nums = nums.filter(|n| !n.is_empty())?;
        Some((nums, pre))
    }
    let (Some((a, ap)), Some((b, bp))) = (parse(cat), parse(have)) else {
        return false;
    };
    let n = a.len().max(b.len());
    for i in 0..n {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    match (ap, bp) {
        (None, None) | (Some(_), None) => false,
        (None, Some(_)) => true,
        (Some(x), Some(y)) => {
            for (p, q) in x.iter().zip(&y) {
                let ord = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(p), Ok(q)) => p.cmp(&q),
                    (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                    (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                    _ => p.cmp(q),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord == std::cmp::Ordering::Greater;
                }
            }
            x.len() > y.len()
        }
    }
}

/// An installed plugin that has a newer version in the source it came from.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Update {
    pub id: String,
    pub name: String,
    pub have: String,
    pub version: String,
    /// `official` or the source's catalog URL (what `install_from` takes as `source`, "" for official).
    pub source: String,
    pub source_url: String,
    pub official: bool,
}

/// Which source an installed plugin should be updated from. The recorded source wins; an older
/// install of a signed plugin (no source recorded yet) counts as official; hand-made installs and
/// developer links (`dev`) are never updated behind the player's back.
fn effective_source(recorded: &str, signed: bool) -> Option<String> {
    match recorded {
        "dev" => None,
        "" if signed => Some(OFFICIAL.to_owned()),
        "" => None,
        other => Some(other.to_owned()),
    }
}

/// Plugins with a newer version in their own source (cached catalogs unless `refresh`).
pub async fn updates(c: &Core, refresh: bool) -> Result<Vec<Update>, CoreError> {
    updates_with(c, &Net, refresh, true).await
}

async fn updates_with<F: Fetch>(
    c: &Core,
    f: &F,
    refresh: bool,
    official: bool,
) -> Result<Vec<Update>, CoreError> {
    let cat = catalog_with(c, f, refresh, official).await?;
    let st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap_or_default();
    let mut out = Vec::new();
    for a in addons::list(&c.data_dir, &st) {
        let (Some(id), Some(have)) = (a["id"].as_str(), a["version"].as_str()) else {
            continue;
        };
        if a.get("error").is_some_and(|e| !e.is_null()) {
            continue;
        }
        let recorded = st
            .addons
            .get(id)
            .map(|x| x.source.as_str())
            .unwrap_or_default();
        let signed = addons::trust(&c.data_dir, id) != crate::Trust::Community;
        let Some(src) = effective_source(recorded, signed) else {
            continue;
        };
        let found = cat.plugins.iter().find(|e| {
            e.id == id
                && if src == OFFICIAL {
                    e.official
                } else {
                    !e.official && e.source_url == src
                }
        });
        if let Some(e) = found.filter(|e| is_newer(&e.version, have)) {
            out.push(Update {
                id: id.to_owned(),
                name: e.name.clone(),
                have: have.to_owned(),
                version: e.version.clone(),
                source: src,
                source_url: e.source_url.clone(),
                official: e.official,
            });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- install

/// Every permission the archive asks for must be one the entry listed: what the player read in the
/// catalog is a ceiling, not a suggestion.
fn perms_within(manifest: &[String], listed: &[String]) -> Result<(), String> {
    let extra: Vec<&str> = manifest
        .iter()
        .filter(|p| !listed.contains(p))
        .map(String::as_str)
        .collect();
    if extra.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Плагин просит права, которых нет в описании каталога: {}",
            extra.join(", ")
        ))
    }
}

/// May a plugin from `source` (`official` or a catalog URL) replace what is installed?
/// `have_source` is where the installed copy came from (empty: put in by hand) and
/// `signed` whether it carries a «Проверено» / «Официальный» signature.
fn may_replace(
    have: bool,
    have_source: &str,
    signed: bool,
    source: &str,
) -> Result<(), &'static str> {
    if !have {
        return Ok(());
    }
    if have_source == source {
        return Ok(());
    }
    if signed && source != OFFICIAL {
        return Err(
            "Этот плагин уже установлен с подписью проекта; другой источник не может его заменить",
        );
    }
    if !have_source.is_empty() {
        return Err("Этот плагин уже установлен из другого источника; сначала удалите его");
    }
    Ok(())
}

pub const OFFICIAL: &str = "official";

/// Downloads a catalog plugin, checks it and installs it like a picked `.zip`. `source_url` picks
/// the source when several list the same id (empty: the official catalog).
pub async fn install(c: &Core, id: &str, source_url: &str) -> Result<String, CoreError> {
    install_with(c, &Net, id, source_url, true).await
}

async fn install_with<F: Fetch>(
    c: &Core,
    f: &F,
    id: &str,
    source_url: &str,
    official: bool,
) -> Result<String, CoreError> {
    let cat = catalog_with(c, f, false, official).await?;
    let e = cat
        .plugins
        .into_iter()
        .find(|e| e.id == id && e.source_url == source_url)
        .ok_or_else(|| CoreError::msg("Этого плагина нет в каталоге"))?;
    let buf = if e.official {
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
        buf
    } else {
        f.get(&e.url, MAX_ZIP)
            .await
            .map_err(|m| CoreError::msg(format!("Не удалось скачать плагин: {m}")))?
    };
    let got = crate::signing::hex(&Sha256::digest(&buf));
    if !got.eq_ignore_ascii_case(&e.sha256) {
        // The catalog may have moved on since it was cached.
        let _ = fs::remove_file(c.path(CACHE));
        let _ = fs::remove_file(c.path(SRC_CACHE));
        return Err(CoreError::msg(
            "Файл плагина не совпал с каталогом. Обновите список и попробуйте ещё раз",
        ));
    }
    fs::create_dir_all(&c.data_dir)?;
    let tmp = c
        .data_dir
        .join(format!("addon-dl-{}.zip", uuid::Uuid::new_v4()));
    fs::write(&tmp, &buf)?;
    let done = (|| -> Result<(String, String), CoreError> {
        let source = if e.official {
            OFFICIAL.to_owned()
        } else {
            e.source_url.clone()
        };
        let (mid, perms) = addons::zip_manifest(&tmp).map_err(CoreError::BadArgs)?;
        if mid != e.id {
            return Err(CoreError::msg("В архиве другой плагин"));
        }
        if !e.official {
            perms_within(&perms, &e.permissions).map_err(CoreError::BadArgs)?;
        }
        let st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap_or_default();
        let have = addons::installed_ids(&c.data_dir).contains(&e.id);
        let have_source = st
            .addons
            .get(&e.id)
            .map(|x| x.source.clone())
            .unwrap_or_default();
        let signed = addons::trust(&c.data_dir, &e.id) != crate::Trust::Community;
        may_replace(have, &have_source, signed, &source).map_err(CoreError::msg)?;
        let got_id = addons::install_zip(&c.data_dir, &tmp).map_err(CoreError::BadArgs)?;
        Ok((got_id, source))
    })();
    let _ = fs::remove_file(&tmp);
    let (got_id, source) = done?;
    // Remember where it came from: another source may not replace it later.
    let gid = got_id.clone();
    update_json(c, addons::STATE, move |s: &mut addons::State| {
        s.addons.entry(gid).or_default().source = source;
    })
    .await?;
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
            url: format!(
                "https://raw.githubusercontent.com/Freedom-Launcher/releases/main/plugins/{id}.zip"
            ),
            sha256: "a".repeat(64),
            asset: format!("{id}.zip"),
            issue: 3,
            ..Default::default()
        }
    }

    #[test]
    fn urls() {
        assert!(url_ok(
            "https://github.com/Freedom-Launcher/releases/releases/download/plugins/a.zip"
        ));
        assert!(!url_ok("http://github.com/Freedom-Launcher/x.zip"));
        assert!(!url_ok("https://github.com/someone/x.zip"));
        assert!(!url_ok(
            "https://raw.githubusercontent.com/Freedom-Launcher/../x/y.zip"
        ));
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
        assert_eq!(
            out[0].like_url,
            "https://github.com/Freedom-Launcher/releases/issues/3"
        );
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

#[cfg(test)]
mod sources {
    use super::*;

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.into(),
            name: "Plugin".into(),
            version: "1.0.0".into(),
            url: "https://example.org/p.zip".into(),
            sha256: "A".repeat(64),
            permissions: vec!["network".into()],
            ..Default::default()
        }
    }

    #[test]
    fn what_a_player_types_becomes_a_catalog_url() {
        let raw = |o: &str, r: &str, rf: &str| {
            format!("https://raw.githubusercontent.com/{o}/{r}/{rf}/freedom-plugins.json")
        };
        assert_eq!(
            resolve_source("someone/plugins").unwrap(),
            raw("someone", "plugins", "HEAD")
        );
        assert_eq!(
            resolve_source("  some-one/my.plugins  ").unwrap(),
            raw("some-one", "my.plugins", "HEAD")
        );
        assert_eq!(
            resolve_source("https://github.com/someone/plugins").unwrap(),
            raw("someone", "plugins", "HEAD")
        );
        assert_eq!(
            resolve_source("https://github.com/someone/plugins.git").unwrap(),
            raw("someone", "plugins", "HEAD")
        );
        assert_eq!(
            resolve_source("https://github.com/someone/plugins/tree/dev").unwrap(),
            raw("someone", "plugins", "dev")
        );
        // A direct https URL stays as it is.
        assert_eq!(
            resolve_source("https://example.org/list.json").unwrap(),
            "https://example.org/list.json"
        );
        for bad in [
            "",
            "plugins",
            "a/b/c",
            "http://example.org/list.json",
            "ftp://x.org/a.json",
            "https://1.2.3.4/list.json",
            "https://[::1]/list.json",
            "https://u:p@example.org/a.json",
            "https://example.org:8443/a.json",
            "../x/y",
            ".hidden/repo",
            "a b/c",
            "https://github.com/onlyowner",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(resolve_source(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_feed_cannot_vouch_for_itself() {
        let mut liar = entry("liar");
        liar.official = true;
        liar.likes = 9999;
        liar.downloads = 9999;
        liar.like_url = "https://evil.example/".into();
        liar.source = "Freedom Launcher".into();
        liar.source_url = String::new(); // claims to be the official catalog
        liar.asset = "x.zip".into();
        liar.issue = 7;
        liar.sha256 = "A".repeat(64);
        let out = third_party_entries(vec![liar], "https://example.org/list.json", "Example");
        assert_eq!(out.len(), 1);
        let e = &out[0];
        assert!(!e.official && e.likes == 0 && e.downloads == 0 && e.like_url.is_empty());
        assert_eq!(e.source, "Example");
        assert_eq!(e.source_url, "https://example.org/list.json");
        assert!(e.asset.is_empty() && e.issue == 0);
        assert_eq!(e.sha256, "a".repeat(64), "hashes are compared lowercase");
    }

    #[test]
    fn broken_or_dangerous_entries_are_dropped() {
        let mut list = vec![entry("ok"), entry("ok")]; // duplicate id
        let mut bad_id = entry("Bad Id");
        bad_id.id = "Bad Id".into();
        let reserved = entry("accounts");
        let mut http = entry("plain");
        http.url = "http://example.org/p.zip".into();
        let mut ip = entry("ip");
        ip.url = "https://10.0.0.1/p.zip".into();
        let mut short = entry("short");
        short.sha256 = "abc".into();
        let mut long = entry("long");
        long.description = "x".repeat(401);
        let mut ctl = entry("ctl");
        ctl.name = "bad\u{7}name".into();
        let mut many = entry("many");
        many.permissions = (0..17).map(|i| format!("p{i}")).collect();
        list.extend([bad_id, reserved, http, ip, short, long, ctl, many]);
        let out = third_party_entries(list, "https://example.org/l.json", "S");
        assert_eq!(
            out.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["ok"]
        );
    }

    /// The file `scripts/plugin-catalog-entry.py` writes is what the core reads.
    #[test]
    fn the_author_tool_output_is_accepted() {
        let feed: SourceFeed = serde_json::from_str(
            r#"{ "name": "You's plugins", "plugins": [ {
                "id": "ely2", "name": "Ely2", "version": "0.1.0", "author": "Freedom Launcher",
                "description": "Вход через Ely2", "permissions": ["accounts_provide", "network"],
                "url": "https://github.com/you/repo/releases/download/v2/my-plugin.zip",
                "sha256": "62ebb546b664d6a1c0f3e1a6b0f2f4a9f2a8f0f0e6c5a3d1b2c3d4e5f6a7b8c9", "size": 8848 } ] }"#,
        )
        .unwrap();
        let out = third_party_entries(
            feed.plugins,
            "https://example.org/l.json",
            &feed_name(&feed.name, "https://example.org/l.json"),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].source, "You's plugins");
        assert_eq!(out[0].permissions, vec!["accounts_provide", "network"]);
        assert_eq!(out[0].size, 8848);
    }

    #[test]
    fn one_source_cannot_flood_the_catalog() {
        let list: Vec<Entry> = (0..500).map(|i| entry(&format!("p{i}"))).collect();
        assert_eq!(
            third_party_entries(list, "https://example.org/l.json", "S").len(),
            MAX_PER_SOURCE
        );
    }

    #[test]
    fn the_archive_may_not_ask_for_more_than_the_entry_said() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(perms_within(&s(&["network"]), &s(&["network", "storage"])).is_ok());
        assert!(perms_within(&s(&[]), &s(&[])).is_ok());
        let e = perms_within(&s(&["network", "accounts_provide"]), &s(&["network"])).unwrap_err();
        assert!(
            e.contains("accounts_provide") && !e.contains("network,"),
            "{e}"
        );
    }

    #[test]
    fn another_source_cannot_take_over_an_installed_plugin() {
        const OTHER: &str = "https://example.org/list.json";
        // Nothing installed: anything goes.
        assert!(may_replace(false, "", false, OTHER).is_ok());
        // Same source updates its own plugin; a hand-made install may be replaced by a source.
        assert!(may_replace(true, OTHER, false, OTHER).is_ok());
        assert!(may_replace(true, "", false, OTHER).is_ok());
        // A plugin from one source is not replaced by another source, nor by the official one's rival.
        assert!(may_replace(true, OTHER, false, "https://evil.example/l.json").is_err());
        assert!(may_replace(true, OTHER, false, OFFICIAL).is_err());
        // A signed plugin (e.g. the official one) is not replaced by any foreign source,
        // even if it was put in by hand and so has no recorded source.
        assert!(may_replace(true, OFFICIAL, true, OTHER).is_err());
        assert!(may_replace(true, "", true, OTHER).is_err());
        assert!(may_replace(true, OFFICIAL, true, OFFICIAL).is_ok());
    }

    fn zip_with(path: &std::path::Path, names: &[(&str, &str)]) {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, c) in names {
            w.start_file(*n, o).unwrap();
            w.write_all(c.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn the_manifest_is_read_before_anything_is_installed() {
        let d = std::env::temp_dir().join(format!("fl-zm-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        let m = r#"{"id":"demo","name":"D","permissions":["network","storage"]}"#;
        zip_with(
            &d.join("top.zip"),
            &[("addon.json", m), ("index.html", "x")],
        );
        zip_with(
            &d.join("folder.zip"),
            &[("demo/addon.json", m), ("demo/index.html", "x")],
        );
        zip_with(&d.join("none.zip"), &[("readme.txt", "x")]);
        for f in ["top.zip", "folder.zip"] {
            let (id, perms) = addons::zip_manifest(&d.join(f)).unwrap();
            assert_eq!(
                (id.as_str(), perms),
                ("demo", vec!["network".to_string(), "storage".to_string()])
            );
        }
        assert!(addons::zip_manifest(&d.join("none.zip")).is_err());
        assert!(addons::zip_manifest(&d.join("missing.zip")).is_err());
        assert!(!addons::dir(&d).exists(), "peeking installs nothing");
        let _ = fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod end_to_end_support {
    pub use super::end_to_end::{archive, core, sha, Web, ZIP};
    use super::*;

    pub fn manifest_v(id: &str, version: &str, perms: &str) -> String {
        format!(r#"{{"id":"{id}","name":"Demo","version":"{version}","permissions":[{perms}]}}"#)
    }

    pub fn feed_v(id: &str, version: &str, perms: &[&str], zip: &[u8]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "name": "S",
            "plugins": [{ "id": id, "name": "Demo", "version": version, "url": ZIP,
                "sha256": sha(zip), "permissions": perms }]
        }))
        .unwrap()
    }
}

#[cfg(test)]
mod end_to_end {
    use super::*;
    use std::sync::Mutex;

    /// A pretend internet: url -> bytes. Records what was asked for.
    #[derive(Default)]
    pub struct Web {
        pub files: Mutex<BTreeMap<String, Vec<u8>>>,
        pub asked: Mutex<Vec<String>>,
    }
    impl Web {
        pub fn put(&self, url: &str, bytes: Vec<u8>) {
            self.files.lock().unwrap().insert(url.into(), bytes);
        }
    }
    impl Fetch for Web {
        async fn get(&self, url: &str, max: usize) -> Result<Vec<u8>, String> {
            self.asked.lock().unwrap().push(url.into());
            let b = self.files.lock().unwrap().get(url).cloned().ok_or("404")?;
            if b.len() > max {
                Err("файл слишком большой".into())
            } else {
                Ok(b)
            }
        }
    }

    const LIST: &str = "https://example.org/list.json";
    pub const ZIP: &str = "https://example.org/demo.zip";

    pub fn archive(manifest: &str) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("addon.json", o).unwrap();
        w.write_all(manifest.as_bytes()).unwrap();
        w.start_file("index.html", o).unwrap();
        w.write_all(b"<p>hi</p>").unwrap();
        w.finish().unwrap().into_inner()
    }

    pub fn sha(b: &[u8]) -> String {
        crate::signing::hex(&Sha256::digest(b))
    }

    fn feed(id: &str, perms: &[&str], zip: &[u8], name: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "name": name,
            "plugins": [{ "id": id, "name": "Demo", "version": "1.0.0", "url": ZIP,
                "sha256": sha(zip), "permissions": perms }]
        }))
        .unwrap()
    }

    pub fn core() -> std::sync::Arc<Core> {
        let d = std::env::temp_dir().join(format!("fl-e2e-{}", uuid::Uuid::new_v4()));
        Core::new(d, std::time::Instant::now(), false).unwrap()
    }

    fn manifest(id: &str, perms: &str) -> String {
        format!(r#"{{"id":"{id}","name":"Demo","version":"1.0.0","permissions":[{perms}]}}"#)
    }

    #[tokio::test]
    async fn add_a_source_once_and_install_from_it() {
        let c = core();
        let web = Web::default();
        let z = archive(&manifest("demo", r#""network""#));
        web.put(
            "https://raw.githubusercontent.com/someone/plugins/HEAD/freedom-plugins.json",
            feed("demo", &["network"], &z, "Someone's plugins"),
        );
        web.put(ZIP, z);

        // The repository shorthand is all the player types.
        let list = add_source_with(&c, &web, "someone/plugins").await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Someone's plugins");
        // Adding it again changes nothing.
        assert_eq!(
            add_source_with(&c, &web, "someone/plugins")
                .await
                .unwrap()
                .len(),
            1
        );

        let cat = catalog_with(&c, &web, true, false).await.unwrap();
        assert_eq!(cat.plugins.len(), 1);
        let e = &cat.plugins[0];
        assert!(!e.official && e.source == "Someone's plugins");
        let src = e.source_url.clone();

        // One call installs it. No folder to copy by hand.
        assert_eq!(
            install_with(&c, &web, "demo", &src, false).await.unwrap(),
            "demo"
        );
        assert!(addons::installed_ids(&c.data_dir).contains(&"demo".to_string()));
        // It is a community plugin (no signature) and not switched on: the player still approves it.
        assert_eq!(addons::trust(&c.data_dir, "demo"), crate::Trust::Community);
        let st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap();
        assert_eq!(st.addons["demo"].source, src);
        assert!(!st.addons["demo"].enabled && st.addons["demo"].granted.is_empty());
        // The same source may update it.
        assert!(install_with(&c, &web, "demo", &src, false).await.is_ok());
    }

    #[tokio::test]
    async fn a_wrong_hash_a_wrong_plugin_and_extra_permissions_are_refused() {
        let c = core();
        let web = Web::default();
        let good = archive(&manifest("demo", r#""network""#));
        let url = resolve_source(LIST).unwrap();
        web.put(&url, feed("demo", &["network"], &good, "S"));
        add_source_with(&c, &web, LIST).await.unwrap();
        let src = url.clone();

        // 1. The file is not the one the list described.
        web.put(ZIP, archive(&manifest("demo", r#""network","storage""#)));
        let e = install_with(&c, &web, "demo", &src, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("не совпал"), "{e}");

        // 2. Hash right, but it is another plugin inside.
        let other = archive(&manifest("other", r#""network""#));
        web.put(&url, feed("demo", &["network"], &other, "S"));
        web.put(ZIP, other);
        // The cached list still names the old hash, so refresh it first.
        let cat = catalog_with(&c, &web, true, false).await.unwrap();
        assert_eq!(
            cat.plugins[0].sha256,
            sha(&archive(&manifest("other", r#""network""#)))
        );
        let e = install_with(&c, &web, "demo", &src, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("другой плагин"), "{e}");

        // 3. Hash right, right id, but it asks for more than the list said.
        let greedy = archive(&manifest("demo", r#""network","accounts_provide""#));
        web.put(&url, feed("demo", &["network"], &greedy, "S"));
        web.put(ZIP, greedy);
        catalog_with(&c, &web, true, false).await.unwrap();
        let e = install_with(&c, &web, "demo", &src, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("accounts_provide"), "{e}");
        assert!(
            !addons::installed_ids(&c.data_dir).contains(&"demo".to_string()),
            "nothing was installed"
        );
    }

    #[tokio::test]
    async fn a_second_source_cannot_replace_a_plugin_from_the_first() {
        let c = core();
        let web = Web::default();
        let a = archive(&manifest("demo", r#""network""#));
        let url_a = resolve_source("https://a.example.org/list.json").unwrap();
        web.put(&url_a, feed("demo", &["network"], &a, "A"));
        web.put(ZIP, a);
        add_source_with(&c, &web, "https://a.example.org/list.json")
            .await
            .unwrap();
        install_with(&c, &web, "demo", &url_a, false).await.unwrap();

        // Source B lists the same id with different content.
        let b2 = {
            use std::io::Write;
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("addon.json", o).unwrap();
            w.write_all(manifest("demo", r#""network""#).as_bytes())
                .unwrap();
            w.start_file("index.html", o).unwrap();
            w.write_all(b"<p>different</p>").unwrap();
            w.finish().unwrap().into_inner()
        };
        let url_b = resolve_source("https://b.example.org/list.json").unwrap();
        let zip_b = "https://b.example.org/demo.zip";
        let feed_b = serde_json::to_vec(
            &serde_json::json!({ "plugins": [{ "id": "demo", "name": "Demo", "version": "9.9.9",
            "url": zip_b, "sha256": sha(&b2), "permissions": ["network"] }] }),
        )
        .unwrap();
        web.put(&url_b, feed_b);
        web.put(zip_b, b2);
        add_source_with(&c, &web, "https://b.example.org/list.json")
            .await
            .unwrap();

        let cat = catalog_with(&c, &web, true, false).await.unwrap();
        assert_eq!(
            cat.plugins.iter().filter(|e| e.id == "demo").count(),
            2,
            "both are listed, with their sources"
        );
        let e = install_with(&c, &web, "demo", &url_b, false)
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("другого источника"), "{e}");
        let st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap();
        assert_eq!(st.addons["demo"].source, url_a, "still A's");
    }

    #[tokio::test]
    async fn a_dead_source_is_reported_and_does_not_hide_the_others() {
        let c = core();
        let web = Web::default();
        let z = archive(&manifest("demo", r#""network""#));
        let ok = resolve_source("https://ok.example.org/list.json").unwrap();
        web.put(&ok, feed("demo", &["network"], &z, "Ok"));
        add_source_with(&c, &web, "https://ok.example.org/list.json")
            .await
            .unwrap();
        // A second source that was fine when added but is gone now.
        let gone = resolve_source("https://gone.example.org/list.json").unwrap();
        web.put(&gone, feed("demo2", &[], &z, "Gone"));
        add_source_with(&c, &web, "https://gone.example.org/list.json")
            .await
            .unwrap();
        web.files.lock().unwrap().remove(&gone);
        let _ = std::fs::remove_file(c.path(SRC_CACHE));

        let cat = catalog_with(&c, &web, true, false).await.unwrap();
        assert_eq!(
            cat.plugins.len(),
            1,
            "the working source still shows its plugin"
        );
        let bad = cat.sources.iter().find(|s| s.url == gone).unwrap();
        assert!(bad.error.is_some() && !bad.official);
        assert!(cat
            .sources
            .iter()
            .find(|s| s.url == ok)
            .unwrap()
            .error
            .is_none());
    }

    #[tokio::test]
    async fn adding_a_source_that_is_not_a_plugin_list_fails_now() {
        let c = core();
        let web = Web::default();
        web.put(
            "https://example.org/not.json",
            b"<html>hello</html>".to_vec(),
        );
        assert!(add_source_with(&c, &web, "https://example.org/not.json")
            .await
            .is_err());
        assert!(
            add_source_with(&c, &web, "https://example.org/missing.json")
                .await
                .is_err()
        );
        assert!(add_source_with(&c, &web, "http://example.org/x.json")
            .await
            .is_err());
        let st: crate::settings::Settings = crate::store::load(&c.path(SETTINGS_FILE)).unwrap();
        assert!(
            st.plugin_sources.is_empty(),
            "nothing is saved for a bad address"
        );
    }

    #[tokio::test]
    async fn removing_a_source_forgets_it_and_its_cache() {
        let c = core();
        let web = Web::default();
        let z = archive(&manifest("demo", r#""network""#));
        let url = resolve_source("https://example.org/list.json").unwrap();
        web.put(&url, feed("demo", &["network"], &z, "S"));
        add_source_with(&c, &web, "https://example.org/list.json")
            .await
            .unwrap();
        catalog_with(&c, &web, true, false).await.unwrap();
        assert!(remove_source(&c, &url).await.unwrap().is_empty());
        assert!(catalog_with(&c, &web, false, false)
            .await
            .unwrap()
            .plugins
            .is_empty());
    }
}

#[cfg(test)]
mod update_flow {
    use super::end_to_end_support::*;
    use super::*;

    #[tokio::test]
    async fn a_new_version_in_the_source_is_found_and_installed() {
        let c = core();
        let web = Web::default();
        let url = resolve_source("https://example.org/list.json").unwrap();
        let v1 = archive(&manifest_v("demo", "1.0.0", r#""network""#));
        web.put(&url, feed_v("demo", "1.0.0", &["network"], &v1));
        web.put(ZIP, v1);
        add_source_with(&c, &web, "https://example.org/list.json")
            .await
            .unwrap();
        install_with(&c, &web, "demo", &url, false).await.unwrap();
        assert!(
            updates_with(&c, &web, true, false)
                .await
                .unwrap()
                .is_empty(),
            "nothing newer yet"
        );

        // The player approves it and switches it on.
        crate::store::save(
            &c.path(addons::STATE),
            &{
                let mut st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap();
                let e = st.addons.get_mut("demo").unwrap();
                e.enabled = true;
                e.granted.insert(crate::Permission::Network);
                st
            },
            false,
        )
        .unwrap();

        // The author publishes 1.1.0, which also wants `storage` and says so in the list.
        let v2 = archive(&manifest_v("demo", "1.1.0", r#""network","storage""#));
        web.put(&url, feed_v("demo", "1.1.0", &["network", "storage"], &v2));
        web.put(ZIP, v2);
        let ups = updates_with(&c, &web, true, false).await.unwrap();
        assert_eq!(ups.len(), 1);
        assert_eq!(
            (
                ups[0].id.as_str(),
                ups[0].have.as_str(),
                ups[0].version.as_str()
            ),
            ("demo", "1.0.0", "1.1.0")
        );
        assert_eq!(ups[0].source_url, url);
        assert!(!ups[0].official);

        install_with(&c, &web, "demo", &ups[0].source_url, false)
            .await
            .unwrap();
        assert!(
            updates_with(&c, &web, true, false)
                .await
                .unwrap()
                .is_empty(),
            "now up to date"
        );
        // The new right is not given for free: still on, still has `network`, `storage` waits for the player.
        let st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap();
        let info = addons::list(&c.data_dir, &st)
            .into_iter()
            .find(|a| a["id"] == "demo")
            .unwrap();
        assert_eq!(info["version"], "1.1.0");
        assert_eq!(info["pending"], true, "a new permission has to be approved");
        let storage = info["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "storage")
            .unwrap();
        assert_eq!(storage["granted"], false);
        let net = info["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "network")
            .unwrap();
        assert_eq!(net["granted"], true, "what was approved stays approved");
    }

    #[tokio::test]
    async fn other_sources_and_hand_made_installs_get_no_update_offers() {
        let c = core();
        let web = Web::default();
        let a = resolve_source("https://a.example.org/l.json").unwrap();
        let b = resolve_source("https://b.example.org/l.json").unwrap();
        let v1 = archive(&manifest_v("demo", "1.0.0", r#""network""#));
        web.put(&a, feed_v("demo", "1.0.0", &["network"], &v1));
        web.put(ZIP, v1.clone());
        add_source_with(&c, &web, "https://a.example.org/l.json")
            .await
            .unwrap();
        install_with(&c, &web, "demo", &a, false).await.unwrap();
        // Source B lists a higher version of the same id. It is not where the plugin came from.
        let v9 = archive(&manifest_v("demo", "9.0.0", r#""network""#));
        web.put(&b, feed_v("demo", "9.0.0", &["network"], &v9));
        add_source_with(&c, &web, "https://b.example.org/l.json")
            .await
            .unwrap();
        assert!(updates_with(&c, &web, true, false)
            .await
            .unwrap()
            .is_empty());
        // A developer link is never updated from a catalog either.
        crate::store::save(
            &c.path(addons::STATE),
            &{
                let mut st: addons::State = crate::store::load(&c.path(addons::STATE)).unwrap();
                st.addons.get_mut("demo").unwrap().source = "dev".into();
                st
            },
            false,
        )
        .unwrap();
        web.put(&a, feed_v("demo", "2.0.0", &["network"], &v9));
        assert!(updates_with(&c, &web, true, false)
            .await
            .unwrap()
            .is_empty());
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn versions_compare_like_people_expect() {
        for (new, old) in [
            ("1.0.1", "1.0.0"),
            ("1.1", "1.0.9"),
            ("2", "1.99.99"),
            ("1.10.0", "1.9.0"),
            ("v1.2.0", "1.1.9"),
            ("1.0.0", "1.0.0-beta"),
            ("1.0.0-rc.2", "1.0.0-rc.1"),
            ("1.0.0-beta.10", "1.0.0-beta.9"),
            ("1.0.0-beta", "1.0.0-alpha"),
            ("1.0.0.1", "1.0.0"),
            ("1.0.0+b2", "0.9.9+b1"),
        ] {
            assert!(is_newer(new, old), "{new} should be newer than {old}");
            assert!(!is_newer(old, new), "{old} should not be newer than {new}");
        }
        for (a, b) in [
            ("1.0.0", "1.0.0"),
            ("1.0", "1.0.0"),
            ("v1.0.0", "1.0.0"),
            ("1.0.0+x", "1.0.0+y"),
            ("1.0.0-beta", "1.0.0-beta"),
        ] {
            assert!(
                !is_newer(a, b) && !is_newer(b, a),
                "{a} and {b} are the same version"
            );
        }
        // Unreadable versions never cause an update, in either position.
        for (a, b) in [
            ("latest", "1.0.0"),
            ("1.0.0", "latest"),
            ("", "1.0.0"),
            ("1.0.0", ""),
            ("1..0", "1.0.0"),
            ("-", "1.0.0"),
        ] {
            assert!(!is_newer(a, b), "{a:?} vs {b:?} must not be an update");
        }
    }

    #[test]
    fn where_an_installed_plugin_is_updated_from() {
        assert_eq!(
            effective_source("https://example.org/l.json", false).as_deref(),
            Some("https://example.org/l.json")
        );
        assert_eq!(
            effective_source("official", true).as_deref(),
            Some("official")
        );
        // Installed before sources were recorded, but signed: it came from the official catalog.
        assert_eq!(effective_source("", true).as_deref(), Some("official"));
        // By hand, or linked from a developer's folder: never touched automatically.
        assert_eq!(effective_source("", false), None);
        assert_eq!(effective_source("dev", true), None);
        assert_eq!(effective_source("dev", false), None);
    }
}
