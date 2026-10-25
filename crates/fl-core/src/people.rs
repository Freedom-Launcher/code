//! Creators of catalog projects: who made a project, and a creator's own page —
//! profile, members or organizations, and every public project — for Modrinth
//! people and organizations and for CurseForge authors. Answers are cached for
//! a few minutes, so going back and forth between pages costs nothing.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use futures_util::future::{join, join3};
use serde::Serialize;
use serde_json::Value;

use crate::{curseforge as cf, modrinth, Core, CoreError};

const V3: &str = "https://api.modrinth.com/v3";
const TTL: Duration = Duration::from_secs(600);
/// CurseForge classes the launcher can install (mods, packs, resource packs, shaders, datapacks).
const CF_CLASSES: [u32; 5] = [6, 12, 4471, 6552, 6945];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Creator {
    /// `user` | `org` (Modrinth) or `cf` (a CurseForge author).
    pub kind: &'static str,
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// The member's role in the project or organization; empty when unknown.
    pub role: String,
    /// The creator's page on its catalog's site.
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Work {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    /// `mod` | `modpack` | `resourcepack` | `shader` | `datapack`.
    pub kind: String,
    pub downloads: u64,
    pub follows: u64,
    pub categories: Vec<String>,
    pub loaders: Vec<String>,
    /// The newest release game version, empty when unknown.
    pub mc: String,
    pub updated: String,
    pub published: String,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub creator: Creator,
    pub bio: String,
    pub created: String,
    /// An organization's members; empty for people.
    pub members: Vec<Creator>,
    /// The organizations a person belongs to.
    pub orgs: Vec<Creator>,
    /// Most downloaded first.
    pub projects: Vec<Work>,
    pub downloads: u64,
    pub follows: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Credits {
    pub org: Option<Creator>,
    /// Owner first, then in the team's own order.
    pub members: Vec<Creator>,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_owned()
}
fn n(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}
fn list(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}
fn https(u: Option<&str>) -> Option<String> {
    u.filter(|u| u.starts_with("https://") && u.len() < 2048 && !u.contains(char::is_whitespace))
        .map(str::to_owned)
}

/// The newest release (`1.21.1`, `26.1`) among game versions; snapshots are skipped.
pub fn newest_release(vs: &[String]) -> String {
    vs.iter()
        .filter_map(|g| {
            let p: Option<Vec<u32>> = g.split('.').map(|x| x.parse().ok()).collect();
            p.filter(|p| (2..=3).contains(&p.len())).map(|p| (p, g))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|x| x.1.clone())
        .unwrap_or_default()
}

fn kind_of(types: &[String], loaders: &[String]) -> Option<String> {
    if !loaders.is_empty() && loaders.iter().all(|l| l == "datapack") {
        return Some("datapack".into());
    }
    types.iter().find_map(|t| match t.as_str() {
        "modpack" | "resourcepack" | "shader" | "datapack" => Some(t.clone()),
        "mod" | "plugin" => Some("mod".into()),
        _ => None,
    })
}

/// A public Modrinth project, from API v2 (`title`, `description`, `project_type`)
/// or v3 (`name`, `summary`, `project_types`). Drafts and unlisted ones are skipped.
pub fn work_of(v: &Value) -> Option<Work> {
    let status = s(v, "status");
    if !matches!(status.as_str(), "" | "approved" | "archived") {
        return None;
    }
    let id = s(v, "id");
    if id.is_empty() {
        return None;
    }
    let mut types = list(v, "project_types");
    let t = s(v, "project_type");
    if !t.is_empty() {
        types.insert(0, t);
    }
    let loaders = list(v, "loaders");
    let kind = kind_of(&types, &loaders)?;
    let title = Some(s(v, "title")).filter(|t| !t.is_empty()).unwrap_or_else(|| s(v, "name"));
    // v2's `description` is the summary; v3 calls it `summary` and `description` is the body.
    let description = if v.get("summary").is_some() { s(v, "summary") } else { s(v, "description") };
    Some(Work {
        project_id: id,
        slug: s(v, "slug"),
        title,
        description,
        icon_url: https(v.get("icon_url").and_then(Value::as_str)),
        kind,
        downloads: n(v, "downloads"),
        follows: n(v, "followers"),
        categories: list(v, "categories"),
        mc: newest_release(&list(v, "game_versions")),
        loaders,
        updated: s(v, "updated"),
        published: s(v, "published"),
        archived: status == "archived",
    })
}

fn works(v: &Value) -> Vec<Work> {
    let mut out: Vec<Work> = Vec::new();
    for w in v.as_array().into_iter().flatten().filter_map(work_of) {
        if !out.iter().any(|x| x.project_id == w.project_id) {
            out.push(w);
        }
    }
    out.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    out
}

fn user_url(name: &str) -> String {
    format!("https://modrinth.com/user/{}", modrinth::encode(name))
}

/// Accepted team members, the owner first and then in the team's order.
pub fn members_of(v: &Value) -> Vec<Creator> {
    let mut ms: Vec<(bool, i64, Creator)> = Vec::new();
    for m in v.as_array().into_iter().flatten() {
        if m.get("accepted").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let Some(u) = m.get("user") else { continue };
        let name = s(u, "username");
        let id = s(u, "id");
        if name.is_empty() || id.is_empty() || ms.iter().any(|x| x.2.id == id) {
            continue;
        }
        let role = s(m, "role");
        ms.push((
            role != "Owner",
            m.get("ordering").and_then(Value::as_i64).unwrap_or(0),
            Creator {
                kind: "user",
                id,
                url: user_url(&name),
                name,
                avatar: https(u.get("avatar_url").and_then(Value::as_str)),
                role,
            },
        ));
    }
    ms.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    ms.into_iter().map(|x| x.2).collect()
}

fn org_of(v: &Value) -> Option<Creator> {
    let id = s(v, "id");
    if id.is_empty() {
        return None;
    }
    let slug = s(v, "slug");
    let name = Some(s(v, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| slug.clone());
    Some(Creator {
        kind: "org",
        url: format!("https://modrinth.com/organization/{}", modrinth::encode(if slug.is_empty() { &id } else { &slug })),
        name,
        id,
        avatar: https(v.get("icon_url").and_then(Value::as_str)),
        role: String::new(),
    })
}

fn cf_author(a: &cf::CfAuthor) -> Creator {
    Creator {
        kind: "cf",
        id: a.id.to_string(),
        name: a.name.clone(),
        avatar: https(Some(&a.avatar_url)),
        role: String::new(),
        url: if a.url.starts_with("https://") {
            a.url.clone()
        } else {
            format!("https://www.curseforge.com/members/{}", modrinth::encode(&a.name))
        },
    }
}

fn cf_authors(list: &[cf::CfAuthor]) -> Vec<Creator> {
    let mut out: Vec<Creator> = Vec::new();
    for a in list.iter().filter(|a| a.id > 0 && !a.name.is_empty()) {
        if !out.iter().any(|x| x.id == a.id.to_string()) {
            out.push(cf_author(a));
        }
    }
    out
}

/// A CurseForge project the launcher can install; other classes (worlds, addons) are skipped.
pub fn cf_work(m: &cf::CfMod) -> Option<Work> {
    if !CF_CLASSES.contains(&m.class_id) {
        return None;
    }
    let mut loaders: Vec<String> = Vec::new();
    let mut games: Vec<String> = Vec::new();
    for i in &m.latest_files_indexes {
        if let Some(l) = i.mod_loader.and_then(cf::loader_of_type) {
            if !loaders.iter().any(|x| x == l) {
                loaders.push(l.into());
            }
        }
        games.push(i.game_version.clone());
    }
    Some(Work {
        project_id: cf::cid(m.id),
        slug: m.slug.clone(),
        title: m.name.clone(),
        description: m.summary.clone(),
        icon_url: cf::icon(m),
        kind: cf::project_type(m.class_id).into(),
        downloads: m.download_count.max(0.0) as u64,
        follows: m.thumbs_up_count,
        categories: m.categories.iter().map(|c| c.slug.clone()).collect(),
        mc: newest_release(&games),
        loaders,
        updated: m.date_modified.clone(),
        published: m.date_created.clone(),
        archived: false,
    })
}

async fn fetch(core: &Core, url: &str) -> Result<Option<Value>, CoreError> {
    let r = core.http().get(url).send().await?;
    if r.status().as_u16() == 404 {
        return Ok(None);
    }
    Ok(Some(r.error_for_status()?.json().await?))
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}
fn cached(key: &str) -> Option<Value> {
    let c = cache().lock().ok()?;
    c.get(key).filter(|(t, _)| t.elapsed() < TTL).map(|(_, v)| v.clone())
}
fn keep(key: String, v: &Value) {
    if let Ok(mut c) = cache().lock() {
        if c.len() > 200 {
            c.retain(|_, (t, _)| t.elapsed() < TTL);
        }
        c.insert(key, (Instant::now(), v.clone()));
    }
}

/// Who made a project: its organization (Modrinth) and its members or authors.
pub async fn credits(core: &Core, project: &str) -> Result<Value, CoreError> {
    let key = format!("credits:{project}");
    if let Some(v) = cached(&key) {
        return Ok(v);
    }
    let c = if cf::is_cf(project) {
        let id = cf::num(project).ok_or_else(|| CoreError::msg("Неверный проект"))?;
        let m = cf::mods(core, &[id]).await?;
        Credits { org: None, members: m.first().map(|m| cf_authors(&m.authors)).unwrap_or_default() }
    } else {
        let e = modrinth::encode(project);
        let (ms, org) = join(
            fetch(core, &format!("{}/project/{e}/members", modrinth::API)),
            fetch(core, &format!("{V3}/project/{e}/organization")),
        )
        .await;
        let ms = ms?.ok_or_else(|| CoreError::msg("Проект не найден"))?;
        // A project without an organization answers 404; that is not an error.
        let org = org.ok().flatten().filter(|v| !v.is_null()).and_then(|v| org_of(&v));
        Credits { org, members: members_of(&ms) }
    };
    let v = serde_json::to_value(c)?;
    keep(key, &v);
    Ok(v)
}

/// A creator's page: `kind` is `user` or `org` (Modrinth id or slug) or `cf` (author id).
pub async fn profile(core: &Core, kind: &str, id: &str) -> Result<Value, CoreError> {
    let ok = match kind {
        "user" | "org" => modrinth::valid_project_id(id),
        "cf" => id.parse::<u64>().is_ok_and(|n| n > 0),
        _ => false,
    };
    if !ok {
        return Err(CoreError::msg("Неверный автор"));
    }
    let key = format!("person:{kind}:{id}");
    if let Some(v) = cached(&key) {
        return Ok(v);
    }
    let mut p = match kind {
        "user" => user(core, id).await?,
        "org" => org(core, id).await?,
        _ => cf_person(core, id.parse().unwrap_or(0)).await?,
    };
    p.downloads = p.projects.iter().map(|w| w.downloads).sum();
    p.follows = p.projects.iter().map(|w| w.follows).sum();
    let v = serde_json::to_value(p)?;
    keep(key, &v);
    Ok(v)
}

async fn user(core: &Core, id: &str) -> Result<Profile, CoreError> {
    let e = modrinth::encode(id);
    let (u, ps, os) = join3(
        fetch(core, &format!("{}/user/{e}", modrinth::API)),
        fetch(core, &format!("{}/user/{e}/projects", modrinth::API)),
        fetch(core, &format!("{V3}/user/{e}/organizations")),
    )
    .await;
    let u = u?.ok_or_else(|| CoreError::msg("Автор не найден на Modrinth"))?;
    let name = s(&u, "username");
    let creator = Creator {
        kind: "user",
        id: s(&u, "id"),
        url: user_url(&name),
        name,
        avatar: https(u.get("avatar_url").and_then(Value::as_str)),
        role: String::new(),
    };
    let orgs = os.ok().flatten().unwrap_or_default();
    Ok(Profile {
        creator,
        bio: s(&u, "bio"),
        created: s(&u, "created"),
        members: Vec::new(),
        orgs: orgs.as_array().into_iter().flatten().filter_map(org_of).collect(),
        projects: works(&ps?.unwrap_or_default()),
        downloads: 0,
        follows: 0,
    })
}

async fn org(core: &Core, id: &str) -> Result<Profile, CoreError> {
    let e = modrinth::encode(id);
    let (o, ps) = join(
        fetch(core, &format!("{V3}/organization/{e}")),
        fetch(core, &format!("{V3}/organization/{e}/projects")),
    )
    .await;
    let o = o?.ok_or_else(|| CoreError::msg("Организация не найдена на Modrinth"))?;
    let creator = org_of(&o).ok_or_else(|| CoreError::msg("Организация не найдена на Modrinth"))?;
    Ok(Profile {
        creator,
        bio: s(&o, "description"),
        created: String::new(),
        members: members_of(o.get("members").unwrap_or(&Value::Null)),
        orgs: Vec::new(),
        projects: works(&ps?.unwrap_or_default()),
        downloads: 0,
        follows: 0,
    })
}

async fn cf_person(core: &Core, id: u64) -> Result<Profile, CoreError> {
    let mods = cf::by_author(core, id).await?;
    let me = mods
        .iter()
        .flat_map(|m| m.authors.iter())
        .find(|a| a.id == id)
        .map(cf_author)
        .ok_or_else(|| CoreError::msg("У автора нет проектов на CurseForge"))?;
    let mut projects: Vec<Work> = Vec::new();
    for w in mods.iter().filter_map(cf_work) {
        if !projects.iter().any(|x| x.project_id == w.project_id) {
            projects.push(w);
        }
    }
    projects.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    Ok(Profile {
        creator: me,
        bio: String::new(),
        created: String::new(),
        members: Vec::new(),
        orgs: Vec::new(),
        projects,
        downloads: 0,
        follows: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn newest_release_skips_snapshots() {
        let v: Vec<String> = ["1.20.1", "1.21", "24w14a", "1.21.1", "26.1", "1.8.9"].map(String::from).to_vec();
        assert_eq!(newest_release(&v), "26.1");
        assert_eq!(newest_release(&[]), "");
    }

    #[test]
    fn works_from_v2_and_v3() {
        let v2 = json!({"id":"YL57xq9U","slug":"iris","title":"Iris Shaders","description":"short","project_type":"mod",
            "status":"approved","downloads":10,"followers":3,"loaders":["fabric"],"game_versions":["1.20.1","1.21.1"],
            "icon_url":"https://cdn.modrinth.com/x.webp","categories":["optimization"]});
        let w = work_of(&v2).unwrap();
        assert_eq!((w.title.as_str(), w.description.as_str(), w.kind.as_str(), w.mc.as_str()), ("Iris Shaders", "short", "mod", "1.21.1"));
        let v3 = json!({"id":"AANobbMI","slug":"sodium","name":"Sodium","summary":"fast","description":"# body",
            "project_types":["mod"],"status":"archived","loaders":["fabric"],"icon_url":"http://bad"});
        let w = work_of(&v3).unwrap();
        assert_eq!((w.title.as_str(), w.description.as_str()), ("Sodium", "fast"));
        assert!(w.archived);
        assert_eq!(w.icon_url, None);
        let dp = json!({"id":"a","project_type":"mod","loaders":["datapack"]});
        assert_eq!(work_of(&dp).unwrap().kind, "datapack");
        assert!(work_of(&json!({"id":"b","project_type":"mod","status":"draft"})).is_none());
        assert!(work_of(&json!({"id":"c","project_types":["minecraft_java_server"]})).is_none());
    }

    #[test]
    fn members_owner_first() {
        let v = json!([
            {"role":"Lead Developer","ordering":0,"user":{"id":"1","username":"IMS"}},
            {"role":"Owner","ordering":3,"user":{"id":"2","username":"coderbot","avatar_url":"https://a/b.png"}},
            {"role":"Dev","ordering":1,"accepted":false,"user":{"id":"3","username":"pending"}},
            {"role":"Dev","ordering":0,"user":{"id":"1","username":"IMS"}}
        ]);
        let m = members_of(&v);
        assert_eq!(m.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["coderbot", "IMS"]);
        assert_eq!(m[0].url, "https://modrinth.com/user/coderbot");
        assert_eq!(m[0].avatar.as_deref(), Some("https://a/b.png"));
    }

    #[test]
    fn org_links_by_slug() {
        let o = org_of(&json!({"id":"LjcZDkRW","slug":"caffeinemc","name":"CaffeineMC"})).unwrap();
        assert_eq!((o.kind, o.url.as_str()), ("org", "https://modrinth.com/organization/caffeinemc"));
    }

    #[test]
    fn cf_skips_other_classes() {
        let m = cf::CfMod { id: 7, class_id: 17, ..Default::default() };
        assert!(cf_work(&m).is_none());
        let m = cf::CfMod { id: 238222, class_id: 6, name: "JEI".into(), ..Default::default() };
        let w = cf_work(&m).unwrap();
        assert_eq!((w.project_id.as_str(), w.kind.as_str()), ("cf:238222", "mod"));
        let a = cf::CfAuthor { id: 5, name: "mezz".into(), ..Default::default() };
        assert_eq!(cf_author(&a).url, "https://www.curseforge.com/members/mezz");
    }
}