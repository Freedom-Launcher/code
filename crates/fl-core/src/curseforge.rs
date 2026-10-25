//! CurseForge API v1, mapped onto the Modrinth types so search, the project
//! page, installs, updates and modpacks share one path with Modrinth.
//! CurseForge ids carry a `cf:` prefix (projects and files alike): they never
//! collide with Modrinth ids and tell every caller which catalog to ask.

use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek},
    sync::{Mutex, OnceLock},
};

use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::{
    game::download::Task,
    modrinth::{
        self, encode, Dependency, GalleryImage, Hashes, Hit, Kind, Project, ProjectPage,
        SearchPage, Version, VersionFile,
    },
    mods::{Author, Source},
    mrpack::{self, Plan},
    Core, CoreError,
};

pub const API: &str = "https://api.curseforge.com/v1";
/// Minecraft on CurseForge.
pub const GAME: u32 = 432;
pub const PREFIX: &str = "cf:";
pub const MANIFEST: &str = "manifest.json";
/// Hosts CurseForge serves project files from.
const FILE_HOSTS: &[&str] = &["edge.forgecdn.net", "mediafilez.forgecdn.net"];
/// Newest files shown on a project page.
const MAX_FILES: usize = 400;
/// Files a modpack may list.
const MAX_PACK_FILES: usize = 5000;

/// The API key: the runtime environment wins over the one baked in at build time.
pub fn key() -> Option<String> {
    std::env::var("FREEDOM_CF_API_KEY")
        .ok()
        .or_else(|| option_env!("FREEDOM_CF_API_KEY").map(str::to_owned))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub fn available() -> bool {
    key().is_some()
}

pub fn is_cf(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// The numeric CurseForge id behind `cf:<n>`.
pub fn num(id: &str) -> Option<u64> {
    let n = id.strip_prefix(PREFIX)?;
    if n.is_empty() || n.len() > 12 || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse().ok().filter(|n| *n > 0)
}

pub fn cid(n: u64) -> String {
    format!("{PREFIX}{n}")
}

/// The CurseForge class (section) a content kind lives in.
pub fn class_id(kind: Kind) -> u32 {
    match kind {
        Kind::Mod => 6,
        Kind::ResourcePack => 12,
        Kind::Shader => 6552,
        Kind::Datapack => 6945,
        Kind::Modpack => 4471,
    }
}

pub(crate) fn project_type(class: u32) -> &'static str {
    match class {
        12 => "resourcepack",
        6552 => "shader",
        6945 => "datapack",
        4471 => "modpack",
        _ => "mod",
    }
}

/// CurseForge `ModLoaderType` for a loader name.
pub fn loader_type(l: &str) -> Option<u32> {
    match l {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

pub(crate) fn loader_of_type(t: u32) -> Option<&'static str> {
    match t {
        1 => Some("forge"),
        4 => Some("fabric"),
        5 => Some("quilt"),
        6 => Some("neoforge"),
        _ => None,
    }
}

/// CurseForge has no relevance order; popularity stands in for it.
pub fn sort_field(sort: &str) -> u32 {
    match sort {
        "downloads" => 6,
        "follows" => 12,
        "newest" => 11,
        "updated" => 3,
        _ => 2,
    }
}

/// Whether a download URL is a CurseForge file host (https only).
pub fn download_ok(url: &str) -> bool {
    reqwest::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| FILE_HOSTS.contains(&h))
    })
}

fn bad(m: impl Into<String>) -> CoreError {
    CoreError::BadArgs(m.into())
}

/// `null` reads as the default, as CurseForge sends `null` for empty fields.
fn nn<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize)]
struct Resp<T> {
    data: T,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    #[serde(default, deserialize_with = "nn")]
    total_count: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
    pub id: u64,
    #[serde(default, deserialize_with = "nn")]
    pub name: String,
    #[serde(default, deserialize_with = "nn")]
    pub slug: String,
    #[serde(default, deserialize_with = "nn")]
    pub summary: String,
    #[serde(default, deserialize_with = "nn")]
    pub class_id: u32,
    #[serde(default, deserialize_with = "nn")]
    pub links: Links,
    #[serde(default, deserialize_with = "nn")]
    pub authors: Vec<CfAuthor>,
    #[serde(default)]
    pub logo: Option<Asset>,
    #[serde(default, deserialize_with = "nn")]
    pub screenshots: Vec<Asset>,
    #[serde(default, deserialize_with = "nn")]
    pub download_count: f64,
    #[serde(default, deserialize_with = "nn")]
    pub thumbs_up_count: u64,
    #[serde(default, deserialize_with = "nn")]
    pub categories: Vec<Category>,
    #[serde(default, deserialize_with = "nn")]
    pub date_created: String,
    #[serde(default, deserialize_with = "nn")]
    pub date_modified: String,
    #[serde(default, deserialize_with = "nn")]
    pub latest_files_indexes: Vec<FileIndex>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    #[serde(default, deserialize_with = "nn")]
    pub website_url: String,
    #[serde(default, deserialize_with = "nn")]
    pub wiki_url: String,
    #[serde(default, deserialize_with = "nn")]
    pub issues_url: String,
    #[serde(default, deserialize_with = "nn")]
    pub source_url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfAuthor {
    #[serde(default, deserialize_with = "nn")]
    pub id: u64,
    #[serde(default, deserialize_with = "nn")]
    pub name: String,
    #[serde(default, deserialize_with = "nn")]
    pub url: String,
    #[serde(default, rename = "avatarUrl", deserialize_with = "nn")]
    pub avatar_url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    #[serde(default, deserialize_with = "nn")]
    pub title: String,
    #[serde(default, deserialize_with = "nn")]
    pub description: String,
    #[serde(default, deserialize_with = "nn")]
    pub thumbnail_url: String,
    #[serde(default, deserialize_with = "nn")]
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    #[serde(default, deserialize_with = "nn")]
    pub id: u64,
    #[serde(default, deserialize_with = "nn")]
    pub slug: String,
    #[serde(default, deserialize_with = "nn")]
    pub name: String,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub parent_category_id: Option<u64>,
    #[serde(default)]
    pub is_class: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    #[serde(default, deserialize_with = "nn")]
    pub game_version: String,
    #[serde(default)]
    pub mod_loader: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
    pub id: u64,
    #[serde(default, deserialize_with = "nn")]
    pub mod_id: u64,
    #[serde(default, deserialize_with = "nn")]
    pub display_name: String,
    #[serde(default, deserialize_with = "nn")]
    pub file_name: String,
    #[serde(default, deserialize_with = "nn")]
    pub release_type: u32,
    #[serde(default, deserialize_with = "nn")]
    pub file_date: String,
    #[serde(default, deserialize_with = "nn")]
    pub file_length: u64,
    #[serde(default, deserialize_with = "nn")]
    pub download_count: f64,
    #[serde(default, deserialize_with = "nn")]
    pub game_versions: Vec<String>,
    #[serde(default, deserialize_with = "nn")]
    pub dependencies: Vec<CfDep>,
    /// `null` when the author does not allow downloads outside CurseForge.
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default, deserialize_with = "nn")]
    pub hashes: Vec<CfHash>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CfDep {
    #[serde(default, deserialize_with = "nn")]
    pub mod_id: u64,
    #[serde(default, deserialize_with = "nn")]
    pub relation_type: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CfHash {
    #[serde(default, deserialize_with = "nn")]
    pub value: String,
    #[serde(default, deserialize_with = "nn")]
    pub algo: u32,
}

// ---------- Mapping onto the Modrinth types ----------

fn sha1_of(hashes: &[CfHash]) -> String {
    hashes
        .iter()
        .find(|h| h.algo == 1 && h.value.len() == 40 && h.value.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|h| h.value.to_ascii_lowercase())
        .unwrap_or_default()
}

/// A CurseForge file as a Modrinth version. Loaders come from the file's tags;
/// an untagged jar is taken as Forge (old mods), an untagged zip fits packs and
/// shaders. A file the author keeps on CurseForge only has no files.
pub fn to_version(f: CfFile) -> Version {
    let mut loaders: Vec<String> = Vec::new();
    let mut games = Vec::new();
    for g in &f.game_versions {
        let l = g.to_ascii_lowercase();
        if matches!(l.as_str(), "forge" | "fabric" | "quilt" | "neoforge" | "iris" | "optifine") {
            if !loaders.contains(&l) {
                loaders.push(l);
            }
        } else if g.starts_with(|c: char| c.is_ascii_digit()) && modrinth::valid_version(g) {
            games.push(g.clone());
        }
    }
    if loaders.is_empty() {
        loaders = if f.file_name.ends_with(".jar") {
            vec!["forge".into()]
        } else {
            ["minecraft", "datapack", "iris", "optifine"].map(String::from).to_vec()
        };
    }
    let version_type = match f.release_type {
        2 => "beta",
        3 => "alpha",
        _ => "release",
    };
    let dependencies = f
        .dependencies
        .iter()
        .filter(|d| d.mod_id > 0)
        .map(|d| Dependency {
            project_id: Some(cid(d.mod_id)),
            dependency_type: match d.relation_type {
                3 => "required",
                2 => "optional",
                5 => "incompatible",
                1 => "embedded",
                _ => "other",
            }
            .into(),
        })
        .collect();
    let files = f
        .download_url
        .as_deref()
        .filter(|u| download_ok(u) && !f.file_name.is_empty())
        .map(|u| VersionFile {
            url: u.to_owned(),
            filename: f.file_name.clone(),
            primary: true,
            size: f.file_length,
            hashes: Hashes { sha1: sha1_of(&f.hashes) },
        })
        .into_iter()
        .collect();
    let number = f
        .file_name
        .rsplit_once('.')
        .map_or(f.file_name.as_str(), |(a, _)| a)
        .to_owned();
    let name = if f.display_name.trim().is_empty() { f.file_name.clone() } else { f.display_name };
    Version {
        id: cid(f.id),
        project_id: cid(f.mod_id),
        name,
        version_number: number,
        changelog: None,
        date_published: f.file_date,
        downloads: f.download_count.max(0.0) as u64,
        version_type: version_type.into(),
        loaders,
        game_versions: games,
        dependencies,
        files,
    }
}

pub fn icon(m: &CfMod) -> Option<String> {
    m.logo
        .as_ref()
        .map(|l| if l.thumbnail_url.is_empty() { l.url.clone() } else { l.thumbnail_url.clone() })
        .filter(|u| u.starts_with("https://"))
}

pub fn author(m: &CfMod) -> Option<Author> {
    m.authors.first().filter(|a| !a.name.is_empty()).map(|a| Author {
        id: cid(a.id),
        name: a.name.clone(),
        avatar: None,
        org: false,
    })
}

fn to_hit(m: &CfMod) -> Hit {
    Hit {
        project_id: cid(m.id),
        slug: m.slug.clone(),
        title: m.name.clone(),
        description: m.summary.clone(),
        author: m.authors.first().map(|a| a.name.clone()).unwrap_or_default(),
        author_id: m.authors.first().map(|a| a.id.to_string()).unwrap_or_default(),
        icon_url: icon(m),
        downloads: m.download_count.max(0.0) as u64,
        follows: m.thumbs_up_count,
        categories: m.categories.iter().map(|c| c.slug.clone()).collect(),
        date_modified: m.date_modified.clone(),
    }
}

fn to_project(m: &CfMod) -> Project {
    Project {
        id: cid(m.id),
        title: m.name.clone(),
        icon_url: icon(m),
        slug: m.slug.clone(),
        team: String::new(),
        organization: None,
        license: None,
        client_side: String::new(),
        server_side: String::new(),
    }
}

fn some(s: &str) -> Option<String> {
    Some(s.to_owned()).filter(|s| s.starts_with("https://") || s.starts_with("http://"))
}

fn to_page(m: CfMod, body: String) -> ProjectPage {
    let mut loaders: Vec<String> = Vec::new();
    let mut games: Vec<String> = Vec::new();
    for i in &m.latest_files_indexes {
        if let Some(l) = i.mod_loader.and_then(loader_of_type) {
            if !loaders.iter().any(|x| x == l) {
                loaders.push(l.into());
            }
        }
        if modrinth::valid_version(&i.game_version) && !games.contains(&i.game_version) {
            games.push(i.game_version.clone());
        }
    }
    ProjectPage {
        id: cid(m.id),
        slug: m.slug.clone(),
        title: m.name.clone(),
        description: m.summary.clone(),
        body,
        project_type: project_type(m.class_id).into(),
        icon_url: icon(&m),
        categories: m.categories.iter().map(|c| c.slug.clone()).collect(),
        additional_categories: Vec::new(),
        loaders,
        game_versions: games,
        client_side: String::new(),
        server_side: String::new(),
        downloads: m.download_count.max(0.0) as u64,
        followers: m.thumbs_up_count,
        published: m.date_created.clone(),
        updated: m.date_modified.clone(),
        license: None,
        source_url: some(&m.links.source_url),
        issues_url: some(&m.links.issues_url),
        wiki_url: some(&m.links.wiki_url),
        discord_url: None,
        donation_urls: Vec::new(),
        gallery: m
            .screenshots
            .iter()
            .filter(|s| s.url.starts_with("https://"))
            .enumerate()
            .map(|(i, s)| GalleryImage {
                url: s.url.clone(),
                featured: i == 0,
                title: Some(s.title.clone()).filter(|t| !t.is_empty()),
                description: Some(s.description.clone()).filter(|t| !t.is_empty()),
                ordering: i as i64,
            })
            .collect(),
        web_url: some(&m.links.website_url),
    }
}

// ---------- Requests ----------

fn need_key() -> Result<String, CoreError> {
    key().ok_or_else(|| CoreError::msg("CurseForge недоступен: в этой сборке лаунчера нет ключа API"))
}

async fn get<T: DeserializeOwned>(core: &Core, url: &str) -> Result<T, CoreError> {
    let k = need_key()?;
    Ok(core
        .http()
        .get(url)
        .header("x-api-key", k)
        .header("Accept", "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

async fn post<T: DeserializeOwned>(core: &Core, url: &str, body: Value) -> Result<T, CoreError> {
    let k = need_key()?;
    Ok(core
        .http()
        .post(url)
        .header("x-api-key", k)
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

fn id_of(id: &str) -> Result<u64, CoreError> {
    num(id).ok_or_else(|| bad("Неверный проект CurseForge"))
}

/// One search page, the same query the Modrinth search takes. Exclusions,
/// environment and the open-source filter have no CurseForge counterpart.
pub async fn search(core: &Core, q: &modrinth::Query<'_>) -> Result<SearchPage, CoreError> {
    let class = class_id(q.kind);
    let limit = q.limit.clamp(1, 50);
    let index = q.offset.min(10_000 - limit);
    let mut url = format!(
        "{API}/mods/search?gameId={GAME}&classId={class}&sortField={}&sortOrder=desc&index={index}&pageSize={limit}",
        sort_field(q.sort)
    );
    let text = q.text.trim();
    if !text.is_empty() {
        url.push_str(&format!("&searchFilter={}", encode(text)));
    }
    if !q.mc.is_empty() {
        url.push_str(&format!("&gameVersion={}", encode(q.mc)));
    } else {
        let vs: Vec<&str> = q
            .versions
            .iter()
            .filter(|v| modrinth::valid_version(v))
            .take(20)
            .map(String::as_str)
            .collect();
        if !vs.is_empty() {
            url.push_str(&format!("&gameVersions={}", encode(&json!(vs).to_string())));
        }
    }
    if matches!(q.kind, Kind::Mod | Kind::Modpack) {
        let mut types: Vec<u32> = Vec::new();
        for t in q.loaders.iter().filter_map(|l| loader_type(l)) {
            if !types.contains(&t) {
                types.push(t);
            }
        }
        if !types.is_empty() {
            url.push_str(&format!("&modLoaderTypes={}", encode(&json!(types).to_string())));
        }
    }
    if !q.categories.is_empty() {
        if let Ok(cats) = categories(core, q.kind).await {
            let ids: Vec<u64> = q
                .categories
                .iter()
                .filter_map(|s| cats.iter().find(|c| &c.slug == s).map(|c| c.id))
                .take(10)
                .collect();
            if !ids.is_empty() {
                url.push_str(&format!("&categoryIds={}", encode(&json!(ids).to_string())));
            }
        }
    }
    let r: Resp<Vec<CfMod>> = get(core, &url).await?;
    let total = r.pagination.map_or(0, |p| p.total_count).min(10_000);
    Ok(SearchPage {
        hits: r.data.iter().map(to_hit).collect(),
        total_hits: total,
    })
}

fn cats_cache() -> &'static Mutex<HashMap<u32, Vec<Category>>> {
    static C: OnceLock<Mutex<HashMap<u32, Vec<Category>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// The categories of a kind's class (cached for the session).
pub async fn categories(core: &Core, kind: Kind) -> Result<Vec<Category>, CoreError> {
    let class = class_id(kind);
    let cached = {
        let m = cats_cache().lock();
        m.ok().and_then(|m| m.get(&class).cloned())
    };
    if let Some(v) = cached {
        return Ok(v);
    }
    let r: Resp<Vec<Category>> =
        get(core, &format!("{API}/categories?gameId={GAME}&classId={class}")).await?;
    let v: Vec<Category> = r
        .data
        .into_iter()
        .filter(|c| c.is_class != Some(true) && c.id != u64::from(class) && !c.slug.is_empty())
        .collect();
    if let Ok(mut m) = cats_cache().lock() {
        m.insert(class, v.clone());
    }
    Ok(v)
}

/// Category filters for the catalog: grouped by parent, named as CurseForge names them.
pub fn category_rows(kind: Kind, cats: &[Category]) -> Vec<Value> {
    let class = u64::from(class_id(kind));
    let by: HashMap<u64, &Category> = cats.iter().map(|c| (c.id, c)).collect();
    let mut rows: Vec<&Category> = cats.iter().collect();
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    rows.into_iter()
        .map(|c| {
            let parent = c.parent_category_id.filter(|p| *p != class).and_then(|p| by.get(&p));
            let (h, hl) = parent.map_or(("categories", ""), |p| (p.slug.as_str(), p.name.as_str()));
            json!({ "name": c.slug, "label": c.name, "header": h, "header_label": hl })
        })
        .collect()
}

pub async fn project(core: &Core, id: &str) -> Result<Project, CoreError> {
    let n = id_of(id)?;
    let r: Resp<CfMod> = get(core, &format!("{API}/mods/{n}")).await?;
    Ok(to_project(&r.data))
}

/// The project page: the project, its HTML description and screenshots.
pub async fn project_page(core: &Core, id: &str) -> Result<ProjectPage, CoreError> {
    let n = id_of(id)?;
    let (m, d) = futures_util::future::try_join(
        get::<Resp<CfMod>>(core, &format!("{API}/mods/{n}")),
        get::<Resp<String>>(core, &format!("{API}/mods/{n}/description")),
    )
    .await?;
    Ok(to_page(m.data, d.data))
}

fn newest_first(vs: &mut [Version]) {
    vs.sort_by(|a, b| b.date_published.cmp(&a.date_published));
}

/// The newest files of a project (up to `MAX_FILES`), newest first.
pub async fn project_versions(core: &Core, id: &str) -> Result<Vec<Version>, CoreError> {
    let n = id_of(id)?;
    let mut out = Vec::new();
    let mut index = 0usize;
    loop {
        let r: Resp<Vec<CfFile>> =
            get(core, &format!("{API}/mods/{n}/files?index={index}&pageSize=50")).await?;
        let got = r.data.len();
        out.extend(r.data.into_iter().map(to_version));
        index += 50;
        let total = r.pagination.map_or(0, |p| p.total_count) as usize;
        if got < 50 || index >= total || index >= MAX_FILES {
            break;
        }
    }
    newest_first(&mut out);
    Ok(out)
}

/// Files for one game version and any of `loaders` (newest first).
pub async fn versions(
    core: &Core,
    id: &str,
    mc: &str,
    loaders: &[&str],
) -> Result<Vec<Version>, CoreError> {
    let n = id_of(id)?;
    let mut types: Vec<u32> = Vec::new();
    for t in loaders.iter().filter_map(|l| loader_type(l)) {
        if !types.contains(&t) {
            types.push(t);
        }
    }
    let gv = if mc.is_empty() { String::new() } else { format!("&gameVersion={}", encode(mc)) };
    let urls: Vec<String> = if types.is_empty() {
        vec![format!("{API}/mods/{n}/files?pageSize=50{gv}")]
    } else {
        types
            .iter()
            .map(|t| format!("{API}/mods/{n}/files?pageSize=50{gv}&modLoaderType={t}"))
            .collect()
    };
    let pages = futures_util::future::try_join_all(
        urls.iter().map(|u| get::<Resp<Vec<CfFile>>>(core, u)),
    )
    .await?;
    let mut seen = HashSet::new();
    let mut out: Vec<Version> = pages
        .into_iter()
        .flat_map(|p| p.data)
        .filter(|f| seen.insert(f.id))
        .map(to_version)
        .collect();
    newest_first(&mut out);
    Ok(out)
}

/// One file of a project.
pub async fn version(core: &Core, project: &str, id: &str) -> Result<Version, CoreError> {
    let (m, f) = (id_of(project)?, id_of(id)?);
    let r: Resp<CfFile> = get(core, &format!("{API}/mods/{m}/files/{f}")).await?;
    Ok(to_version(r.data))
}

/// Several projects at once.
/// Every project an author is credited on, most downloaded first (at most 200).
pub async fn by_author(core: &Core, author: u64) -> Result<Vec<CfMod>, CoreError> {
    let mut out: Vec<CfMod> = Vec::new();
    let mut index = 0u64;
    loop {
        let url = format!(
            "{API}/mods/search?gameId={GAME}&authorId={author}&sortField=6&sortOrder=desc&index={index}&pageSize=50"
        );
        let r: Resp<Vec<CfMod>> = get(core, &url).await?;
        let n = r.data.len() as u64;
        let total = r.pagination.map_or(0, |p| p.total_count);
        out.extend(r.data);
        index += n;
        if n < 50 || index >= total || index >= 200 {
            break;
        }
    }
    Ok(out)
}

pub async fn mods(core: &Core, ids: &[u64]) -> Result<Vec<CfMod>, CoreError> {
    let mut out = Vec::new();
    for chunk in ids.chunks(500) {
        let r: Resp<Vec<CfMod>> = post(core, &format!("{API}/mods"), json!({ "modIds": chunk })).await?;
        out.extend(r.data);
    }
    Ok(out)
}

async fn files(core: &Core, ids: &[u64]) -> Result<Vec<CfFile>, CoreError> {
    let mut out = Vec::new();
    for chunk in ids.chunks(500) {
        let r: Resp<Vec<CfFile>> =
            post(core, &format!("{API}/mods/files"), json!({ "fileIds": chunk })).await?;
        out.extend(r.data);
    }
    Ok(out)
}

// ---------- Modpacks (CurseForge export: manifest.json + overrides/) ----------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default, deserialize_with = "nn")]
    pub minecraft: McInfo,
    #[serde(default, deserialize_with = "nn")]
    pub manifest_type: String,
    #[serde(default, deserialize_with = "nn")]
    pub name: String,
    #[serde(default, deserialize_with = "nn")]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub overrides: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McInfo {
    #[serde(default, deserialize_with = "nn")]
    pub version: String,
    #[serde(default, deserialize_with = "nn")]
    pub mod_loaders: Vec<ManifestLoader>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ManifestLoader {
    #[serde(default, deserialize_with = "nn")]
    pub id: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Deserialize)]
pub struct ManifestFile {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    #[serde(default = "yes")]
    pub required: bool,
}

fn yes() -> bool {
    true
}

/// A CurseForge pack: has `manifest.json` and no Modrinth index.
pub fn is_pack<R: Read + Seek>(z: &mut zip::ZipArchive<R>) -> bool {
    let cf = z.by_name(MANIFEST).is_ok();
    let mr = z.by_name("modrinth.index.json").is_ok();
    cf && !mr
}

pub fn read_manifest<R: Read + Seek>(z: &mut zip::ZipArchive<R>) -> Result<Manifest, CoreError> {
    let mut e = z.by_name(MANIFEST).map_err(|_| bad("Это не сборка CurseForge"))?;
    let mut s = String::new();
    e.by_ref().take(16 << 20).read_to_string(&mut s)?;
    let m: Manifest =
        serde_json::from_str(&s).map_err(|e| bad(format!("Повреждённый {MANIFEST}: {e}")))?;
    if !m.manifest_type.is_empty() && m.manifest_type != "minecraftModpack" {
        return Err(bad("Неподдерживаемый формат сборки"));
    }
    Ok(m)
}

/// The pack's loader: `forge-47.2.0`, `neoforge-21.1.77`, `fabric-0.16.9`…
pub fn pack_loader(m: &Manifest) -> Result<(String, String), CoreError> {
    let l = m.minecraft.mod_loaders.iter().find(|l| l.primary).or(m.minecraft.mod_loaders.first());
    let Some(l) = l else {
        return Ok(("vanilla".into(), String::new()));
    };
    let (name, ver) = l
        .id
        .split_once('-')
        .ok_or_else(|| bad(format!("Неизвестный загрузчик: {}", l.id)))?;
    let name = name.to_ascii_lowercase();
    let ok = !ver.is_empty()
        && ver.len() <= 64
        && ver.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b));
    if loader_type(&name).is_none() || !ok {
        return Err(bad(format!("Неизвестный загрузчик: {}", l.id)));
    }
    Ok((name, ver.to_owned()))
}

fn folder_for(class: u32, file: &str) -> &'static str {
    match class {
        6 => "mods",
        12 => "resourcepacks",
        6552 => "shaderpacks",
        6945 => "datapacks",
        _ if file.ends_with(".jar") => "mods",
        _ => "resourcepacks",
    }
}

/// Resolves a pack's files into download tasks (paths relative to the instance)
/// and their catalog origin. Files whose authors keep them on CurseForge only
/// cannot be downloaded by a launcher; they are listed in `blocked`.
pub async fn pack_plan(core: &Core, m: Manifest) -> Result<Plan, CoreError> {
    let mc = m.minecraft.version.trim().to_owned();
    if mc.is_empty() || !modrinth::valid_version(&mc) {
        return Err(bad("В сборке не указана версия Minecraft"));
    }
    let (loader, loader_version) = pack_loader(&m)?;
    let want: Vec<u64> = m
        .files
        .iter()
        .filter(|f| f.required && f.file_id > 0 && f.project_id > 0)
        .map(|f| f.file_id)
        .collect();
    if want.len() > MAX_PACK_FILES {
        return Err(bad("В сборке слишком много файлов"));
    }
    let got = files(core, &want).await?;
    let mut mod_ids: Vec<u64> = got.iter().map(|f| f.mod_id).collect();
    mod_ids.sort_unstable();
    mod_ids.dedup();
    let by_mod: HashMap<u64, CfMod> = mods(core, &mod_ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.id, m))
        .collect();
    let found: HashSet<u64> = got.iter().map(|f| f.id).collect();
    let mut blocked: Vec<String> = want
        .iter()
        .filter(|id| !found.contains(id))
        .map(|id| format!("файл {id}"))
        .collect();
    let mut blocked_files = Vec::new();
    let mut tasks = Vec::new();
    let mut index = Vec::new();
    let mut seen = HashSet::new();
    for f in got {
        if !seen.insert(f.id) {
            continue;
        }
        let meta = by_mod.get(&f.mod_id);
        let title = meta
            .map(|m| m.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| f.file_name.clone());
        let name = f.file_name.clone();
        if name.is_empty()
            || name.starts_with('.')
            || name.contains(['/', '\\', ':'])
            || name.contains("..")
        {
            return Err(bad(format!("Недопустимое имя файла: {name}")));
        }
        let rel_s = format!("{}/{name}", folder_for(meta.map_or(0, |m| m.class_id), &name));
        let rel = mrpack::safe_rel(&rel_s).ok_or_else(|| bad(format!("Недопустимый путь: {rel_s}")))?;
        let (fid, mod_id, flen, fsha) = (f.id, f.mod_id, f.file_length, sha1_of(&f.hashes));
        let v = to_version(f);
        let source = Source {
            project_id: v.project_id.clone(),
            version_id: v.id.clone(),
            title: title.clone(),
            version_number: v.version_number.clone(),
            icon_url: meta.and_then(icon),
            slug: meta.map(|m| m.slug.clone()).unwrap_or_default(),
            author: meta.and_then(author),
            deps: v.required().map(str::to_owned).collect(),
            published: v.date_published.clone(),
            ..Default::default()
        };
        let Some(file) = v.primary_file().cloned() else {
            blocked_files.push(crate::blocked::Blocked {
                title: title.clone(),
                file_name: name.clone(),
                rel: rel_s.clone(),
                sha1: fsha,
                size: flen,
                url: site_download_url(meta, mod_id, fid),
                source: Some(source),
            });
            blocked.push(title);
            continue;
        };
        tasks.push(Task {
            url: file.url,
            path: rel,
            sha1: file.hashes.sha1,
            size: file.size,
        });
        index.push((rel_s, source));
    }
    let overrides = m
        .overrides
        .as_deref()
        .map(|o| o.trim().trim_matches('/'))
        .filter(|o| !o.is_empty())
        .unwrap_or("overrides")
        .to_owned();
    if mrpack::safe_rel(&overrides).is_none() {
        return Err(bad(format!("Недопустимый путь: {overrides}")));
    }
    let name = match m.name.trim() {
        "" => "Сборка".to_owned(),
        n => n.chars().take(64).collect(),
    };
    Ok(Plan {
        name,
        mc,
        loader,
        loader_version,
        tasks,
        overrides,
        blocked,
        blocked_files,
        index,
    })
}

/// The browser page that downloads one file on curseforge.com.
fn site_download_url(meta: Option<&CfMod>, mod_id: u64, file_id: u64) -> String {
    match meta.filter(|m| !m.slug.is_empty() && m.slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')) {
        Some(m) => {
            let section = match m.class_id {
                12 => "texture-packs",
                6552 => "shaders",
                6945 => "data-packs",
                17 => "worlds",
                _ => "mc-mods",
            };
            format!("https://www.curseforge.com/minecraft/{section}/{}/download/{file_id}", m.slug)
        }
        None => format!("https://www.curseforge.com/projects/{mod_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(num("cf:238222"), Some(238222));
        assert_eq!(num("cf:"), None);
        assert_eq!(num("cf:0"), None);
        assert_eq!(num("cf:12a"), None);
        assert_eq!(num("AANobbMI"), None);
        assert_eq!(cid(5), "cf:5");
    }

    #[test]
    fn file_maps_to_version() {
        let f: CfFile = serde_json::from_value(json!({
            "id": 9268985, "modId": 238222, "displayName": "15.74.0.227 for Forge 1.20.1",
            "fileName": "jei-1.20.1-forge-15.74.0.227.jar", "releaseType": 2,
            "fileDate": "2026-10-20T17:04:56.703Z", "fileLength": 6892654,
            "gameVersions": ["Client", "1.20.1", "Forge", "NeoForge", "Server"],
            "dependencies": [{"modId": 1689768, "relationType": 3}, {"modId": 1700987, "relationType": 2}],
            "downloadUrl": "https://edge.forgecdn.net/files/9268/985/jei-1.20.1-forge-15.74.0.227.jar",
            "hashes": [{"value": "A9F7FB95DC766FF71066E1F374DD9219D22D726A", "algo": 1}, {"value": "182b", "algo": 2}],
            "downloadCount": null
        }))
        .unwrap();
        let v = to_version(f);
        assert_eq!(v.id, "cf:9268985");
        assert_eq!(v.project_id, "cf:238222");
        assert_eq!(v.version_type, "beta");
        assert_eq!(v.loaders, ["forge", "neoforge"]);
        assert_eq!(v.game_versions, ["1.20.1"]);
        assert_eq!(v.version_number, "jei-1.20.1-forge-15.74.0.227");
        assert_eq!(v.required().collect::<Vec<_>>(), ["cf:1689768"]);
        let f = v.primary_file().unwrap();
        assert_eq!(f.hashes.sha1, "a9f7fb95dc766ff71066e1f374dd9219d22d726a");
        assert!(modrinth::pick(std::slice::from_ref(&v), "1.20.1", &["neoforge"]).is_some());
    }

    #[test]
    fn blocked_file_has_no_download() {
        let f: CfFile = serde_json::from_value(json!({
            "id": 1, "modId": 2, "fileName": "x.jar", "downloadUrl": null, "gameVersions": ["1.20.1", "Fabric"]
        }))
        .unwrap();
        assert!(to_version(f).files.is_empty());
        let f: CfFile = serde_json::from_value(json!({
            "id": 1, "modId": 2, "fileName": "x.jar", "downloadUrl": "https://evil.example/x.jar"
        }))
        .unwrap();
        assert!(to_version(f).files.is_empty());
    }

    #[test]
    fn untagged_files() {
        let zip: CfFile = serde_json::from_value(json!({ "id": 1, "modId": 2, "fileName": "pack.zip", "gameVersions": ["1.21.1"] })).unwrap();
        let v = to_version(zip);
        assert!(v.loaders.iter().any(|l| l == "minecraft") && v.loaders.iter().any(|l| l == "iris"));
        let jar: CfFile = serde_json::from_value(json!({ "id": 1, "modId": 2, "fileName": "old.jar", "gameVersions": ["1.7.10"] })).unwrap();
        assert_eq!(to_version(jar).loaders, ["forge"]);
    }

    #[test]
    fn manifest_loader() {
        let m: Manifest = serde_json::from_value(json!({
            "minecraft": { "version": "1.20.1", "modLoaders": [{ "id": "forge-47.2.0", "primary": true }] },
            "manifestType": "minecraftModpack", "name": "P",
            "files": [{ "projectID": 1, "fileID": 2 }, { "projectID": 3, "fileID": 4, "required": false }],
            "overrides": "overrides"
        }))
        .unwrap();
        assert_eq!(pack_loader(&m).unwrap(), (String::from("forge"), String::from("47.2.0")));
        assert!(m.files[0].required && !m.files[1].required);
        let n: Manifest = serde_json::from_value(json!({
            "minecraft": { "version": "1.20.1", "modLoaders": [{ "id": "neoforge-1.20.1-47.1.84", "primary": true }] }
        }))
        .unwrap();
        assert_eq!(pack_loader(&n).unwrap(), (String::from("neoforge"), String::from("1.20.1-47.1.84")));
        let bad: Manifest = serde_json::from_value(json!({
            "minecraft": { "version": "1.20.1", "modLoaders": [{ "id": "liteloader-1", "primary": true }] }
        }))
        .unwrap();
        assert!(pack_loader(&bad).is_err());
    }

    #[test]
    fn hosts() {
        assert!(download_ok("https://edge.forgecdn.net/files/1/2/a.jar"));
        assert!(!download_ok("http://edge.forgecdn.net/files/1/2/a.jar"));
        assert!(!download_ok("https://edge.forgecdn.net.evil.com/a.jar"));
    }

    #[test]
    fn folders() {
        assert_eq!(folder_for(6, "a.jar"), "mods");
        assert_eq!(folder_for(12, "a.zip"), "resourcepacks");
        assert_eq!(folder_for(6552, "a.zip"), "shaderpacks");
        assert_eq!(folder_for(0, "a.jar"), "mods");
    }
}