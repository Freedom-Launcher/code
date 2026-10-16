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

/// A game version id as Modrinth lists them (`1.21.4`, `24w14a`, `1.21-pre1`).
pub fn valid_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 32
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_ ".contains(&b))
}

/// The modpack version to install: the first loader in `order` that has a release
/// wins (or any build of it), else the newest release, else the newest build.
pub fn pick_pack<'a>(versions: &'a [Version], order: &[&str]) -> Option<&'a Version> {
    let has = |v: &&Version, l: &str| v.loaders.iter().any(|x| x == l);
    order
        .iter()
        .find_map(|l| {
            let mut of = versions.iter().filter(|v| has(v, l));
            of.clone()
                .find(|v| v.version_type == "release")
                .or_else(|| of.next())
        })
        .or_else(|| versions.iter().find(|v| v.version_type == "release"))
        .or(versions.first())
}

/// One search request. Categories are ANDed, loaders ORed.
pub struct Query<'a> {
    pub text: &'a str,
    pub kind: Kind,
    pub mc: &'a str,
    pub loaders: &'a [&'a str],
    pub categories: &'a [String],
    /// Game versions ORed; used only when `mc` is empty (no instance).
    pub versions: &'a [String],
    /// `""` | `client` | `server` | `both`, as in Modrinth's environment filter.
    pub env: &'a str,
    pub open_source: bool,
    /// Categories a project must NOT have.
    pub exclude: &'a [String],
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
    } else {
        let vs: Vec<String> = q
            .versions
            .iter()
            .filter(|v| valid_version(v))
            .take(40)
            .map(|v| format!("versions:{v}"))
            .collect();
        if !vs.is_empty() {
            facets.push(json!(vs));
        }
    }
    let side =
        |s: &str, a: &str, b: &str| json!([format!("{s}_side:{a}"), format!("{s}_side:{b}")]);
    match q.env {
        "client" => facets.extend([
            side("client", "optional", "required"),
            side("server", "optional", "unsupported"),
        ]),
        "server" => facets.extend([
            side("client", "optional", "unsupported"),
            side("server", "optional", "required"),
        ]),
        "both" => facets.extend([
            json!(["client_side:required"]),
            json!(["server_side:required"]),
        ]),
        _ => {}
    }
    if q.open_source {
        facets.push(json!(["open_source:true"]));
    }
    for c in q.exclude.iter().filter(|c| valid_category(c)).take(8) {
        facets.push(json!([format!("categories!={c}")]));
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    pub version_number: String,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    pub date_published: String,
    #[serde(default)]
    pub downloads: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hashes {
    #[serde(default)]
    pub sha1: String,
}

/// What the project page shows: Modrinth's project, minus moderation fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPage {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub project_type: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub additional_categories: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub client_side: String,
    #[serde(default)]
    pub server_side: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub followers: u64,
    #[serde(default)]
    pub published: String,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub license: Option<License>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub issues_url: Option<String>,
    #[serde(default)]
    pub wiki_url: Option<String>,
    #[serde(default)]
    pub discord_url: Option<String>,
    #[serde(default)]
    pub donation_urls: Vec<Donation>,
    #[serde(default)]
    pub gallery: Vec<GalleryImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Donation {
    #[serde(default)]
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryImage {
    pub url: String,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub ordering: i64,
}

/// Modrinth project ids and slugs: letters, digits, `-`, `_`, `.`; short.
pub fn valid_project_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !id.starts_with('.')
}

/// The full project for its page; gallery in Modrinth's order.
pub async fn project_page(core: &Core, id: &str) -> Result<ProjectPage, CoreError> {
    let url = format!("{API}/project/{}", encode(id));
    let mut p: ProjectPage = core
        .http()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    p.gallery.sort_by_key(|g| g.ordering);
    Ok(p)
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
    ok.iter().find_map(|l| best(std::slice::from_ref(l)))
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
            versions: &[],
            env: "",
            open_source: false,
            exclude: &[],
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
    fn project_ids_are_plain() {
        assert!(valid_project_id("AANobbMI"));
        assert!(valid_project_id("fabric-api"));
        assert!(!valid_project_id(""));
        assert!(!valid_project_id("../user"));
        assert!(!valid_project_id("a/b"));
        assert!(!valid_project_id("a?b"));
        assert!(!valid_project_id(&"x".repeat(65)));
    }

    #[test]
    fn search_filters_versions_env_license_exclude() {
        let vs = vec![
            "1.21.4".to_string(),
            "1.20.1".to_string(),
            "bad\"v".to_string(),
        ];
        let ex = vec!["magic".to_string()];
        let u = search_url(&Query {
            versions: &vs,
            env: "client",
            open_source: true,
            exclude: &ex,
            ..q("", Kind::Mod, "", &["neoforge", "fabric"])
        });
        assert!(u.contains("versions%3A1.21.4%22%2C%22versions%3A1.20.1%22%5D"));
        assert!(u.contains("client_side%3Aoptional") && u.contains("server_side%3Aunsupported"));
        assert!(u.contains("open_source%3Atrue") && u.contains("categories%21%3Dmagic"));
        assert!(u.contains("categories%3Aneoforge%22%2C%22categories%3Afabric"));
        let locked = search_url(&Query {
            versions: &vs,
            ..q("", Kind::Mod, "1.19.2", &[])
        });
        assert!(locked.contains("1.19.2") && !locked.contains("1.20.1"));
    }

    #[test]
    fn pack_version_follows_loader_priority() {
        let v = |id: &str, t: &str, l: &str| -> Version {
            serde_json::from_value(
                json!({ "id": id, "project_id": "p", "version_number": id, "version_type": t,
                "loaders": [l], "game_versions": ["1.21.1"], "files": [] }),
            )
            .unwrap()
        };
        let list = vec![
            v("f", "release", "forge"),
            v("nb", "beta", "neoforge"),
            v("fa", "release", "fabric"),
        ];
        assert_eq!(pick_pack(&list, &["neoforge", "forge"]).unwrap().id, "nb");
        assert_eq!(pick_pack(&list, &["fabric", "neoforge"]).unwrap().id, "fa");
        assert_eq!(pick_pack(&list, &["quilt"]).unwrap().id, "f");
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
