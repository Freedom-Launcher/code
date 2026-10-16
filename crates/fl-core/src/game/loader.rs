//! Mod loaders on top of vanilla.
//!
//! Fabric's and Quilt's meta APIs serve a ready loader profile: a partial version JSON
//! with `inheritsFrom` that is merged onto the vanilla JSON
//! ([`VersionJson::merge_onto`]). The merged version installs and launches
//! like any other; its client jar and assets come from vanilla.

use std::fs;

use serde_json::Value;

use super::{
    download,
    install::{self, Layout},
    version::VersionJson,
};
use crate::{instances::Instance, Core, CoreError};

const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const QUILT_META: &str = "https://meta.quiltmc.org/v3";

/// Loaders whose profile comes from a Fabric-style meta API.
fn meta_base(loader: &str) -> Option<&'static str> {
    match loader {
        "fabric" => Some(FABRIC_META),
        "quilt" => Some(QUILT_META),
        _ => None,
    }
}

pub(super) async fn get(core: &Core, url: &str) -> Result<Vec<u8>, CoreError> {
    Ok(core
        .http()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}

/// Newest stable Fabric loader for a game version (the newest of any kind
/// when none is stable).
pub async fn fabric_latest(core: &Core, game: &str) -> Result<String, CoreError> {
    let url = format!("{FABRIC_META}/versions/loader/{game}");
    let list: Value = serde_json::from_slice(&get(core, &url).await?)?;
    pick_loader(&list).ok_or_else(|| CoreError::msg(format!("Fabric is not available for {game}")))
}

/// Default loader version for a new instance.
pub async fn latest(core: &Core, loader: &str, game: &str) -> Result<String, CoreError> {
    match loader {
        "fabric" => fabric_latest(core, game).await,
        "quilt" => quilt_latest(core, game).await,
        _ => super::forge::latest(core, loader, game).await,
    }
}

/// Newest stable Quilt loader for a game version. Quilt's meta has no
/// `stable` flag and no fixed order: stable versions are the ones without a
/// pre-release suffix, compared numerically.
pub async fn quilt_latest(core: &Core, game: &str) -> Result<String, CoreError> {
    let url = format!("{QUILT_META}/versions/loader/{game}");
    let missing = || CoreError::msg(format!("Quilt is not available for {game}"));
    let bytes = get(core, &url).await.map_err(|_| missing())?;
    let list: Value = serde_json::from_slice(&bytes)?;
    pick_quilt(&list).ok_or_else(missing)
}

fn pick_quilt(list: &Value) -> Option<String> {
    let key = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let versions: Vec<&str> = list
        .as_array()?
        .iter()
        .filter_map(|e| e["loader"]["version"].as_str())
        .collect();
    let stable = versions.iter().filter(|v| !v.contains('-'));
    stable
        .max_by_key(|v| key(v))
        .or_else(|| versions.iter().max_by_key(|v| key(v)))
        .map(|v| v.to_string())
}

fn pick_loader(list: &Value) -> Option<String> {
    let arr = list.as_array()?;
    arr.iter()
        .find(|e| e["loader"]["stable"] == true)
        .or(arr.first())
        .and_then(|e| e["loader"]["version"].as_str())
        .map(str::to_owned)
}

/// The instance's full version JSON. Downloads what is missing; the loader
/// profile is fetched once and then read from disk.
pub async fn resolve(
    core: &Core,
    layout: &Layout,
    inst: &Instance,
) -> Result<VersionJson, CoreError> {
    let base = install::load_version(core, layout, &inst.version).await?;
    if inst.loader == "vanilla" {
        return Ok(base);
    }
    let path = layout.version_json(&inst.version_id());
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            let b = if let Some(meta) = meta_base(&inst.loader) {
                let url = format!(
                    "{meta}/versions/loader/{}/{}/profile/json",
                    inst.version, inst.loader_version
                );
                get(core, &url).await?
            } else {
                super::forge::version_json(core, layout, inst).await?
            };
            download::write_atomic(&path, &b)?;
            b
        }
    };
    let child: VersionJson = serde_json::from_slice(&bytes)?;
    Ok(child.merge_onto(base))
}

/// Same as [`resolve`] from local files only (no network), `None` when
/// anything is missing.
pub fn resolve_local(layout: &Layout, inst: &Instance) -> Option<VersionJson> {
    let read = |id: &str| -> Option<VersionJson> {
        serde_json::from_slice(&fs::read(layout.version_json(id)).ok()?).ok()
    };
    let base = read(&inst.version)?;
    if inst.loader == "vanilla" {
        return Some(base);
    }
    Some(read(&inst.version_id())?.merge_onto(base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{install::library_plan, version::Env};
    use serde_json::json;

    fn inst(loader: &str, lv: &str) -> Instance {
        Instance {
            id: "i".into(),
            name: "n".into(),
            dir: String::new(),
            version: "1.21.4".into(),
            loader: loader.into(),
            loader_version: lv.into(),
            created_at: 0,
            last_played: 0,
            playtime_secs: 0,
            settings: Default::default(),
            pack: None,
        }
    }

    #[test]
    fn picks_stable_loader() {
        let l = json!([
            { "loader": { "version": "0.17.0-beta.1", "stable": false } },
            { "loader": { "version": "0.16.10", "stable": true } }
        ]);
        assert_eq!(pick_loader(&l).as_deref(), Some("0.16.10"));
        assert_eq!(pick_loader(&json!([])), None);
        assert_eq!(
            inst("fabric", "0.16.10").version_id(),
            "fabric-loader-0.16.10-1.21.4"
        );
        assert_eq!(inst("vanilla", "").version_id(), "1.21.4");
    }

    #[test]
    fn picks_newest_stable_quilt() {
        let l = json!([
            { "loader": { "version": "0.20.0-beta.9" } },
            { "loader": { "version": "0.9.2" } },
            { "loader": { "version": "0.29.2" } },
            { "loader": { "version": "0.30.0-beta.1" } },
            { "loader": { "version": "0.29.10" } }
        ]);
        assert_eq!(pick_quilt(&l).as_deref(), Some("0.29.10"));
        let betas = json!([{ "loader": { "version": "0.1.0-beta.2" } },
                           { "loader": { "version": "0.1.0-beta.10" } }]);
        assert_eq!(pick_quilt(&betas).as_deref(), Some("0.1.0-beta.10"));
        assert_eq!(pick_quilt(&json!([])), None);
        assert_eq!(
            inst("quilt", "0.29.2").version_id(),
            "quilt-loader-0.29.2-1.21.4"
        );
        assert_eq!(meta_base("quilt"), Some(QUILT_META));
        assert_eq!(meta_base("forge"), None);
    }

    #[test]
    fn resolves_fabric_from_disk() {
        let root = std::env::temp_dir().join(format!("fl-loader-{}", std::process::id()));
        let layout = Layout { root: root.clone() };
        let i = inst("fabric", "0.16.10");
        let vanilla = json!({
            "id": "1.21.4", "mainClass": "net.minecraft.client.main.Main", "type": "release",
            "javaVersion": { "component": "java-runtime-delta", "majorVersion": 21 },
            "arguments": { "game": ["--username", "${auth_player_name}"], "jvm": ["-cp", "${classpath}"] },
            "libraries": []
        });
        let fabric = json!({
            "id": i.version_id(), "inheritsFrom": "1.21.4",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": { "game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "] },
            "libraries": [{ "name": "net.fabricmc:fabric-loader:0.16.10",
                            "url": "https://maven.fabricmc.net/", "sha1": "ab", "size": 1 }]
        });
        for (id, v) in [("1.21.4", &vanilla), (i.version_id().as_str(), &fabric)] {
            download::write_atomic(&layout.version_json(id), v.to_string().as_bytes()).unwrap();
        }
        let v = resolve_local(&layout, &i).expect("resolved");
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            v.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(v.jar_id(), "1.21.4");
        assert_eq!(v.java_version.as_ref().map(|j| j.major_version), Some(21));
        let plan = library_plan(&v, &Env::current(), std::path::Path::new("/l"));
        assert_eq!(
            plan.tasks[0].url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar"
        );
    }
}
