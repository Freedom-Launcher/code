//! Minecraft version JSON (`<id>.json`): model, OS rules, and loader merging.
//!
//! Covers both formats: legacy `minecraftArguments` (up to 1.12) and modern
//! `arguments.game` / `arguments.jvm` with rules. Loader profiles (Fabric etc.)
//! are partial version JSONs with `inheritsFrom`; see [`VersionJson::merge_onto`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The machine we launch on, used to evaluate rules.
#[derive(Debug, Clone)]
pub struct Env {
    /// `windows`, `linux` or `osx` (Mojang naming).
    pub os: &'static str,
    /// `x86_64`, `x86` or `arm64`.
    pub arch: &'static str,
    pub os_version: String,
    pub features: BTreeMap<String, bool>,
}

impl Env {
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "osx",
            _ => "linux",
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "aarch64" => "arm64",
            _ => "x86_64",
        };
        Self {
            os,
            arch,
            os_version: String::new(),
            features: BTreeMap::new(),
        }
    }

    /// `${arch}` in native classifiers: `32` or `64`.
    pub fn bits(&self) -> &'static str {
        if self.arch == "x86" {
            "32"
        } else {
            "64"
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    /// Which version's client jar to use (loader profiles reuse vanilla's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub main_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Ruled { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl Argument {
    /// Values of this argument that apply on `env` (placeholders not yet filled).
    pub fn values(&self, env: &Env) -> Vec<&str> {
        match self {
            Argument::Plain(s) => vec![s.as_str()],
            Argument::Ruled { rules, value } if rules_allow(rules, env) => match value {
                OneOrMany::One(s) => vec![s.as_str()],
                OneOrMany::Many(v) => v.iter().map(String::as_str).collect(),
            },
            Argument::Ruled { .. } => vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// Regex in Mojang's data; only used for old Windows 10 workarounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Rule {
    fn matches(&self, env: &Env) -> bool {
        if let Some(os) = &self.os {
            if os.name.as_deref().is_some_and(|n| n != env.os) {
                return false;
            }
            if os.arch.as_deref().is_some_and(|a| a != env.arch) {
                return false;
            }
            if let Some(v) = os.version.as_deref() {
                // Without a regex engine: treat a pattern as a prefix match.
                let prefix = v
                    .trim_start_matches('^')
                    .split(['\\', '.', '$'])
                    .next()
                    .unwrap_or("");
                if !env.os_version.starts_with(prefix) {
                    return false;
                }
            }
        }
        if let Some(features) = &self.features {
            for (k, want) in features {
                if env.features.get(k).copied().unwrap_or(false) != *want {
                    return false;
                }
            }
        }
        true
    }
}

/// Mojang semantics: no rules → allowed; otherwise disallowed unless the last
/// matching rule allows it.
pub fn rules_allow(rules: &[Rule], env: &Env) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        if r.matches(env) {
            allowed = r.action == RuleAction::Allow;
        }
    }
    allowed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<DownloadFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<BTreeMap<String, DownloadFile>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base (loader libraries, e.g. Fabric).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// OS name → classifier (may contain `${arch}`). Legacy natives format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub total_size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<DownloadFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClient {
    /// e.g. `-Dlog4j.configurationFile=${path}`
    pub argument: String,
    pub file: LoggingFile,
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    pub url: String,
}

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// `group:artifact:version[:classifier][@ext]` → relative Maven path.
pub fn maven_path(name: &str) -> Option<String> {
    let (coords, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coords.split(':').collect();
    let (group, artifact, version) = (parts.first()?, parts.get(1)?, parts.get(2)?);
    let classifier = parts.get(3).map(|c| format!("-{c}")).unwrap_or_default();
    Some(format!(
        "{}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}",
        group.replace('.', "/")
    ))
}

impl Library {
    pub fn applies(&self, env: &Env) -> bool {
        rules_allow(&self.rules, env)
    }

    /// Key for de-duplication: `group:artifact[:classifier]` (no version).
    fn key(&self) -> String {
        let coords = self.name.split('@').next().unwrap_or(&self.name);
        let p: Vec<&str> = coords.split(':').collect();
        match p.as_slice() {
            [g, a, _, c, ..] => format!("{g}:{a}:{c}"),
            [g, a, ..] => format!("{g}:{a}"),
            _ => coords.to_owned(),
        }
    }

    /// Modern (1.19+) natives are ordinary libraries with a `natives-*` classifier.
    pub fn is_modern_native(&self) -> bool {
        self.name
            .split(':')
            .nth(3)
            .is_some_and(|c| c.starts_with("natives-"))
    }

    /// The main jar (classpath entry), if any.
    pub fn artifact(&self) -> Option<DownloadFile> {
        if let Some(a) = self.downloads.as_ref().and_then(|d| d.artifact.clone()) {
            let path = a.path.clone().or_else(|| maven_path(&self.name));
            return Some(DownloadFile { path, ..a });
        }
        if self.natives.is_some() && self.url.is_none() {
            return None; // legacy natives-only entry
        }
        let path = maven_path(&self.name)?;
        let base = self.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
        let base = base.trim_end_matches('/');
        Some(DownloadFile {
            url: format!("{base}/{path}"),
            path: Some(path),
            sha1: self.sha1.clone().unwrap_or_default(),
            size: self.size.unwrap_or(0),
        })
    }

    /// Legacy natives jar for `env` (to be extracted, not put on the classpath).
    pub fn native(&self, env: &Env) -> Option<DownloadFile> {
        let classifier = self
            .natives
            .as_ref()?
            .get(env.os)?
            .replace("${arch}", env.bits());
        let file = self
            .downloads
            .as_ref()?
            .classifiers
            .as_ref()?
            .get(&classifier)?
            .clone();
        let path = file
            .path
            .clone()
            .or_else(|| maven_path(&format!("{}:{classifier}", self.name)));
        Some(DownloadFile { path, ..file })
    }
}

impl VersionJson {
    /// Merges a loader profile (`self`, with `inheritsFrom`) onto its parent.
    /// Child libraries come first and replace parent ones with the same key;
    /// child arguments are appended; other fields fall back to the parent.
    pub fn merge_onto(self, parent: VersionJson) -> VersionJson {
        let mut libraries = self.libraries;
        let mut seen: std::collections::HashSet<String> =
            libraries.iter().map(Library::key).collect();
        for lib in parent.libraries {
            if seen.insert(lib.key()) {
                libraries.push(lib);
            }
        }
        let arguments = match (parent.arguments, self.arguments) {
            (Some(mut p), Some(c)) => {
                p.game.extend(c.game);
                p.jvm.extend(c.jvm);
                Some(p)
            }
            (p, c) => c.or(p),
        };
        VersionJson {
            jar: self.jar.or(parent.jar).or(Some(parent.id)),
            id: self.id,
            inherits_from: None,
            kind: if self.kind.is_empty() {
                parent.kind
            } else {
                self.kind
            },
            main_class: if self.main_class.is_empty() {
                parent.main_class
            } else {
                self.main_class
            },
            minecraft_arguments: self.minecraft_arguments.or(parent.minecraft_arguments),
            arguments,
            libraries,
            asset_index: self.asset_index.or(parent.asset_index),
            assets: self.assets.or(parent.assets),
            downloads: self.downloads.or(parent.downloads),
            java_version: self.java_version.or(parent.java_version),
            logging: self.logging.or(parent.logging),
        }
    }

    /// Id of the version whose client jar is launched.
    pub fn jar_id(&self) -> &str {
        self.jar.as_deref().unwrap_or(&self.id)
    }

    /// Asset index id (`legacy`, `pre-1.6`, `17`, ...).
    pub fn assets_id(&self) -> &str {
        self.asset_index
            .as_ref()
            .map(|a| a.id.as_str())
            .or(self.assets.as_deref())
            .unwrap_or("legacy")
    }
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
    fn rules_follow_mojang_semantics() {
        let rules: Vec<Rule> = serde_json::from_value(json!([
            {"action": "allow"},
            {"action": "disallow", "os": {"name": "osx"}}
        ]))
        .unwrap();
        assert!(rules_allow(&rules, &win()));
        let only_osx: Vec<Rule> =
            serde_json::from_value(json!([{"action": "allow", "os": {"name": "osx"}}])).unwrap();
        assert!(!rules_allow(&only_osx, &win()));
        assert!(rules_allow(&[], &win()));
    }

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.5").unwrap(),
            "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(maven_path("a.b:c:1@zip").unwrap(), "a/b/c/1/c-1.zip");
    }

    #[test]
    fn legacy_native_classifier() {
        let lib: Library = serde_json::from_value(json!({
            "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
            "natives": {"windows": "natives-windows-${arch}"},
            "downloads": {"classifiers": {"natives-windows-64": {
                "path": "x/y.jar", "sha1": "ab", "size": 1, "url": "https://e/x/y.jar"}}}
        }))
        .unwrap();
        assert_eq!(lib.native(&win()).unwrap().path.as_deref(), Some("x/y.jar"));
        assert!(lib.artifact().is_none());
    }

    #[test]
    fn modern_arguments_parse_and_filter() {
        let v: VersionJson = serde_json::from_value(json!({
            "id": "1.21", "mainClass": "net.minecraft.client.main.Main",
            "arguments": {
                "game": ["--username", "${auth_player_name}",
                    {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}],
                "jvm": [{"rules": [{"action": "allow", "os": {"name": "windows"}}],
                    "value": ["-XX:HeapDumpPath=x"]}, "-cp", "${classpath}"]
            },
            "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21}
        }))
        .unwrap();
        let a = v.arguments.unwrap();
        let game: Vec<&str> = a.game.iter().flat_map(|x| x.values(&win())).collect();
        assert_eq!(game, ["--username", "${auth_player_name}"]);
        let jvm: Vec<&str> = a.jvm.iter().flat_map(|x| x.values(&win())).collect();
        assert_eq!(jvm, ["-XX:HeapDumpPath=x", "-cp", "${classpath}"]);
    }

    #[test]
    fn loader_profile_merges_onto_vanilla() {
        let vanilla: VersionJson = serde_json::from_value(json!({
            "id": "1.21", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "libraries": [{"name": "org.ow2.asm:asm:9.3"}, {"name": "com.mojang:brigadier:1.0"}],
            "arguments": {"game": ["--version", "${version_name}"], "jvm": []},
            "assetIndex": {"id": "17", "sha1": "a", "url": "https://e/17.json"}
        }))
        .unwrap();
        let fabric: VersionJson = serde_json::from_value(json!({
            "id": "fabric-loader-0.16.5-1.21", "inheritsFrom": "1.21",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries": [{"name": "org.ow2.asm:asm:9.7", "url": "https://maven.fabricmc.net/"}],
            "arguments": {"game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]}
        }))
        .unwrap();
        let m = fabric.merge_onto(vanilla);
        assert_eq!(
            m.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(m.libraries.len(), 2);
        assert_eq!(m.libraries[0].name, "org.ow2.asm:asm:9.7");
        assert_eq!(
            m.libraries[0].artifact().unwrap().url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.7/asm-9.7.jar"
        );
        assert_eq!(m.assets_id(), "17");
        assert_eq!(m.arguments.unwrap().jvm.len(), 1);
    }
}
