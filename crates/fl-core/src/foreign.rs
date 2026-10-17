//! Import from other launchers (Э6): Prism Launcher / MultiMC, CurseForge,
//! ATLauncher, GDLauncher, Modrinth App and the official launcher's `.minecraft`.
//!
//! Each launcher's own instance metadata gives the name, game version and loader;
//! when it is missing (Modrinth App keeps it in a database, a bare folder has none)
//! the game log and the mods themselves are read instead. Importing copies only the
//! game content — mods, packs, worlds, configs, options — never the other
//! launcher's versions, libraries, logs or caches.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use serde_json::Value;

use crate::game::download::Progress;

/// An instance found in another launcher.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Found {
    /// prism | multimc | curseforge | atlauncher | gdlauncher | modrinth | minecraft | folder
    pub source: String,
    pub name: String,
    /// Empty when it could not be determined (the user picks it).
    pub mc: String,
    pub loader: String,
    pub loader_version: String,
    /// The instance folder in the other launcher.
    pub path: String,
    /// Where its game files live (`.minecraft` inside Prism instances).
    pub game_dir: String,
    pub mods: u32,
    pub worlds: u32,
}

/// Content copied from a game folder; everything else stays behind.
pub const CONTENT: &[&str] = &[
    "mods", "config", "defaultconfigs", "kubejs", "scripts", "resourcepacks", "texturepacks", "shaderpacks",
    "saves", "screenshots", "schematics", "global_packs", "options.txt", "optionsof.txt", "optionsshaders.txt",
    "servers.dat",
];

const MAX_CHILDREN: usize = 500;

fn read_json(p: &Path) -> Option<Value> {
    let b = std::fs::read(p).ok()?;
    if b.len() > 4 << 20 {
        return None;
    }
    serde_json::from_slice(&b).ok()
}
fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_owned()
}
fn folder_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
fn loader_of(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("neoforge") {
        "neoforge"
    } else if n.contains("quilt") {
        "quilt"
    } else if n.contains("fabric") {
        "fabric"
    } else if n.contains("forge") {
        "forge"
    } else {
        "vanilla"
    }
}
fn strip_mc(lv: &str, mc: &str) -> String {
    if mc.is_empty() {
        return lv.to_owned();
    }
    let lv = lv.strip_prefix(&format!("{mc}-")).unwrap_or(lv);
    lv.strip_suffix(&format!("-{mc}")).unwrap_or(lv).to_owned()
}
fn token(s: &str) -> String {
    s.chars().take_while(|c| c.is_ascii_alphanumeric() || ".-_+".contains(*c)).collect()
}
fn after(hay: &str, pat: &str) -> Option<String> {
    let i = hay.find(pat)?;
    let t = token(&hay[i + pat.len()..]);
    (!t.is_empty()).then_some(t)
}
fn starts_digit(v: &str) -> bool {
    v.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// A version id of the official launcher → (game version, loader).
pub fn parse_version_id(id: &str) -> (String, Option<(String, String)>) {
    for (pre, l) in [("fabric-loader-", "fabric"), ("quilt-loader-", "quilt")] {
        if let Some((lv, mc)) = id.strip_prefix(pre).and_then(|r| r.split_once('-')) {
            return (mc.into(), Some((l.into(), lv.into())));
        }
    }
    if let Some(lv) = id.strip_prefix("neoforge-") {
        let mut p = lv.split('.');
        let (a, b) = (p.next().unwrap_or(""), p.next().unwrap_or("0"));
        let mc = if b == "0" { format!("1.{a}") } else { format!("1.{a}.{b}") };
        return (mc, Some(("neoforge".into(), lv.into())));
    }
    let low = id.to_ascii_lowercase();
    if let Some(i) = low.find("-forge") {
        let mc = id[..i].to_owned();
        let lv = strip_mc(id[i + 6..].trim_start_matches('-'), &mc);
        return (mc, Some(("forge".into(), lv)));
    }
    if starts_digit(id) {
        return (id.split('-').next().unwrap_or(id).to_owned(), None);
    }
    (String::new(), None)
}

/// The loader most mods in `mods/` are built for (a sample of jars).
fn jar_loader(mods: &Path) -> Option<(String, String)> {
    const FILES: [&str; 4] = ["quilt.mod.json", "META-INF/neoforge.mods.toml", "fabric.mod.json", "META-INF/mods.toml"];
    const NAMES: [&str; 4] = ["quilt", "neoforge", "fabric", "forge"];
    let mut votes = [0u32; 4];
    let rd = std::fs::read_dir(mods).ok()?;
    let jars = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jar")));
    for j in jars.take(12) {
        let Ok(f) = std::fs::File::open(&j) else { continue };
        let Ok(mut z) = zip::ZipArchive::new(f) else { continue };
        if let Some(i) = (0..4).find(|&i| z.by_name(FILES[i]).is_ok()) {
            votes[i] += 1;
        }
    }
    let (i, &n) = votes.iter().enumerate().max_by_key(|(i, &v)| (v, std::cmp::Reverse(*i)))?;
    (n > 0).then(|| (NAMES[i].to_owned(), String::new()))
}

/// Game version and loader from the last game log, then from the mods.
fn guess(game: &Path) -> (String, Option<(String, String)>) {
    let mut raw = Vec::new();
    if let Ok(f) = std::fs::File::open(game.join("logs").join("latest.log")) {
        let _ = f.take(512 * 1024).read_to_end(&mut raw);
    }
    let text = String::from_utf8_lossy(&raw);
    let mc = after(&text, "Loading Minecraft ")
        .or_else(|| after(&text, "--fml.mcVersion, "))
        .filter(|v| starts_digit(v))
        .unwrap_or_default();
    let pats = [
        (" with Fabric Loader ", "fabric"),
        (" with Quilt Loader ", "quilt"),
        ("--fml.neoForgeVersion, ", "neoforge"),
        ("--fml.forgeVersion, ", "forge"),
    ];
    let l = pats.iter().find_map(|(p, l)| after(&text, p).map(|v| (l.to_string(), v)));
    (mc, l.or_else(|| jar_loader(&game.join("mods"))))
}

fn count(dir: &Path, f: impl Fn(&Path) -> bool) -> u32 {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| f(&e.path())).count() as u32).unwrap_or(0)
}

fn found(source: &str, name: String, mc: String, known: Option<(String, String)>, path: &Path, game: &Path) -> Found {
    let (mut mc, mut l) = (mc, known);
    if mc.is_empty() || l.is_none() {
        let (gm, gl) = guess(game);
        if mc.is_empty() {
            mc = gm;
        }
        if l.is_none() {
            l = gl;
        }
    }
    let (mut loader, mut lv) = l.unwrap_or(("vanilla".into(), String::new()));
    if !crate::instances::LOADERS.contains(&loader.as_str()) {
        loader = "vanilla".into();
    }
    if loader == "vanilla" {
        lv.clear();
    }
    let lv = strip_mc(&lv, &mc);
    let name = if name.trim().is_empty() { folder_name(path) } else { name.trim().to_owned() };
    Found {
        source: source.into(),
        name,
        mc,
        loader,
        loader_version: lv,
        path: path.to_string_lossy().into_owned(),
        game_dir: game.to_string_lossy().into_owned(),
        mods: count(&game.join("mods"), |p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jar"))),
        worlds: count(&game.join("saves"), |p| p.is_dir()),
    }
}

// ---------- Launchers ----------

fn prism(dir: &Path) -> Option<Found> {
    let cfg = std::fs::read_to_string(dir.join("instance.cfg")).ok()?;
    let val = |k: &str| cfg.lines().find_map(|l| l.strip_prefix(k)).map(|v| v.trim().trim_matches('"').to_owned());
    let name = val("name=").unwrap_or_default();
    let mut mc = String::new();
    let mut l = None;
    if let Some(c) = read_json(&dir.join("mmc-pack.json")).and_then(|p| p["components"].as_array().cloned()) {
        l = Some(("vanilla".into(), String::new()));
        for x in &c {
            let uid = s(&x["uid"]);
            match uid.as_str() {
                "net.minecraft" => mc = s(&x["version"]),
                "net.fabricmc.fabric-loader" | "org.quiltmc.quilt-loader" | "net.minecraftforge" | "net.neoforged" => {
                    l = Some((loader_of(&uid).into(), s(&x["version"])))
                }
                _ => {}
            }
        }
    }
    if mc.is_empty() {
        mc = val("IntendedVersion=").unwrap_or_default();
    }
    let game = [".minecraft", "minecraft"].iter().map(|d| dir.join(d)).find(|p| p.is_dir()).unwrap_or_else(|| dir.join(".minecraft"));
    let src = if dir.to_string_lossy().to_lowercase().contains("multimc") { "multimc" } else { "prism" };
    Some(found(src, name, mc, l, dir, &game))
}

fn curseforge(dir: &Path) -> Option<Found> {
    let j = read_json(&dir.join("minecraftinstance.json"))?;
    let mc = s(&j["gameVersion"]);
    let bl = s(&j["baseModLoader"]["name"]);
    let l = if bl.is_empty() {
        Some(("vanilla".into(), String::new()))
    } else {
        let (k, rest) = bl.split_once('-').unwrap_or((&bl, ""));
        Some((loader_of(k).into(), strip_mc(rest, &mc).split('-').next().unwrap_or("").to_owned()))
    };
    Some(found("curseforge", s(&j["name"]), mc, l, dir, dir))
}

fn instance_json(dir: &Path) -> Option<Found> {
    let j = read_json(&dir.join("instance.json"))?;
    if let Some(la) = j.get("launcher") {
        // ATLauncher
        let mc = Some(s(&j["id"])).filter(|v| !v.is_empty()).unwrap_or_else(|| s(&la["minecraftVersion"]));
        let lvj = &la["loaderVersion"];
        let l = if lvj.is_object() { (loader_of(&s(&lvj["type"])).into(), s(&lvj["version"])) } else { ("vanilla".into(), String::new()) };
        return Some(found("atlauncher", s(&la["name"]), mc, Some(l), dir, dir));
    }
    // GDLauncher
    let gc = j.get("game_configuration")?;
    let st = &gc["version"]["Standard"];
    let l = st["modloaders"].as_array().and_then(|a| a.first()).map(|m| (loader_of(&s(&m["type_"])).into(), s(&m["version"])));
    let game = if dir.join("instance").is_dir() { dir.join("instance") } else { dir.to_path_buf() };
    Some(found("gdlauncher", s(&j["name"]), s(&st["release"]), l.or(Some(("vanilla".into(), String::new()))), dir, &game))
}

fn gdlauncher_legacy(dir: &Path) -> Option<Found> {
    let j = read_json(&dir.join("config.json"))?;
    let lo = j.get("loader")?;
    let mc = s(&lo["mcVersion"]);
    if mc.is_empty() {
        return None;
    }
    let l = (loader_of(&s(&lo["loaderType"])).into(), s(&lo["loaderVersion"]));
    Some(found("gdlauncher", folder_name(dir), mc, Some(l), dir, dir))
}

fn modrinth(dir: &Path) -> Option<Found> {
    let j = read_json(&dir.join("profile.json"))?;
    let m = &j["metadata"];
    let l = Some(s(&m["loader"])).filter(|v| !v.is_empty()).map(|v| (v, s(&m["loader_version"]["id"])));
    Some(found("modrinth", s(&m["name"]), s(&m["game_version"]), l, dir, dir))
}

/// The official launcher: the last used profile decides the version.
fn vanilla(dir: &Path) -> Option<Found> {
    let lp = read_json(&dir.join("launcher_profiles.json"))?;
    let mut best = (String::new(), String::new());
    for p in lp["profiles"].as_object().into_iter().flat_map(|m| m.values()) {
        let (id, used) = (s(&p["lastVersionId"]), s(&p["lastUsed"]));
        if !id.is_empty() && used >= best.0 {
            best = (used, id);
        }
    }
    let (mc, l) = parse_version_id(&best.1);
    Some(found("minecraft", "Minecraft".into(), mc, l.or(Some(("vanilla".into(), String::new()))), dir, dir))
}

fn looks_game(p: &Path) -> bool {
    ["mods", "saves", "resourcepacks", "options.txt"].iter().any(|e| p.join(e).exists())
}
fn generic(dir: &Path) -> Option<Found> {
    let game = [".minecraft", "minecraft"]
        .iter()
        .map(|d| dir.join(d))
        .find(|p| looks_game(p))
        .or_else(|| looks_game(dir).then(|| dir.to_path_buf()))?;
    Some(found("folder", folder_name(dir), String::new(), None, dir, &game))
}

fn inspect_known(dir: &Path) -> Option<Found> {
    prism(dir)
        .or_else(|| curseforge(dir))
        .or_else(|| instance_json(dir))
        .or_else(|| gdlauncher_legacy(dir))
        .or_else(|| modrinth(dir))
        .or_else(|| vanilla(dir))
}

/// One folder: a known launcher's instance or any game folder.
pub fn inspect(dir: &Path) -> Option<Found> {
    if !dir.is_dir() {
        return None;
    }
    inspect_known(dir).or_else(|| generic(dir))
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !folder_name(p).starts_with(['.', '_']))
        .collect();
    v.sort();
    v.truncate(MAX_CHILDREN);
    v
}

/// A folder the user picked: an instance itself, or a folder of instances.
pub fn scan_folder(p: &Path) -> Vec<Found> {
    if let Some(f) = inspect(p) {
        return vec![f];
    }
    let mut seen = HashSet::new();
    let mut out = vec![];
    for base in [p.to_path_buf(), p.join("instances"), p.join("Instances"), p.join("profiles")] {
        for d in children(&base) {
            if let Some(f) = inspect(&d) {
                if seen.insert(f.path.clone()) {
                    out.push(f);
                }
            }
        }
    }
    out
}

fn prism_instances(base: &Path) -> PathBuf {
    let cfg = std::fs::read_to_string(base.join("prismlauncher.cfg")).unwrap_or_default();
    match cfg.lines().find_map(|l| l.strip_prefix("InstanceDir=")).map(|v| v.trim().trim_matches('"')) {
        Some(v) if !v.is_empty() => {
            let p = PathBuf::from(v);
            if p.is_absolute() { p } else { base.join(p) }
        }
        _ => base.join("instances"),
    }
}

fn roots() -> Vec<(&'static str, PathBuf)> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = env("USERPROFILE").or_else(|| env("HOME"));
    let mut bases: Vec<PathBuf> = env("APPDATA").into_iter().collect();
    if let Some(h) = &home {
        bases.push(env("XDG_DATA_HOME").unwrap_or_else(|| h.join(".local/share")));
        bases.push(h.join("Library/Application Support"));
        bases.push(h.join(".var/app/org.prismlauncher.PrismLauncher/data"));
    }
    let mut out = vec![];
    for b in &bases {
        out.push(("prism", prism_instances(&b.join("PrismLauncher"))));
        out.push(("atlauncher", b.join("ATLauncher/instances")));
        out.push(("atlauncher", b.join("atlauncher/instances")));
        out.push(("gdlauncher", b.join("gdlauncher_carbon/data/instances")));
        out.push(("gdlauncher", b.join("gdlauncher_next/instances")));
        out.push(("modrinth", b.join("ModrinthApp/profiles")));
        out.push(("modrinth", b.join("com.modrinth.theseus/profiles")));
        out.push(("minecraft", b.join(".minecraft")));
        out.push(("minecraft", b.join("minecraft")));
    }
    if let Some(h) = &home {
        out.push(("curseforge", h.join("curseforge/minecraft/Instances")));
        out.push(("multimc", h.join("MultiMC/instances")));
        out.push(("minecraft", h.join(".minecraft")));
    }
    out.retain(|(_, p)| p.is_dir());
    out
}

/// Every instance of every launcher installed for this user.
pub fn scan() -> Vec<Found> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for (src, root) in roots() {
        let list: Vec<Found> = if src == "minecraft" {
            vanilla(&root).into_iter().collect()
        } else {
            children(&root)
                .iter()
                .filter_map(|d| if src == "modrinth" { inspect(d) } else { inspect_known(d) })
                .collect()
        };
        for mut f in list {
            if f.source == "folder" {
                f.source = src.into();
            }
            let key = std::fs::canonicalize(&f.path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| f.path.clone());
            if seen.insert(key) {
                out.push(f);
            }
        }
    }
    out
}

// ---------- Copy ----------

fn measure(p: &Path) -> (u64, u64) {
    let Ok(m) = std::fs::symlink_metadata(p) else { return (0, 0) };
    if m.is_file() {
        return (m.len(), 1);
    }
    if !m.is_dir() {
        return (0, 0);
    }
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| measure(&e.path())).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)))
        .unwrap_or((0, 0))
}

/// Copies the game content of `from` into `to`, reporting bytes and files to `p`.
pub fn copy_content(from: &Path, to: &Path, p: &Progress) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    let items: Vec<&str> = CONTENT.iter().copied().filter(|n| from.join(n).exists()).collect();
    let (bytes, files) = items.iter().map(|n| measure(&from.join(n))).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    p.total_bytes.store(bytes, Ordering::Relaxed);
    p.total_files.store(files, Ordering::Relaxed);
    for n in items {
        let (src, dst) = (from.join(n), to.join(n));
        let t = std::fs::symlink_metadata(&src)?.file_type();
        if t.is_dir() {
            crate::relocate::copy_tree(&src, &dst, p)?;
        } else if t.is_file() {
            let b = std::fs::copy(&src, &dst)?;
            p.done_bytes.fetch_add(b, Ordering::Relaxed);
            p.done_files.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-foreign-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn put(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    #[test]
    fn prism_instance() {
        let d = tmp().join("Pack");
        put(&d.join("instance.cfg"), "[General]\nname=Мой пак\n");
        put(&d.join("mmc-pack.json"), r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.fabricmc.intermediary","version":"1.20.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.15.0"}]}"#);
        put(&d.join(".minecraft/mods/a.jar"), "x");
        std::fs::create_dir_all(d.join(".minecraft/saves/World")).unwrap();
        let f = inspect(&d).unwrap();
        assert_eq!((f.source.as_str(), f.name.as_str(), f.mc.as_str()), ("prism", "Мой пак", "1.20.1"));
        assert_eq!((f.loader.as_str(), f.loader_version.as_str(), f.mods, f.worlds), ("fabric", "0.15.0", 1, 1));
        assert!(f.game_dir.ends_with(".minecraft"));
    }

    #[test]
    fn curseforge_instance() {
        let d = tmp();
        put(&d.join("minecraftinstance.json"), r#"{"name":"All the Mods","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0"}}"#);
        let f = inspect(&d).unwrap();
        assert_eq!((f.source.as_str(), f.loader.as_str(), f.loader_version.as_str()), ("curseforge", "forge", "47.2.0"));
        put(&d.join("minecraftinstance.json"), r#"{"name":"F","gameVersion":"1.20.1","baseModLoader":{"name":"fabric-0.15.0-1.20.1"}}"#);
        assert_eq!(inspect(&d).unwrap().loader_version, "0.15.0");
    }

    #[test]
    fn version_ids() {
        assert_eq!(parse_version_id("fabric-loader-0.15.0-1.20.1"), ("1.20.1".into(), Some(("fabric".into(), "0.15.0".into()))));
        assert_eq!(parse_version_id("1.20.1-forge-47.2.0"), ("1.20.1".into(), Some(("forge".into(), "47.2.0".into()))));
        assert_eq!(parse_version_id("neoforge-21.1.77").0, "1.21.1");
        assert_eq!(parse_version_id("neoforge-21.0.10").0, "1.21");
        assert_eq!(parse_version_id("1.21.4"), ("1.21.4".into(), None));
        assert_eq!(parse_version_id("latest-release"), (String::new(), None));
    }

    #[test]
    fn bare_folder_guesses_from_log() {
        let d = tmp().join("profile");
        put(&d.join("logs/latest.log"), "[main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.5\n");
        put(&d.join("mods/x.jar"), "x");
        let f = inspect(&d).unwrap();
        assert_eq!((f.source.as_str(), f.mc.as_str(), f.loader.as_str(), f.loader_version.as_str()), ("folder", "1.21.1", "fabric", "0.16.5"));
        assert!(scan_folder(&d.parent().unwrap().join("nothing")).is_empty());
    }

    #[test]
    fn copies_only_content() {
        let (a, b) = (tmp(), tmp().join("dst"));
        put(&a.join("mods/m.jar"), "jar");
        put(&a.join("options.txt"), "fov:1");
        put(&a.join("logs/latest.log"), "log");
        put(&a.join("versions/1.20.1/1.20.1.jar"), "big");
        let p = Progress::default();
        copy_content(&a, &b, &p).unwrap();
        assert!(b.join("mods/m.jar").is_file() && b.join("options.txt").is_file());
        assert!(!b.join("logs").exists() && !b.join("versions").exists());
        assert_eq!(p.done_files.load(Ordering::Relaxed), 2);
        assert_eq!(p.total_bytes.load(Ordering::Relaxed), 8);
    }
}