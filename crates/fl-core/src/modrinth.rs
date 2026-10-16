//! Modrinth API v2: content search, project info and version picking.
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

/// Modrinth loaders whose mods run on `loader`: Quilt also loads Fabric mods.
pub fn accepts(loader: &str) -> Vec<&str> {
    match loader {
        "quilt" => vec!["quilt", "fabric"],
        l => vec![l],
    }
}

/// What a Modrinth project installs as. Mods are jars in `mods/`; resource
/// packs and shaders are zips in their own folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Mod,
    ResourcePack,
    Shader,
    /// Lives in a world's `datapacks/` folder, so it is installed per world.
    Datapack,
    /// A whole `.mrpack`; installing one creates a new instance.
    Modpack,
}

impl Kind {
    pub fn project_type(self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::ResourcePack => "resourcepack",
            Kind::Shader => "shader",
            Kind::Datapack => "datapack",
            Kind::Modpack => "modpack",
        }
    }
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Mod => "mods",
            Kind::ResourcePack => "resourcepacks",
            Kind::Shader => "shaderpacks",
            Kind::Datapack => "datapacks",
            Kind::Modpack => "modpacks",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            Kind::Mod => ".jar",
            Kind::Modpack => ".mrpack",
            _ => ".zip",
        }
    }
    /// Modrinth loaders whose files fit an instance on `loader`, best first;
    /// `None` when the instance cannot use this kind at all. Shaders need
    /// Iris (Fabric, Quilt, NeoForge) or OptiFine (Forge).
    pub fn loaders(self, loader: &str) -> Option<Vec<&'static str>> {
        match self {
            Kind::Mod => loader_name(loader).map(accepts),
            Kind::ResourcePack => Some(vec!["minecraft"]),
            Kind::Shader => match loader {
                "fabric" | "quilt" | "neoforge" => Some(vec!["iris", "optifine"]),
                "forge" => Some(vec!["optifine"]),
                _ => None,
            },
            Kind::Datapack => Some(vec!["datapack"]),
            // Modpacks are not installed into an instance.
            Kind::Modpack => None,
        }
    }
    /// `loaders`, also refusing data packs before 1.13 (they did not exist).
    pub fn supports(self, loader: &str, mc: &str) -> Option<Vec<&'static str>> {
        if self == Kind::Datapack && !datapacks_ok(mc) {
            return None;
        }
        self.loaders(loader)
    }
    /// Data packs reuse the mod category list on Modrinth.
    pub fn category_type(self) -> &'static str {
        match self {
            Kind::Datapack => "mod",
            k => k.project_type(),
        }
    }
}

/// Data packs arrived in 1.13; unknown version formats are assumed newer.
pub fn datapacks_ok(mc: &str) -> bool {
    match mc.strip_prefix("1.") {
        Some(rest) => rest
            .split(['.', '-', ' '])
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .is_none_or(|n| n >= 13),
        None => true,
    }
}

/// Modrinth search order; an empty query has nothing to be relevant to.
pub fn sort_index(sort: &str, query: &str) -> &'static str {
    match sort {
        "downloads" => "downloads",
        "follows" => "follows",
        "newest" => "newest",
        "updated" => "updated",
        "relevance" if !query.trim().is_empty() => "relevance",
        _ if query.trim().is_empty() => "downloads",
        _ => "relevance",
    }
}

/// Category slugs from the Modrinth tag list: lowercase words and dashes.
pub fn valid_category(c: &str) -> bool {
    !c.is_empty()
        && c.len() <= 32
        && c.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One search request. Categories are ANDed, loaders ORed.
pub struct Query<'a> {
    pub text: &'a str,
    pub kind: Kind,
    pub mc: &'a str,
    pub loaders: &'a [&'a str],
    pub categories: &'a [String],
    pub sort: &'a str,
    pub offset: u32,
    pub limit: u32,
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

pub fn search_url(q: &Query) -> String {
    let mut facets = vec![json!([format!("project_type:{}", q.kind.project_type())])];
    // Browsing without an instance may leave the game version open.
    if !q.mc.is_empty() {
        facets.push(json!([format!("versions:{}", q.mc)]));
    }
    // Resource and data packs carry no loader category in the search index.
    if !matches!(q.kind, Kind::ResourcePack | Kind::Datapack) && !q.loaders.is_empty() {
        facets.push(json!(q
            .loaders
            .iter()
            .map(|l| format!("categories:{l}"))
            .collect::<Vec<_>>()));
    }
    for c in q.categories.iter().filter(|c| valid_category(c)).take(8) {
        facets.push(json!([format!("categories:{c}")]));
    }
    let text = q.text.trim();
    format!(
        "{API}/search?query={}&facets={}&index={}&offset={}&limit={}",
        encode(text),
        encode(&json!(facets).to_string()),
        sort_index(q.sort, text),
        q.offset,
        q.limit.clamp(1, 50)
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
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub date_modified: String,
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
/// otherwise the newest beta/alpha that fits. A build for the loader itself
/// beats one it merely accepts (a Quilt build over a Fabric one).
pub fn pick<'a>(versions: &'a [Version], mc: &str, ok: &[&str]) -> Option<&'a Version> {
    if ok.is_empty() {
        return None;
    }
    let best = |names: &[&str]| {
        let fits = |v: &&Version| {
            v.game_versions.iter().any(|g| g == mc)
                && v.loaders.iter().any(|l| names.contains(&l.as_str()))
                && v.primary_file().is_some()
        };
        versions
            .iter()
            .filter(fits)
            .find(|v| v.version_type == "release")
            .or_else(|| versions.iter().find(fits))
    };
    best(&ok[..1]).or_else(|| best(ok))
}

pub async fn search(core: &Core, q: &Query<'_>) -> Result<SearchPage, CoreError> {
    Ok(core
        .http()
        .get(search_url(q))
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
    loaders: &[&str],
) -> Result<Vec<Version>, CoreError> {
    let url = format!(
        "{API}/project/{}/version?loaders={}&game_versions={}",
        encode(id),
        encode(&json!(loaders).to_string()),
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

/// Every version of a project, newest first.
pub async fn project_versions(core: &Core, id: &str) -> Result<Vec<Version>, CoreError> {
    let url = format!("{API}/project/{}/version", encode(id));
    Ok(core
        .http()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// The exact version each sha1 belongs to; unknown hashes are missing.
pub async fn versions_by_hash(
    core: &Core,
    hashes: &[String],
) -> Result<std::collections::HashMap<String, Version>, CoreError> {
    if hashes.is_empty() {
        return Ok(Default::default());
    }
    Ok(core
        .http()
        .post(format!("{API}/version_files"))
        .json(&json!({ "hashes": hashes, "algorithm": "sha1" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Several projects at once, in chunks so the URL stays short.
pub async fn projects(core: &Core, ids: &[String]) -> Result<Vec<Project>, CoreError> {
    let mut out = Vec::new();
    for chunk in ids.chunks(100) {
        let url = format!("{API}/projects?ids={}", encode(&json!(chunk).to_string()));
        let page: Vec<Project> = core
            .http()
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        out.extend(page);
    }
    Ok(out)
}

/// For each sha1, the newest version of the same project that fits `mc` and
/// `loader` (releases only when `release_only`). Hashes Modrinth does not
/// know are simply missing from the map.
pub async fn latest_by_hash(
    core: &Core,
    hashes: &[String],
    mc: &str,
    loaders: &[&str],
    release_only: bool,
) -> Result<std::collections::HashMap<String, Version>, CoreError> {
    if hashes.is_empty() {
        return Ok(Default::default());
    }
    let mut body = json!({
        "hashes": hashes,
        "algorithm": "sha1",
        "loaders": loaders,
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

    fn q<'a>(text: &'a str, kind: Kind, mc: &'a str, loaders: &'a [&'a str]) -> Query<'a> {
        Query {
            text,
            kind,
            mc,
            loaders,
            categories: &[],
            sort: "",
            offset: 0,
            limit: 20,
        }
    }

    #[test]
    fn search_url_encodes_query_and_facets() {
        let u = search_url(&Query {
            offset: 20,
            limit: 500,
            ..q("fabric api & co", Kind::Mod, "1.21.4", &["fabric"])
        });
        assert!(u.starts_with(
            "https://api.modrinth.com/v2/search?query=fabric%20api%20%26%20co&facets="
        ));
        assert!(u.contains("versions%3A1.21.4"));
        assert!(u.contains("categories%3Afabric"));
        assert!(u.contains("index=relevance"));
        assert!(u.ends_with("offset=20&limit=50"));
        assert!(search_url(&q("  ", Kind::Mod, "1.20.1", &["forge"])).contains("index=downloads"));
        assert!(search_url(&Query {
            sort: "relevance",
            ..q(" ", Kind::Mod, "1.20.1", &[])
        })
        .contains("index=downloads"));
        assert!(search_url(&Query {
            sort: "updated",
            ..q("x", Kind::Mod, "1.20.1", &[])
        })
        .contains("index=updated"));
    }

    #[test]
    fn kinds_have_their_own_facets_and_loaders() {
        let cats = vec!["optimization".to_string(), "bad\"cat".to_string()];
        let u = search_url(&Query {
            categories: &cats,
            ..q("", Kind::Shader, "1.21.4", &["iris", "optifine"])
        });
        assert!(u.contains("project_type%3Ashader"));
        assert!(u.contains("categories%3Airis%22%2C%22categories%3Aoptifine"));
        assert!(u.contains("categories%3Aoptimization"));
        assert!(!u.contains("bad"));
        let r = search_url(&q("", Kind::ResourcePack, "1.21.4", &["minecraft"]));
        assert!(r.contains("project_type%3Aresourcepack") && !r.contains("categories%3Aminecraft"));
        assert_eq!(
            Kind::ResourcePack.loaders("vanilla"),
            Some(vec!["minecraft"])
        );
        assert_eq!(Kind::Shader.loaders("vanilla"), None);
        assert_eq!(Kind::Shader.loaders("forge"), Some(vec!["optifine"]));
        assert_eq!(Kind::Mod.loaders("quilt"), Some(vec!["quilt", "fabric"]));
        assert_eq!(
            (Kind::Shader.folder(), Kind::Shader.ext()),
            ("shaderpacks", ".zip")
        );
    }

    #[test]
    fn datapacks_and_modpacks() {
        assert!(datapacks_ok("1.13") && datapacks_ok("1.21.4") && datapacks_ok("26.1"));
        assert!(datapacks_ok("24w14a") && !datapacks_ok("1.12.2") && !datapacks_ok("1.8"));
        assert_eq!(
            Kind::Datapack.supports("vanilla", "1.20.1"),
            Some(vec!["datapack"])
        );
        assert_eq!(Kind::Datapack.supports("fabric", "1.12.2"), None);
        assert_eq!(Kind::Modpack.supports("fabric", "1.21.4"), None);
        assert_eq!(Kind::Datapack.category_type(), "mod");
        let d = search_url(&q("", Kind::Datapack, "1.21.4", &["datapack"]));
        assert!(d.contains("project_type%3Adatapack") && !d.contains("categories%3Adatapack"));
        let m = search_url(&q("", Kind::Modpack, "", &["fabric"]));
        assert!(m.contains("project_type%3Amodpack") && m.contains("categories%3Afabric"));
        assert!(!m.contains("versions%3A"));
        assert_eq!(Kind::Modpack.ext(), ".mrpack");
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
        assert_eq!(pick(&list, "1.21.4", &["fabric"]).unwrap().id, "rel");
        assert_eq!(pick(&list[..1], "1.21.4", &["fabric"]).unwrap().id, "beta");
        assert!(pick(&list, "1.8.9", &["fabric"]).is_none());
    }

    #[test]
    fn vanilla_takes_no_mods() {
        assert_eq!(loader_name("vanilla"), None);
        assert_eq!(loader_name("neoforge"), Some("neoforge"));
    }

    #[test]
    fn quilt_takes_fabric_mods() {
        assert_eq!(accepts("quilt"), ["quilt", "fabric"]);
        assert_eq!(accepts("forge"), ["forge"]);
        let u = search_url(&q("", Kind::Mod, "1.21.1", &accepts("quilt")));
        assert!(u.contains("categories%3Aquilt%22%2C%22categories%3Afabric"));
        let list = vec![
            v("fab", "release", "1.21.1", "fabric"),
            v("qbeta", "beta", "1.21.1", "quilt"),
        ];
        assert_eq!(
            pick(&list, "1.21.1", &accepts("quilt")).unwrap().id,
            "qbeta"
        );
        assert_eq!(
            pick(&list[..1], "1.21.1", &accepts("quilt")).unwrap().id,
            "fab"
        );
        assert!(pick(&list[..1], "1.21.1", &accepts("forge")).is_none());
    }
}
