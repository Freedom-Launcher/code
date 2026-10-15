//! Modrinth API v2: mod search, project info and version picking.
//! Only the fields the launcher uses are parsed.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Core, CoreError};

pub const API: &str = "https://api.modrinth.com/v2";

/// Modrinth loader for an instance loader; `None` means the instance takes no mods.
pub fn loader_name(loader: &str) -> Option<&'static str> {
    match loader {
        "fabric" => Some("fabric"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        "quilt" => Some("quilt"),
        _ => None,
    }
}

/// Percent-encodes everything outside RFC 3986 unreserved characters.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn search_url(query: &str, mc: &str, loader: &str, offset: u32, limit: u32) -> String {
    let facets = json!([
        ["project_type:mod"],
        [format!("versions:{mc}")],
        [format!("categories:{loader}")]
    ])
    .to_string();
    let q = query.trim();
    let index = if q.is_empty() {
        "downloads"
    } else {
        "relevance"
    };
    format!(
        "{API}/search?query={}&facets={}&index={index}&offset={offset}&limit={}",
        encode(q),
        encode(&facets),
        limit.clamp(1, 50)
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchPage {
    pub hits: Vec<Hit>,
    #[serde(default)]
    pub total_hits: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    #[serde(default)]
    pub version_type: String,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Hashes {
    #[serde(default)]
    pub sha1: String,
}

impl Version {
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
    /// Projects this version cannot run without.
    pub fn required(&self) -> impl Iterator<Item = &str> {
        self.dependencies
            .iter()
            .filter(|d| d.dependency_type == "required")
            .filter_map(|d| d.project_id.as_deref())
    }
}

/// Modrinth lists versions newest first: take the newest release that fits,
/// otherwise the newest beta/alpha that fits.
pub fn pick<'a>(versions: &'a [Version], mc: &str, loader: &str) -> Option<&'a Version> {
    let fits = |v: &&Version| {
        v.game_versions.iter().any(|g| g == mc)
            && v.loaders.iter().any(|l| l == loader)
            && v.primary_file().is_some()
    };
    versions
        .iter()
        .filter(fits)
        .find(|v| v.version_type == "release")
        .or_else(|| versions.iter().find(fits))
}

pub async fn search(
    core: &Core,
    query: &str,
    mc: &str,
    loader: &str,
    offset: u32,
    limit: u32,
) -> Result<SearchPage, CoreError> {
    let url = search_url(query, mc, loader, offset, limit);
    Ok(core
        .http()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub async fn project(core: &Core, id: &str) -> Result<Project, CoreError> {
    let url = format!("{API}/project/{}", encode(id));
    Ok(core
        .http()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub async fn versions(
    core: &Core,
    id: &str,
    mc: &str,
    loader: &str,
) -> Result<Vec<Version>, CoreError> {
    let url = format!(
        "{API}/project/{}/version?loaders={}&game_versions={}",
        encode(id),
        encode(&json!([loader]).to_string()),
        encode(&json!([mc]).to_string())
    );
    Ok(core
        .http()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// For each sha1, the newest version of the same project that fits `mc` and
/// `loader` (releases only when `release_only`). Hashes Modrinth does not
/// know are simply missing from the map.
pub async fn latest_by_hash(
    core: &Core,
    hashes: &[String],
    mc: &str,
    loader: &str,
    release_only: bool,
) -> Result<std::collections::HashMap<String, Version>, CoreError> {
    if hashes.is_empty() {
        return Ok(Default::default());
    }
    let mut body = json!({
        "hashes": hashes,
        "algorithm": "sha1",
        "loaders": [loader],
        "game_versions": [mc],
    });
    if release_only {
        body["version_types"] = json!(["release"]);
    }
    Ok(core
        .http()
        .post(format!("{API}/version_files/update"))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, kind: &str, mc: &str, loader: &str) -> Version {
        serde_json::from_value(json!({
            "id": id, "project_id": "p", "version_number": id, "version_type": kind,
            "loaders": [loader], "game_versions": [mc],
            "files": [{ "url": "https://cdn.modrinth.com/x.jar", "filename": "x.jar", "primary": true }]
        }))
        .unwrap()
    }

    #[test]
    fn search_url_encodes_query_and_facets() {
        let u = search_url("fabric api & co", "1.21.4", "fabric", 20, 500);
        assert!(u.starts_with(
            "https://api.modrinth.com/v2/search?query=fabric%20api%20%26%20co&facets="
        ));
        assert!(u.contains("versions%3A1.21.4"));
        assert!(u.contains("categories%3Afabric"));
        assert!(u.contains("index=relevance"));
        assert!(u.ends_with("offset=20&limit=50"));
        assert!(search_url("  ", "1.20.1", "forge", 0, 10).contains("index=downloads"));
    }

    #[test]
    fn pick_prefers_newest_fitting_release() {
        let list = vec![
            v("beta", "beta", "1.21.4", "fabric"),
            v("other-mc", "release", "1.21.5", "fabric"),
            v("other-loader", "release", "1.21.4", "forge"),
            v("rel", "release", "1.21.4", "fabric"),
            v("old", "release", "1.21.4", "fabric"),
        ];
        assert_eq!(pick(&list, "1.21.4", "fabric").unwrap().id, "rel");
        assert_eq!(pick(&list[..1], "1.21.4", "fabric").unwrap().id, "beta");
        assert!(pick(&list, "1.8.9", "fabric").is_none());
    }

    #[test]
    fn vanilla_takes_no_mods() {
        assert_eq!(loader_name("vanilla"), None);
        assert_eq!(loader_name("neoforge"), Some("neoforge"));
    }
}
