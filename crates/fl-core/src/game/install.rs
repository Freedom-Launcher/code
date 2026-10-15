//! Installing a version: version JSON, client jar, libraries, natives, asset
//! index and objects (including legacy "virtual" assets) and the log config.
//!
//! Layout under `<data_dir>/game/`:
//! `versions/<id>/<id>.json|.jar`, `versions/<id>/natives/`, `libraries/`,
//! `assets/{indexes,objects,virtual,log_configs}/`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Deserialize;

use super::{
    download::{self, Progress, Task},
    natives,
    version::{Env, VersionJson},
};
use crate::{versions, Core, CoreError};

const RESOURCES: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(core: &Core) -> Self {
        Self {
            root: core.data_dir.join("game"),
        }
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.root.join("versions").join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn client_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    /// Written after a full, verified install; lets launch skip re-hashing.
    pub fn installed_marker(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(".installed")
    }
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn log_config(&self, file_id: &str) -> PathBuf {
        self.assets().join("log_configs").join(file_id)
    }
}

#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
    /// Pre-1.6: assets are copied into `<game dir>/resources`.
    #[serde(default)]
    pub map_to_resources: bool,
    /// 1.6–1.7: assets are copied into `assets/virtual/<index id>`.
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    #[serde(default)]
    pub size: u64,
}

impl AssetObject {
    fn rel(&self) -> Option<String> {
        let prefix = self.hash.get(..2)?;
        Some(format!("{prefix}/{}", self.hash))
    }
}

/// What a version needs from `libraries/`.
#[derive(Debug, Default)]
pub struct LibPlan {
    pub tasks: Vec<Task>,
    pub classpath: Vec<PathBuf>,
    /// Natives jars to extract, with their exclude prefixes.
    pub natives: Vec<(PathBuf, Vec<String>)>,
}

pub fn library_plan(v: &VersionJson, env: &Env, libs: &Path) -> LibPlan {
    let mut plan = LibPlan::default();
    for lib in v.libraries.iter().filter(|l| l.applies(env)) {
        if let Some(a) = lib.artifact() {
            if let Some(rel) = a.path.as_deref() {
                let path = libs.join(rel);
                if !a.url.is_empty() {
                    plan.tasks.push(Task {
                        url: a.url.clone(),
                        path: path.clone(),
                        sha1: a.sha1.clone(),
                        size: a.size,
                    });
                }
                // Modern natives jars go on the classpath (LWJGL 3 can load
                // from there) and are also extracted, like the official launcher.
                if lib.is_modern_native() {
                    plan.natives.push((path.clone(), Vec::new()));
                }
                plan.classpath.push(path);
            }
        }
        if let Some(n) = lib.native(env) {
            if let Some(rel) = n.path.as_deref() {
                let path = libs.join(rel);
                plan.tasks.push(Task {
                    url: n.url.clone(),
                    path: path.clone(),
                    sha1: n.sha1.clone(),
                    size: n.size,
                });
                let exclude = lib
                    .extract
                    .as_ref()
                    .map(|e| e.exclude.clone())
                    .unwrap_or_default();
                plan.natives.push((path, exclude));
            }
        }
    }
    plan
}

pub fn asset_tasks(index: &AssetIndex, assets: &Path) -> Vec<Task> {
    let mut seen = std::collections::HashSet::new();
    index
        .objects
        .values()
        .filter(|o| seen.insert(o.hash.clone()))
        .filter_map(|o| {
            let rel = o.rel()?;
            Some(Task {
                url: format!("{RESOURCES}/{rel}"),
                path: assets.join("objects").join(&rel),
                sha1: o.hash.clone(),
                size: o.size,
            })
        })
        .collect()
}

/// Copies objects under their real names (legacy and virtual assets).
pub fn copy_named(index: &AssetIndex, assets: &Path, target: &Path) -> Result<(), CoreError> {
    for (name, o) in &index.objects {
        let Some(rel) = o.rel() else { continue };
        let out = target.join(name);
        if !out.starts_with(target) || download::is_valid(&out, &o.hash, o.size) {
            continue;
        }
        if let Some(dir) = out.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::copy(assets.join("objects").join(rel), &out)?;
    }
    Ok(())
}

pub fn read_asset_index(layout: &Layout, v: &VersionJson) -> Result<AssetIndex, CoreError> {
    let path = layout
        .assets()
        .join("indexes")
        .join(format!("{}.json", v.assets_id()));
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// Version JSON for `id`, downloaded (and SHA1-checked) when missing or stale.
/// Works offline once installed.
pub async fn load_version(
    core: &Core,
    layout: &Layout,
    id: &str,
) -> Result<VersionJson, CoreError> {
    let path = layout.version_json(id);
    let bytes = match versions::locate(core, id).await {
        Ok((url, sha1)) if !download::is_valid(&path, &sha1, 0) => {
            let b = core
                .http()
                .get(&url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            if !sha1.is_empty() && !download::sha1_hex(&b).eq_ignore_ascii_case(&sha1) {
                return Err(CoreError::msg(format!("checksum mismatch: {url}")));
            }
            download::write_atomic(&path, &b)?;
            b.to_vec()
        }
        Ok(_) => fs::read(&path)?,
        Err(_) if path.exists() => fs::read(&path)?,
        Err(e) => return Err(e),
    };
    Ok(serde_json::from_slice(&bytes)?)
}

/// Installs everything a vanilla version needs. Safe to re-run: valid files
/// are skipped.
/// `v` is already resolved (vanilla, or a loader merged onto vanilla, see
/// `loader::resolve`).
pub async fn install(
    core: &Core,
    v: VersionJson,
    progress: Arc<Progress>,
) -> Result<VersionJson, CoreError> {
    let layout = Layout::new(core);
    let env = Env::current();
    let http = core.http();

    // Step 1: small metadata files (asset index, log config) and the client jar.
    let mut first = Vec::new();
    if let Some(c) = v.downloads.as_ref().and_then(|d| d.client.as_ref()) {
        first.push(Task {
            url: c.url.clone(),
            path: layout.client_jar(v.jar_id()),
            sha1: c.sha1.clone(),
            size: c.size,
        });
    }
    if let Some(a) = &v.asset_index {
        first.push(Task {
            url: a.url.clone(),
            path: layout
                .assets()
                .join("indexes")
                .join(format!("{}.json", a.id)),
            sha1: a.sha1.clone(),
            size: a.size,
        });
    }
    if let Some(l) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        first.push(Task {
            url: l.file.url.clone(),
            path: layout.log_config(&l.file.id),
            sha1: l.file.sha1.clone(),
            size: l.file.size,
        });
    }
    download::fetch_all(http, first, Arc::clone(&progress)).await?;

    // Step 2: libraries and asset objects together.
    let libs = library_plan(&v, &env, &layout.libraries());
    let index = if v.asset_index.is_some() {
        Some(read_asset_index(&layout, &v)?)
    } else {
        None
    };
    let mut tasks = libs.tasks;
    if let Some(index) = &index {
        tasks.extend(asset_tasks(index, &layout.assets()));
    }
    download::fetch_all(http, tasks, progress).await?;

    // Step 3: natives, freshly extracted each install.
    let natives_dir = layout.natives_dir(&v.id);
    let _ = fs::remove_dir_all(&natives_dir);
    fs::create_dir_all(&natives_dir)?;
    for (jar, exclude) in &libs.natives {
        natives::extract(jar, &natives_dir, exclude)?;
    }

    // Step 4: 1.6–1.7 virtual assets.
    if let Some(index) = index.as_ref().filter(|i| i.is_virtual) {
        let target = layout.assets().join("virtual").join(v.assets_id());
        copy_named(index, &layout.assets(), &target)?;
    }
    fs::write(layout.installed_marker(&v.id), env!("CARGO_PKG_VERSION"))?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn win() -> Env {
        Env {
            os: "windows",
            arch: "x86_64",
            os_version: "10.0".into(),
            features: BTreeMap::new(),
        }
    }

    #[test]
    fn library_plan_splits_classpath_and_natives() {
        let v: VersionJson = serde_json::from_value(json!({
            "id": "1.8.9",
            "libraries": [
                {"name": "com.mojang:authlib:1.5.21", "downloads": {"artifact": {
                    "path": "com/mojang/authlib/1.5.21/authlib-1.5.21.jar",
                    "sha1": "aa", "size": 1, "url": "https://libraries.minecraft.net/x.jar"}}},
                {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
                 "natives": {"windows": "natives-windows-${arch}"},
                 "extract": {"exclude": ["META-INF/"]},
                 "downloads": {"classifiers": {"natives-windows-64": {
                    "path": "n/w64.jar", "sha1": "bb", "size": 2, "url": "https://e/n.jar"}}}},
                {"name": "ca.weblite:java-objc-bridge:1.0.0",
                 "rules": [{"action": "allow", "os": {"name": "osx"}}]}
            ]
        }))
        .unwrap();
        let p = library_plan(&v, &win(), Path::new("/L"));
        assert_eq!(p.tasks.len(), 2);
        assert_eq!(
            p.classpath,
            [PathBuf::from(
                "/L/com/mojang/authlib/1.5.21/authlib-1.5.21.jar"
            )]
        );
        assert_eq!(p.natives.len(), 1);
        assert_eq!(p.natives[0].0, PathBuf::from("/L/n/w64.jar"));
    }

    #[test]
    fn asset_urls_and_dedup() {
        let index: AssetIndex = serde_json::from_value(json!({"objects": {
            "a.png": {"hash": "abcdef0123", "size": 3},
            "b.png": {"hash": "abcdef0123", "size": 3}
        }}))
        .unwrap();
        let t = asset_tasks(&index, Path::new("/A"));
        assert_eq!(t.len(), 1);
        assert_eq!(
            t[0].url,
            "https://resources.download.minecraft.net/ab/abcdef0123"
        );
        assert_eq!(t[0].path, PathBuf::from("/A/objects/ab/abcdef0123"));
    }
}
