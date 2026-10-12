use std::{fs, time::Duration};

use serde_json::{json, Value};

use crate::{Core, CoreError};

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const CACHE_TTL: Duration = Duration::from_secs(3600);

/// Version list. Served from cache when fresh; falls back to a stale cache offline.
pub async fn list(core: &Core, snapshots: bool) -> Result<Value, CoreError> {
    let manifest = manifest(core).await?;
    let versions: Vec<Value> = manifest["versions"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|v| v["type"] == "release" || (snapshots && v["type"] == "snapshot"))
        .map(|v| json!({ "id": v["id"], "type": v["type"], "release_time": v["releaseTime"] }))
        .collect();

    Ok(json!({ "latest": manifest["latest"], "versions": versions }))
}

/// `(url, sha1)` of a version's JSON from the manifest.
pub async fn locate(core: &Core, id: &str) -> Result<(String, String), CoreError> {
    let manifest = manifest(core).await?;
    manifest["versions"]
        .as_array()
        .and_then(|a| a.iter().find(|v| v["id"] == id))
        .and_then(|v| {
            Some((
                v["url"].as_str()?.to_owned(),
                v["sha1"].as_str().unwrap_or("").to_owned(),
            ))
        })
        .ok_or_else(|| CoreError::msg(format!("unknown game version: {id}")))
}

/// The raw version manifest, cached for an hour, stale cache used offline.
pub async fn manifest(core: &Core) -> Result<Value, CoreError> {
    let cache = core.data_dir.join("cache").join("version_manifest_v2.json");
    let fresh = fs::metadata(&cache)
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().unwrap_or_default() < CACHE_TTL)
        .unwrap_or(false);

    let manifest: Value = if fresh {
        serde_json::from_slice(&fs::read(&cache)?)?
    } else {
        match fetch(core).await {
            Ok(bytes) => {
                let v = serde_json::from_slice(&bytes)?;
                fs::create_dir_all(cache.parent().expect("cache dir"))?;
                fs::write(&cache, &bytes)?;
                v
            }
            Err(e) if cache.exists() => {
                let _ = e;
                serde_json::from_slice(&fs::read(&cache)?)?
            }
            Err(e) => return Err(e),
        }
    };
    Ok(manifest)
}

async fn fetch(core: &Core) -> Result<Vec<u8>, CoreError> {
    Ok(core
        .http()
        .get(MANIFEST_URL)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec())
}
