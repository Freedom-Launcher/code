//! Forge and NeoForge. Both ship an installer jar with `version.json` (a
//! child version merged onto vanilla, like Fabric) and `install_profile.json`
//! (extra libraries plus processors that patch the vanilla client jar).

use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Deserialize;
use serde_json::Value;

use super::{
    download::{self, Progress, Task},
    install::Layout,
    java,
    loader::get,
    version::{maven_path, Library, VersionJson},
};
use crate::{instances::Instance, Core, CoreError};

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/";
const FORGE_PROMOS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEO_MAVEN: &str = "https://maven.neoforged.net/releases/";

fn err(e: impl std::fmt::Display) -> CoreError {
    CoreError::msg(e.to_string())
}

/// Recommended (else latest) Forge, or the newest stable NeoForge.
pub async fn latest(core: &Core, loader: &str, mc: &str) -> Result<String, CoreError> {
    let found = if loader == "forge" {
        let p: Value = serde_json::from_slice(&get(core, FORGE_PROMOS).await?)?;
        ["recommended", "latest"]
            .iter()
            .find_map(|k| p["promos"][format!("{mc}-{k}")].as_str().map(str::to_owned))
    } else {
        let url = format!("{NEO_MAVEN}net/neoforged/neoforge/maven-metadata.xml");
        pick_neoforge(&String::from_utf8_lossy(&get(core, &url).await?), mc)
    };
    found.ok_or_else(|| err(format!("{loader} is not available for {mc}")))
}

/// NeoForge versions follow the game: MC 1.21.4 → 21.4.x, MC 1.21 → 21.0.x.
fn pick_neoforge(xml: &str, mc: &str) -> Option<String> {
    let mut p = mc.split('.').skip(1);
    let prefix = format!("{}.{}.", p.next()?, p.next().unwrap_or("0"));
    let all: Vec<&str> = xml
        .split("<version>")
        .skip(1)
        .filter_map(|s| s.split('<').next())
        .filter(|v| v.starts_with(&prefix))
        .collect();
    all.iter().rev().find(|v| !v.contains('-')).or(all.last()).map(|v| v.to_string())
}

fn installer(layout: &Layout, inst: &Instance) -> (String, PathBuf) {
    let (base, coords) = if inst.loader == "forge" {
        let v = format!("{}-{}", inst.version, inst.loader_version);
        (FORGE_MAVEN, format!("net.minecraftforge:forge:{v}:installer"))
    } else {
        (NEO_MAVEN, format!("net.neoforged:neoforge:{}:installer", inst.loader_version))
    };
    let rel = maven_path(&coords).unwrap_or_default();
    (format!("{base}{rel}"), layout.libraries().join(rel))
}

async fn installer_zip(
    core: &Core,
    layout: &Layout,
    inst: &Instance,
) -> Result<zip::ZipArchive<Cursor<Vec<u8>>>, CoreError> {
    let (url, path) = installer(layout, inst);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            let b = get(core, &url).await?;
            download::write_atomic(&path, &b)?;
            b
        }
    };
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(err)
}

fn entry<R: Read + Seek>(z: &mut zip::ZipArchive<R>, name: &str) -> Result<Vec<u8>, CoreError> {
    let mut f = z.by_name(name.trim_start_matches('/')).map_err(err)?;
    let mut b = Vec::new();
    f.read_to_end(&mut b)?;
    Ok(b)
}

/// The loader's `version.json` (child of vanilla) from the installer.
pub async fn version_json(core: &Core, layout: &Layout, inst: &Instance) -> Result<Vec<u8>, CoreError> {
    entry(&mut installer_zip(core, layout, inst).await?, "version.json")
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    data: HashMap<String, Sides>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
}
#[derive(Deserialize)]
struct Sides {
    #[serde(default)]
    client: String,
}
#[derive(Deserialize)]
struct Processor {
    sides: Option<Vec<String>>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
}

/// `[maven:coords]` → library path, `'x'` → literal, `{KEY}` → variable.
fn resolve_arg(a: &str, vars: &HashMap<String, String>, libs: &Path) -> Result<String, CoreError> {
    if let Some(c) = a.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let rel = maven_path(c).ok_or_else(|| err(format!("bad coordinates: {c}")))?;
        return Ok(libs.join(rel).to_string_lossy().into_owned());
    }
    if let Some(l) = a.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(l.into());
    }
    let mut out = a.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    Ok(out)
}

/// After the merged version is installed: fetches the profile libraries and
/// runs the client processors (they produce the patched client jar).
pub async fn post_install(
    core: &Core,
    layout: &Layout,
    inst: &Instance,
    v: &VersionJson,
    progress: Arc<Progress>,
) -> Result<(), CoreError> {
    let mut z = installer_zip(core, layout, inst).await?;
    let profile: Profile = serde_json::from_slice(&entry(&mut z, "install_profile.json")?)?;
    let libs = layout.libraries();
    let mut tasks = Vec::new();
    let all = profile.libraries.iter().map(|l| (l, true)).chain(v.libraries.iter().map(|l| (l, false)));
    for (lib, fetch) in all {
        let Some(a) = lib.artifact() else { continue };
        let Some(rel) = a.path.as_deref() else { continue };
        let path = libs.join(rel);
        if !a.url.is_empty() {
            if fetch {
                tasks.push(Task { url: a.url.clone(), path, sha1: a.sha1.clone(), size: a.size });
            }
        } else if !path.exists() {
            // Shipped inside the installer (or produced by a processor).
            if let Ok(b) = entry(&mut z, &format!("maven/{rel}")) {
                download::write_atomic(&path, &b)?;
            }
        }
    }
    download::fetch_all(core.http(), tasks, Arc::clone(&progress)).await?;

    let tmp = layout.root.join("tmp").join(inst.version_id());
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let mut vars: HashMap<String, String> = [
        ("SIDE", "client".to_string()),
        ("MINECRAFT_JAR", s(&layout.client_jar(&inst.version))),
        ("MINECRAFT_VERSION", inst.version.clone()),
        ("ROOT", s(&layout.root)),
        ("INSTALLER", s(&installer(layout, inst).1)),
        ("LIBRARY_DIR", s(&libs)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    for (k, d) in &profile.data {
        let val = if d.client.starts_with('/') {
            let dest = tmp.join(d.client.trim_start_matches('/'));
            download::write_atomic(&dest, &entry(&mut z, &d.client)?)?;
            s(&dest)
        } else {
            resolve_arg(&d.client, &vars, &libs)?
        };
        vars.insert(k.clone(), val);
    }

    let java = java::ensure(core, v.java_version.as_ref(), progress).await?;
    let client = |p: &&Processor| p.sides.as_ref().map_or(true, |s| s.iter().any(|x| x == "client"));
    for pr in profile.processors.iter().filter(client) {
        let mut outputs = Vec::new();
        for (f, sha) in &pr.outputs {
            outputs.push((resolve_arg(f, &vars, &libs)?, resolve_arg(sha, &vars, &libs)?));
        }
        let done = |o: &[(String, String)]| o.iter().all(|(f, h)| download::is_valid(Path::new(f), h, 0));
        if !outputs.is_empty() && done(&outputs) {
            continue;
        }
        let mut cp = Vec::new();
        for c in std::iter::once(&pr.jar).chain(&pr.classpath) {
            cp.push(libs.join(maven_path(c).ok_or_else(|| err(format!("bad coordinates: {c}")))?));
        }
        let args: Vec<String> =
            pr.args.iter().map(|a| resolve_arg(a, &vars, &libs)).collect::<Result<_, _>>()?;
        run_java(&java, &cp, &main_class(&cp[0])?, &args)?;
        if !done(&outputs) {
            return Err(err(format!("{} produced a file with a wrong checksum", pr.jar)));
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    Ok(())
}

fn main_class(jar: &Path) -> Result<String, CoreError> {
    let mut z = zip::ZipArchive::new(fs::File::open(jar)?).map_err(err)?;
    let mf = String::from_utf8_lossy(&entry(&mut z, "META-INF/MANIFEST.MF")?).into_owned();
    mf.lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|m| m.trim().to_string())
        .ok_or_else(|| err(format!("no Main-Class in {}", jar.display())))
}

fn run_java(java: &Path, cp: &[PathBuf], main: &str, args: &[String]) -> Result<(), CoreError> {
    let mut cmd = std::process::Command::new(java);
    cmd.arg("-cp").arg(std::env::join_paths(cp).map_err(err)?).arg(main).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output()?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);
    let tail: Vec<&str> = text.lines().rev().take(4).collect();
    Err(err(format!("{main} failed: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | "))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_version_pick_and_args() {
        let xml = "<versions><version>21.3.5</version><version>21.4.150-beta</version>\
                   <version>21.4.158</version><version>21.4.161</version></versions>";
        assert_eq!(pick_neoforge(xml, "1.21.4").as_deref(), Some("21.4.161"));
        assert_eq!(pick_neoforge(xml, "1.20.1"), None);
        let vars = HashMap::from([("SIDE".to_string(), "client".to_string())]);
        let l = Path::new("/l");
        assert_eq!(resolve_arg("{SIDE}", &vars, l).unwrap(), "client");
        assert_eq!(resolve_arg("'abc'", &vars, l).unwrap(), "abc");
        assert_eq!(
            resolve_arg("[net.minecraft:client:1.21.4:mappings@tsrg]", &vars, l).unwrap(),
            "/l/net/minecraft/client/1.21.4/client-1.21.4-mappings.tsrg"
        );
    }
}