//! Share-by-code: an instance's recipe (game version, loader, catalog content) as one
//! pasteable string. Only catalog ids travel; files, worlds and settings stay at home.
use serde::{Deserialize, Serialize};

use crate::{modrinth::Kind, CoreError};

pub const PREFIX: &str = "FL1-";
pub const MAX_ITEMS: usize = 512;
/// Content kinds that travel, with their one-letter tag in the code.
pub const KINDS: [(Kind, &str); 3] = [(Kind::Mod, "m"), (Kind::ResourcePack, "r"), (Kind::Shader, "s")];

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

pub fn encode(r: &Recipe) -> String {
    let json = serde_json::to_vec(r).expect("recipe serializes");
    let body: String = crate::skins::b64_encode(&json)
        .chars()
        .filter(|c| *c != '=')
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect();
    format!("{PREFIX}{body}")
}

pub fn decode(code: &str) -> Result<Recipe, CoreError> {
    let bad = || CoreError::BadArgs("Это не код сборки Freedom Launcher".into());
    let s: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let body = s.strip_prefix(PREFIX).ok_or_else(bad)?;
    if body.is_empty() || body.len() > 200_000 {
        return Err(bad());
    }
    let std: String = body
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    let bytes = crate::skins::b64_decode(&std).ok_or_else(bad)?;
    let r: Recipe = serde_json::from_slice(&bytes).map_err(|_| bad())?;
    let items_ok = r.m.len() <= MAX_ITEMS
        && r.m.iter().all(|(k, p, v)| kind_of(k).is_some() && ok_id(p) && ok_id(v));
    if !items_ok || r.mc.is_empty() || r.mc.len() > 64 || r.l.len() > 32 || r.lv.len() > 64 {
        return Err(bad());
    }
    Ok(r)
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
        assert!(code.starts_with(PREFIX));
        assert!(!code.contains(['+', '/', '=']));
        assert_eq!(decode(&code).unwrap(), sample());
        // Pasted with line breaks and spaces.
        let messy = format!("  {}\n{}  ", &code[..20], &code[20..]);
        assert_eq!(decode(&messy).unwrap(), sample());
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode("hello").is_err());
        assert!(decode("FL1-").is_err());
        assert!(decode("FL1-!!!!").is_err());
        let mut r = sample();
        r.m.push(("x".into(), "a".into(), "b".into()));
        assert!(decode(&encode(&r)).is_err());
        let mut r = sample();
        r.m[0].1 = "../etc".into();
        assert!(decode(&encode(&r)).is_err());
    }
}