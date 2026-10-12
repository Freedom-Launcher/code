//! Java runtime manager.
//!
//! Primary source: Mojang's official `java-runtime` manifest. The component
//! comes from the version JSON (`javaVersion.component`) and every file is
//! SHA1-checked. Fallback: an Eclipse Temurin JRE archive (Windows zip).
//!
//! Layout: `<data_dir>/java/<component>/` with a `.version` marker.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Deserialize;

use super::{
    download::{self, Progress, Task},
    version::{Env, JavaVersion},
};
use crate::{Core, CoreError};

const RUNTIMES_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// Mojang platform key for the runtime manifest.
pub fn platform_key(env: &Env) -> &'static str {
    match (env.os, env.arch) {
        ("windows", "x86") => "windows-x86",
        ("windows", "arm64") => "windows-arm64",
        ("windows", _) => "windows-x64",
        ("osx", "arm64") => "mac-os-arm64",
        ("osx", _) => "mac-os",
        (_, "x86") => "linux-i386",
        _ => "linux",
    }
}

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: Remote,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Remote {
    #[serde(default)]
    sha1: String,
    #[serde(default)]
    size: u64,
    url: String,
}

#[derive(Debug, Deserialize)]
struct FileManifest {
    files: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FileEntry {
    File {
        #[serde(default)]
        executable: bool,
        downloads: FileDownloads,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct FileDownloads {
    raw: Remote,
}

pub fn runtime_dir(core: &Core, component: &str) -> PathBuf {
    core.data_dir.join("java").join(component)
}

/// Path to the Java executable inside a runtime directory.
pub fn java_exe(dir: &Path, env: &Env) -> PathBuf {
    match env.os {
        "windows" => dir.join("bin").join("javaw.exe"),
        "osx" => dir.join("jre.bundle/Contents/Home/bin/java"),
        _ => dir.join("bin").join("java"),
    }
}

/// Component to use when a (very old) version JSON names none.
pub fn component_for(java: Option<&JavaVersion>) -> (String, u32) {
    match java {
        Some(j) => (j.component.clone(), j.major_version),
        None => ("jre-legacy".into(), 8),
    }
}

/// Ensures the runtime is installed and returns the Java executable.
pub async fn ensure(
    core: &Core,
    java: Option<&JavaVersion>,
    progress: Arc<Progress>,
) -> Result<PathBuf, CoreError> {
    let env = Env::current();
    let (component, major) = component_for(java);
    let dir = runtime_dir(core, &component);
    let exe = java_exe(&dir, &env);
    if exe.exists() && dir.join(".version").exists() {
        return Ok(exe);
    }
    match from_mojang(core, &env, &component, &dir, Arc::clone(&progress)).await {
        Ok(()) => {}
        Err(mojang_err) => {
            if let Err(e) = from_temurin(core, &env, major, &dir).await {
                return Err(CoreError::msg(format!(
                    "could not install Java {major}: {mojang_err}; fallback: {e}"
                )));
            }
        }
    }
    if !exe.exists() {
        return Err(CoreError::msg(format!(
            "Java installed but {} is missing",
            exe.display()
        )));
    }
    Ok(exe)
}

async fn from_mojang(
    core: &Core,
    env: &Env,
    component: &str,
    dir: &Path,
    progress: Arc<Progress>,
) -> Result<(), CoreError> {
    let http = core.http();
    let all: BTreeMap<String, BTreeMap<String, Vec<RuntimeEntry>>> = http
        .get(RUNTIMES_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let entry = all
        .get(platform_key(env))
        .and_then(|p| p.get(component))
        .and_then(|v| v.first())
        .ok_or_else(|| CoreError::msg(format!("no {component} for {}", platform_key(env))))?;

    let bytes = http
        .get(&entry.manifest.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !entry.manifest.sha1.is_empty()
        && !download::sha1_hex(&bytes).eq_ignore_ascii_case(&entry.manifest.sha1)
    {
        return Err(CoreError::msg("runtime manifest checksum mismatch"));
    }
    let files: FileManifest = serde_json::from_slice(&bytes)?;

    let (tasks, executables, links) = plan(&files, dir);
    download::fetch_all(http, tasks, progress).await?;
    finish(dir, &executables, &links)?;
    fs::write(dir.join(".version"), &entry.version.name)?;
    Ok(())
}

type Plan = (Vec<Task>, Vec<PathBuf>, Vec<(PathBuf, String)>);

fn plan(files: &FileManifest, dir: &Path) -> Plan {
    let mut tasks = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (name, entry) in &files.files {
        let path = dir.join(name);
        if !path.starts_with(dir) || name.contains("..") {
            continue;
        }
        match entry {
            FileEntry::File {
                executable,
                downloads,
            } => {
                if *executable {
                    executables.push(path.clone());
                }
                tasks.push(Task {
                    url: downloads.raw.url.clone(),
                    path,
                    sha1: downloads.raw.sha1.clone(),
                    size: downloads.raw.size,
                });
            }
            FileEntry::Directory => {}
            FileEntry::Link { target } => links.push((path, target.clone())),
        }
    }
    (tasks, executables, links)
}

#[allow(unused_variables)]
fn finish(
    dir: &Path,
    executables: &[PathBuf],
    links: &[(PathBuf, String)],
) -> Result<(), CoreError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in executables {
            fs::set_permissions(p, fs::Permissions::from_mode(0o755))?;
        }
        for (p, target) in links {
            if let Some(d) = p.parent() {
                fs::create_dir_all(d)?;
            }
            let _ = fs::remove_file(p);
            std::os::unix::fs::symlink(target, p)?;
        }
    }
    Ok(())
}

/// Fallback: Temurin JRE zip (Windows only for now), top folder stripped.
async fn from_temurin(core: &Core, env: &Env, major: u32, dir: &Path) -> Result<(), CoreError> {
    if env.os != "windows" {
        return Err(CoreError::msg("Temurin fallback is Windows-only for now"));
    }
    let arch = match env.arch {
        "x86" => "x32",
        "arm64" => "aarch64",
        _ => "x64",
    };
    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{major}/ga/windows/{arch}/jre/hotspot/normal/eclipse"
    );
    let bytes = core
        .http()
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let _ = fs::remove_dir_all(dir);
    unzip_strip(&bytes, dir)?;
    fs::write(dir.join(".version"), format!("temurin-{major}"))?;
    Ok(())
}

/// Extracts a zip, dropping its single top-level folder.
pub fn unzip_strip(bytes: &[u8], dir: &Path) -> Result<(), CoreError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| CoreError::msg(format!("bad Java archive: {e}")))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| CoreError::msg(format!("bad Java archive: {e}")))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let rel: PathBuf = rel.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dir.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(d) = out.parent() {
            fs::create_dir_all(d)?;
        }
        std::io::copy(&mut entry, &mut fs::File::create(&out)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env(os: &'static str, arch: &'static str) -> Env {
        Env {
            os,
            arch,
            os_version: String::new(),
            features: BTreeMap::new(),
        }
    }

    #[test]
    fn platform_keys() {
        assert_eq!(platform_key(&env("windows", "x86_64")), "windows-x64");
        assert_eq!(platform_key(&env("osx", "arm64")), "mac-os-arm64");
        assert_eq!(platform_key(&env("linux", "x86_64")), "linux");
    }

    #[test]
    fn exe_paths() {
        let d = Path::new("/j");
        assert_eq!(
            java_exe(d, &env("windows", "x86_64")),
            PathBuf::from("/j/bin/javaw.exe")
        );
        assert_eq!(
            java_exe(d, &env("linux", "x86_64")),
            PathBuf::from("/j/bin/java")
        );
    }

    #[test]
    fn manifest_plan() {
        let m: FileManifest = serde_json::from_value(json!({"files": {
            "bin": {"type": "directory"},
            "bin/javaw.exe": {"type": "file", "executable": true,
                "downloads": {"raw": {"sha1": "aa", "size": 5, "url": "https://e/javaw"}}},
            "lib/x": {"type": "link", "target": "../y"},
            "../evil": {"type": "file", "downloads": {"raw": {"url": "https://e/evil"}}}
        }}))
        .unwrap();
        let (tasks, exes, links) = plan(&m, Path::new("/j"));
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].path, PathBuf::from("/j/bin/javaw.exe"));
        assert_eq!(exes.len(), 1);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn legacy_versions_get_java_8() {
        assert_eq!(component_for(None), ("jre-legacy".into(), 8));
    }
}
