use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// Folder under `instances/`, derived from the name. Empty = legacy folder named by `id`.
    #[serde(default)]
    pub dir: String,
    pub version: String,
    /// `vanilla` or `fabric`.
    #[serde(default = "vanilla")]
    pub loader: String,
    /// Loader version (empty for vanilla).
    #[serde(default)]
    pub loader_version: String,
    pub created_at: u64,
    /// Unix time of the last successful launch (0 = never played).
    #[serde(default)]
    pub last_played: u64,
    /// Total time the game ran, in seconds.
    #[serde(default)]
    pub playtime_secs: u64,
    #[serde(default)]
    pub settings: InstanceSettings,
    /// The Modrinth modpack it was installed from; enables updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PackRef {
    pub project_id: String,
    pub version_id: String,
    #[serde(default)]
    pub version_number: String,
}

/// Files the pack put in the game folder (relative paths); an update removes the ones it drops.
pub const PACK_FILES: &str = "freedom-pack.json";

/// Per-instance overrides; `None` = launcher default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InstanceSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// Path to `java`/`javaw`; `None` = managed runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<String>,
    /// What the launcher does once the game starts: `keep`, `minimize`, `close`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_launch: Option<String>,
}

pub const AFTER_LAUNCH: &[&str] = &["keep", "minimize", "close"];

impl InstanceSettings {
    pub fn validate(&self) -> Result<(), CoreError> {
        let bad = |m: &str| Err(CoreError::BadArgs(m.into()));
        if self.memory_mb.is_some_and(|m| !(512..=65536).contains(&m)) {
            return bad("memory must be 512-65536 MB");
        }
        let px = |v: Option<u32>| v.is_some_and(|v| !(320..=7680).contains(&v));
        if px(self.width) || px(self.height) || self.width.is_some() != self.height.is_some() {
            return bad("resolution must be both width and height, 320-7680");
        }
        if self
            .after_launch
            .as_deref()
            .is_some_and(|a| !AFTER_LAUNCH.contains(&a))
        {
            return bad("after_launch must be keep, minimize or close");
        }
        Ok(())
    }
    pub fn resolution(&self) -> Option<(u32, u32)> {
        self.width.zip(self.height)
    }
}

fn vanilla() -> String {
    "vanilla".into()
}

/// Loaders an instance can be created with.
pub const LOADERS: &[&str] = &["vanilla", "fabric", "quilt", "forge", "neoforge"];

impl Instance {
    /// Name of this instance's folder under `instances/`.
    pub fn dir_name(&self) -> &str {
        if self.dir.is_empty() {
            &self.id
        } else {
            &self.dir
        }
    }
    /// The game directory (worlds, mods, configs) of this instance.
    pub fn game_dir(&self, data_dir: &std::path::Path) -> std::path::PathBuf {
        data_dir.join("instances").join(self.dir_name())
    }
    /// Id of the version this instance launches (its `versions/<id>/` dir).
    pub fn version_id(&self) -> String {
        match self.loader.as_str() {
            "fabric" => format!("fabric-loader-{}-{}", self.loader_version, self.version),
            "quilt" => format!("quilt-loader-{}-{}", self.loader_version, self.version),
            "forge" => format!("{}-forge-{}", self.version, self.loader_version),
            "neoforge" => format!("neoforge-{}", self.loader_version),
            _ => self.version.clone(),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct InstanceStore {
    pub instances: Vec<Instance>,
}

pub fn validate(name: &str, version: &str) -> Result<(), CoreError> {
    if name.trim().is_empty() || name.chars().count() > 64 {
        return Err(CoreError::BadArgs("name must be 1-64 characters".into()));
    }
    if version.trim().is_empty() {
        return Err(CoreError::BadArgs("choose a game version".into()));
    }
    Ok(())
}

/// A readable, Windows-safe folder name made from an instance name.
pub fn folder_base(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut s = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > 48 {
        s = s.chars().take(48).collect();
    }
    let s = s
        .trim_end_matches(['.', ' '])
        .trim_start_matches(['.', ' '])
        .to_owned();
    if s.is_empty() {
        return "instance".into();
    }
    let stem = s
        .split('.')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        format!("_{s}")
    } else {
        s
    }
}

/// `base`, or `base (2)`, `base (3)`… — the first one `taken` rejects.
pub fn unique_folder(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|c| !taken(c))
        .expect("unbounded")
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_names() {
        use super::{folder_base, unique_folder};
        assert_eq!(folder_base("Моя сборка 1.21"), "Моя сборка 1.21");
        assert_eq!(folder_base("a/b:c*?"), "a b c");
        assert_eq!(folder_base("  ...  "), "instance");
        assert_eq!(folder_base("con"), "_con");
        assert_eq!(folder_base("COM1.txt"), "_COM1.txt");
        assert_eq!(folder_base("end. "), "end");
        assert_eq!(folder_base(&"x".repeat(80)).len(), 48);
        let taken = ["main", "main (2)"];
        assert_eq!(
            unique_folder("Main", |d| taken.contains(&d.to_lowercase().as_str())),
            "Main (3)"
        );
        assert_eq!(unique_folder("Other", |d| taken.contains(&d)), "Other");
    }

    use super::*;

    #[test]
    fn settings_validation() {
        assert!(InstanceSettings::default().validate().is_ok());
        let ok = InstanceSettings {
            memory_mb: Some(4096),
            width: Some(1280),
            height: Some(720),
            ..Default::default()
        };
        assert!(ok.validate().is_ok());
        assert_eq!(ok.resolution(), Some((1280, 720)));
        assert!(InstanceSettings {
            memory_mb: Some(100),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(InstanceSettings {
            width: Some(1280),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(InstanceSettings {
            after_launch: Some("x".into()),
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
