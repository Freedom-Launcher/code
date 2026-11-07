//! Java runtime manager.
//!
//! Primary source: Mojang's official `java-runtime` manifest. The component
//! comes from the version JSON (`javaVersion.component`) and every file is
//! SHA1-checked. Fallback: an Eclipse Temurin JRE archive (Windows zip), checked against the
//! SHA-256 and size Adoptium publishes for it before anything is unpacked.
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

/// The Java executable if this runtime is already fully installed (no network).
pub fn installed(core: &Core, java: Option<&JavaVersion>) -> Option<PathBuf> {
    let (component, _) = component_for(java);
    let dir = runtime_dir(core, &component);
    let exe = java_exe(&dir, &Env::current());
    (exe.exists() && dir.join(".version").exists()).then_some(exe)
}

/// Ensures the runtime is installed and returns the Java executable.
pub async fn ensure(
    core: &Core,
    java: Option<&JavaVersion>,
    progress: Arc<Progress>,
) -> Result<PathBuf, CoreError> {
    if let Some(exe) = installed(core, java) {
        return Ok(exe);
    }
    let env = Env::current();
    let (component, major) = component_for(java);
    let dir = runtime_dir(core, &component);
    let exe = java_exe(&dir, &env);
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
    // Ask for the package description first: it carries the SHA-256 and size of the archive,
    // so what we unpack is exactly what Adoptium published (a bare download link gives no such proof).
    let meta_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os=windows&vendor=eclipse&release_type=ga&jvm_impl=hotspot&heap_size=normal"
    );
    let meta: serde_json::Value = core
        .http()
        .get(&meta_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let pkg = pick_package(&meta)?;
    let bytes = core
        .http()
        .get(&pkg.link)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    verify_package(&bytes, &pkg)?;
    let _ = fs::remove_dir_all(dir);
    unzip_strip(&bytes, dir)?;
    fs::write(dir.join(".version"), format!("temurin-{major}"))?;
    Ok(())
}

/// What Adoptium says about the archive: where it is, how big, and its SHA-256.
#[derive(Debug, PartialEq)]
struct Package {
    link: String,
    sha256: String,
    size: Option<u64>,
}

/// First package of an Adoptium `assets/latest` answer. The link must be https and the checksum
/// a real SHA-256, otherwise there is nothing to check the download against, so we refuse.
fn pick_package(v: &serde_json::Value) -> Result<Package, CoreError> {
    let p = &v[0]["binary"]["package"];
    let link = p["link"].as_str().unwrap_or_default();
    let sha = p["checksum"].as_str().unwrap_or_default();
    if !link.starts_with("https://") {
        return Err(CoreError::msg("Adoptium: no https download link for Java"));
    }
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::msg(
            "Adoptium: no SHA-256 for the Java archive, refusing to unpack it",
        ));
    }
    Ok(Package {
        link: link.to_owned(),
        sha256: sha.to_ascii_lowercase(),
        size: p["size"].as_u64(),
    })
}

fn verify_package(bytes: &[u8], p: &Package) -> Result<(), CoreError> {
    use sha2::{Digest, Sha256};
    if p.size.is_some_and(|s| s != bytes.len() as u64) {
        return Err(CoreError::msg(
            "Java archive has the wrong size, download again",
        ));
    }
    if crate::signing::hex(&Sha256::digest(bytes)) != p.sha256 {
        return Err(CoreError::msg(
            "Java archive checksum does not match Adoptium's, refusing to unpack it",
        ));
    }
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

#[cfg(test)]
mod temurin_tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn answer(link: &str, sum: &str) -> serde_json::Value {
        json!([{ "binary": { "package": { "link": link, "checksum": sum, "size": 4 } } }])
    }
    const SUM: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"; // sha256("test")

    #[test]
    fn a_real_package_is_picked() {
        let p = pick_package(&answer("https://github.com/adoptium/x.zip", SUM)).unwrap();
        assert_eq!(p.link, "https://github.com/adoptium/x.zip");
        assert_eq!(p.sha256, SUM);
        assert_eq!(p.size, Some(4));
    }

    #[test]
    fn no_proof_no_download() {
        for bad in [
            answer("http://example.org/x.zip", SUM),    // plain http
            answer("https://example.org/x.zip", ""),    // no checksum
            answer("https://example.org/x.zip", "abc"), // not a SHA-256
            answer("https://example.org/x.zip", &"z".repeat(64)), // not hex
            json!([]),
            json!({"message": "nope"}),
        ] {
            assert!(pick_package(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_archive_must_match_checksum_and_size() {
        let p = pick_package(&answer("https://example.org/x.zip", SUM)).unwrap();
        assert!(verify_package(b"test", &p).is_ok());
        assert!(verify_package(b"tesx", &p)
            .unwrap_err()
            .to_string()
            .contains("checksum"));
        assert!(verify_package(b"test!", &p)
            .unwrap_err()
            .to_string()
            .contains("size"));
        // Uppercase hex from the server is the same sum.
        let up = pick_package(&answer(
            "https://example.org/x.zip",
            &SUM.to_ascii_uppercase(),
        ))
        .unwrap();
        assert!(verify_package(b"test", &up).is_ok());
        assert_eq!(crate::signing::hex(&Sha256::digest(b"test")), SUM);
    }
}
