//! Share-by-code: an instance's recipe (game version, loader, catalog content) as one
//! pasteable string. Only catalog ids travel; files, worlds and settings stay at home.
//!
//! Three forms are accepted by [`resolve`]:
//! * `FL2-…` — the recipe as deflated JSON in url-safe base64 (what [`encode`] writes);
//! * `FL1-…` — the 0.3.43 form, plain JSON in base64 (still read);
//! * `FL-<key>` or a pastes.dev / bytebin link — a short link to an uploaded `FL2-` code.
use std::io::{Read, Write};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{modrinth::Kind, CoreError};

pub const PREFIX: &str = "FL1-";
pub const PREFIX2: &str = "FL2-";
pub const LINK_PREFIX: &str = "FL-";
pub const MAX_ITEMS: usize = 512;
/// Content kinds that travel, with their one-letter tag in the code.
pub const KINDS: [(Kind, &str); 3] = [(Kind::Mod, "m"), (Kind::ResourcePack, "r"), (Kind::Shader, "s")];

const PASTE_API: &str = "https://api.pastes.dev/";
const PASTE_VIEW: &str = "https://pastes.dev/";
const BYTEBIN: &str = "https://bytebin.lucko.me/";
/// pastes.dev asks for an app name with contact information.
const UA: &str = concat!("FreedomLauncher/", env!("FL_VERSION"), " (github.com/Freedom-Launcher)");
const MAX_FETCH: usize = 256 * 1024;
const MAX_JSON: u64 = 1 << 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
    pub n: String,
    pub mc: String,
    pub l: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lv: String,
    /// `(kind tag, project id, version id)`.
    #[serde(default)]
    pub m: Vec<(String, String, String)>,
}

pub fn kind_of(tag: &str) -> Option<Kind> {
    KINDS.iter().find(|(_, t)| *t == tag).map(|(k, _)| *k)
}

fn ok_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_'))
}

fn bad() -> CoreError {
    CoreError::BadArgs("Это не код сборки Freedom Launcher".into())
}

fn b64url(data: &[u8]) -> String {
    crate::skins::b64_encode(data)
        .chars()
        .filter(|c| *c != '=')
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect()
}

fn unb64url(body: &str) -> Option<Vec<u8>> {
    if body.is_empty() || body.len() > 200_000 {
        return None;
    }
    let std: String = body
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    crate::skins::b64_decode(&std)
}

/// The compact offline code (`FL2-`).
pub fn encode(r: &Recipe) -> String {
    let json = serde_json::to_vec(r).expect("recipe serializes");
    let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&json).expect("in-memory write");
    let packed = z.finish().expect("in-memory write");
    format!("{PREFIX2}{}", b64url(&packed))
}

/// The 0.3.43 form, kept for tests and for reading old codes.
pub fn encode_v1(r: &Recipe) -> String {
    let json = serde_json::to_vec(r).expect("recipe serializes");
    format!("{PREFIX}{}", b64url(&json))
}

pub fn decode(code: &str) -> Result<Recipe, CoreError> {
    let s: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = if let Some(body) = s.strip_prefix(PREFIX2) {
        let raw = unb64url(body).ok_or_else(bad)?;
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(&raw[..])
            .take(MAX_JSON)
            .read_to_end(&mut out)
            .map_err(|_| bad())?;
        out
    } else if let Some(body) = s.strip_prefix(PREFIX) {
        unb64url(body).ok_or_else(bad)?
    } else {
        return Err(bad());
    };
    let r: Recipe = serde_json::from_slice(&bytes).map_err(|_| bad())?;
    let items_ok = r.m.len() <= MAX_ITEMS
        && r.m.iter().all(|(k, p, v)| kind_of(k).is_some() && ok_id(p) && ok_id(v));
    if !items_ok || r.mc.is_empty() || r.mc.len() > 64 || r.l.len() > 32 || r.lv.len() > 64 {
        return Err(bad());
    }
    Ok(r)
}

fn ok_key(k: &str) -> bool {
    !k.is_empty() && k.len() <= 32 && k.chars().all(|c| c.is_ascii_alphanumeric())
}

fn token_key(tok: &str) -> Option<String> {
    let s = tok.trim_matches(|c: char| matches!(c, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'' | ',' | '.' | '!' | '?' | '«' | '»'));
    let s = s.trim_end_matches('/');
    let (bb, key) = if s.len() > 3 && s.get(..3).is_some_and(|p| p.eq_ignore_ascii_case(LINK_PREFIX)) {
        let k = &s[3..];
        match k.strip_prefix("b.") {
            Some(k) => (true, k),
            None => (false, k),
        }
    } else {
        let rest = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        if let Some(k) = rest.strip_prefix("pastes.dev/").or_else(|| rest.strip_prefix("api.pastes.dev/")) {
            (false, k)
        } else if let Some(k) = rest.strip_prefix("bytebin.lucko.me/") {
            (true, k)
        } else {
            return None;
        }
    };
    let key = key.split(['?', '#']).next().unwrap_or("");
    ok_key(key).then(|| if bb { format!("b.{key}") } else { key.to_owned() })
}

/// The paste key in a short code or link, also when it is part of a longer message.
/// Bytebin keys carry a `b.` mark.
pub fn link_key(input: &str) -> Option<String> {
    input.split_whitespace().find_map(token_key)
}

/// The page a person can open in a browser.
pub fn view_url(key: &str) -> String {
    match key.strip_prefix("b.") {
        Some(k) => format!("{BYTEBIN}{k}"),
        None => format!("{PASTE_VIEW}{key}"),
    }
}

fn raw_url(key: &str) -> String {
    match key.strip_prefix("b.") {
        Some(k) => format!("{BYTEBIN}{k}"),
        None => format!("{PASTE_API}{key}"),
    }
}

async fn post(c: &crate::Core, url: &str, body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct K {
        key: String,
    }
    let r = c
        .http()
        .post(url)
        .header("User-Agent", UA)
        .header("Content-Type", "text/plain")
        .timeout(Duration::from_secs(15))
        .body(body.to_owned())
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        return None;
    }
    let k: K = r.json().await.ok()?;
    ok_key(&k.key).then_some(k.key)
}

/// Uploads a code (only on the user's click — pastes.dev rules) and returns its key:
/// pastes.dev first, bytebin when it is unreachable.
pub async fn upload(c: &crate::Core, code: &str) -> Result<String, CoreError> {
    if let Some(k) = post(c, &format!("{PASTE_API}post"), code).await {
        return Ok(k);
    }
    if let Some(k) = post(c, &format!("{BYTEBIN}post"), code).await {
        return Ok(format!("b.{k}"));
    }
    Err(CoreError::msg("Не удалось создать ссылку — проверьте интернет"))
}

/// A recipe from whatever was pasted: a code, a short code or a link.
pub async fn resolve(c: &crate::Core, input: &str) -> Result<Recipe, CoreError> {
    let t = input.trim();
    if t.starts_with(PREFIX) || t.starts_with(PREFIX2) {
        return decode(t);
    }
    let Some(key) = link_key(t) else { return decode(t) };
    let net = || CoreError::msg("Не удалось открыть ссылку — проверьте интернет");
    let r = c
        .http()
        .get(raw_url(&key))
        .header("User-Agent", UA)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|_| net())?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(CoreError::msg("Ссылка не найдена — возможно, она устарела"));
    }
    if !r.status().is_success() {
        return Err(net());
    }
    let body = r.bytes().await.map_err(|_| net())?;
    if body.len() > MAX_FETCH {
        return Err(bad());
    }
    decode(&String::from_utf8_lossy(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Recipe {
        Recipe {
            n: "Моя сборка ✨".into(),
            mc: "1.21.1".into(),
            l: "fabric".into(),
            lv: "0.16.5".into(),
            m: vec![
                ("m".into(), "AANobbMI".into(), "abc12345".into()),
                ("s".into(), "cf:394468".into(), "5101366".into()),
            ],
        }
    }

    #[test]
    fn roundtrip() {
        let code = encode(&sample());
        assert!(code.starts_with(PREFIX2));
        assert!(!code.contains(['+', '/', '=']));
        assert_eq!(decode(&code).unwrap(), sample());
        // Pasted with line breaks and spaces.
        let messy = format!("  {}\n{}  ", &code[..20], &code[20..]);
        assert_eq!(decode(&messy).unwrap(), sample());
    }

    #[test]
    fn reads_v1_codes() {
        let code = encode_v1(&sample());
        assert!(code.starts_with(PREFIX));
        assert_eq!(decode(&code).unwrap(), sample());
    }

    #[test]
    fn compact_is_shorter() {
        let mut r = sample();
        for n in 0..120 {
            r.m.push(("m".into(), format!("P{n:07}"), format!("V{n:07}")));
        }
        assert!(encode(&r).len() * 10 < encode_v1(&r).len() * 8);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("hello").is_err());
        assert!(decode("FL1-").is_err());
        assert!(decode("FL2-").is_err());
        assert!(decode("FL1-!!!!").is_err());
        assert!(decode("FL2-AAAA").is_err());
        let mut r = sample();
        r.m.push(("x".into(), "a".into(), "b".into()));
        assert!(decode(&encode(&r)).is_err());
        let mut r = sample();
        r.m[0].1 = "../etc".into();
        assert!(decode(&encode(&r)).is_err());
    }

    #[test]
    fn link_keys() {
        assert_eq!(link_key("FL-ZGJ9m48j8y").as_deref(), Some("ZGJ9m48j8y"));
        assert_eq!(link_key(" fl-ZGJ9m48j8y ").as_deref(), Some("ZGJ9m48j8y"));
        assert_eq!(link_key("FL-b.CPlDkLZl7n").as_deref(), Some("b.CPlDkLZl7n"));
        assert_eq!(link_key("https://pastes.dev/ZGJ9m48j8y").as_deref(), Some("ZGJ9m48j8y"));
        assert_eq!(link_key("pastes.dev/ZGJ9m48j8y/").as_deref(), Some("ZGJ9m48j8y"));
        assert_eq!(link_key("https://bytebin.lucko.me/CPlDkLZl7n").as_deref(), Some("b.CPlDkLZl7n"));
        assert_eq!(link_key("Заходи: «https://pastes.dev/abc123»!").as_deref(), Some("abc123"));
        assert_eq!(link_key("https://evil.example/abc"), None);
        assert_eq!(link_key("FL-../x"), None);
        assert_eq!(link_key("FL1-abc"), None);
        assert_eq!(view_url("b.k1"), "https://bytebin.lucko.me/k1");
        assert_eq!(view_url("k1"), "https://pastes.dev/k1");
    }
}
