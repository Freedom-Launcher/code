use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// Folder under `instances/`, derived from the name. Empty = legacy folder named by `id`.
    #[serde(default)]
    pub dir: String,
    /// A folder picked by the user that the instance uses in place (no copy under
    /// `instances/`). Empty = the instance lives in `instances/<dir>`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
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
    /// Library group (Э6); empty = ungrouped.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fullscreen: Option<bool>,
    /// Extra environment variables for the game process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Hooks::is_empty")]
    pub hooks: Hooks,
}

/// Commands around a launch (Modrinth «Хуки запуска»).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hooks {
    /// Runs before the game; a non-zero exit cancels the launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_launch: Option<String>,
    /// Program the game is started through, e.g. `prime-run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapper: Option<String>,
    /// Runs after the game exits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_exit: Option<String>,
}

impl Hooks {
    pub fn is_empty(&self) -> bool {
        self.pre_launch.is_none() && self.wrapper.is_none() && self.post_exit.is_none()
    }
    fn over(&self, base: &Hooks) -> Hooks {
        Hooks {
            pre_launch: self.pre_launch.clone().or_else(|| base.pre_launch.clone()),
            wrapper: self.wrapper.clone().or_else(|| base.wrapper.clone()),
            post_exit: self.post_exit.clone().or_else(|| base.post_exit.clone()),
        }
    }
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
        let line = |v: &Option<String>| {
            v.as_deref()
                .is_some_and(|c| c.len() > 1024 || c.contains(['\n', '\r', '\0']))
        };
        let h = &self.hooks;
        if line(&h.pre_launch) || line(&h.wrapper) || line(&h.post_exit) || line(&self.jvm_args) {
            return bad("commands: one line, up to 1024 characters");
        }
        if let Some(env) = &self.env {
            let key_ok = |k: &str| {
                !k.is_empty()
                    && k.len() <= 128
                    && !k.starts_with(|c: char| c.is_ascii_digit())
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            };
            if env.len() > 64
                || env
                    .iter()
                    .any(|(k, v)| !key_ok(k) || v.len() > 4096 || v.contains('\0'))
            {
                return bad("env: up to 64 variables, names A-Z 0-9 _");
            }
        }
        Ok(())
    }
    /// These settings with every unset field taken from `base` (launcher defaults).
    pub fn over(&self, base: &InstanceSettings) -> InstanceSettings {
        let (w, h) = match self.resolution() {
            Some((w, h)) => (Some(w), Some(h)),
            None => (base.width, base.height),
        };
        InstanceSettings {
            memory_mb: self.memory_mb.or(base.memory_mb),
            java_path: self.java_path.clone().or_else(|| base.java_path.clone()),
            width: w,
            height: h,
            jvm_args: self.jvm_args.clone().or_else(|| base.jvm_args.clone()),
            after_launch: self
                .after_launch
                .clone()
                .or_else(|| base.after_launch.clone()),
            fullscreen: self.fullscreen.or(base.fullscreen),
            env: self.env.clone().or_else(|| base.env.clone()),
            hooks: self.hooks.over(&base.hooks),
        }
    }
    /// Fields that run programs; only the built-in UI may change them.
    pub fn keep_exec_from(&mut self, old: &InstanceSettings) {
        self.java_path = old.java_path.clone();
        self.jvm_args = old.jvm_args.clone();
        self.env = old.env.clone();
        self.hooks = old.hooks.clone();
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
    /// `root` is [`crate::Core::instances_root`].
    pub fn game_dir(&self, root: &std::path::Path) -> std::path::PathBuf {
        if self.is_linked() {
            return std::path::PathBuf::from(&self.path);
        }
        root.join(self.dir_name())
    }
    /// Uses a folder outside `instances/`: never renamed, moved or deleted by the launcher.
    pub fn is_linked(&self) -> bool {
        !self.path.is_empty()
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

    #[test]
    fn settings_layering_and_limits() {
        let base = InstanceSettings {
            memory_mb: Some(6144),
            width: Some(1920),
            height: Some(1080),
            hooks: Hooks {
                wrapper: Some("prime-run".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let own = InstanceSettings {
            memory_mb: Some(2048),
            hooks: Hooks {
                post_exit: Some("echo bye".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let m = own.over(&base);
        assert_eq!(m.memory_mb, Some(2048));
        assert_eq!(m.resolution(), Some((1920, 1080)));
        assert_eq!(m.hooks.wrapper.as_deref(), Some("prime-run"));
        assert_eq!(m.hooks.post_exit.as_deref(), Some("echo bye"));
        let bad_env = InstanceSettings {
            env: Some([("1X".to_string(), "v".to_string())].into()),
            ..Default::default()
        };
        assert!(bad_env.validate().is_err());
        let bad_hook = InstanceSettings {
            hooks: Hooks {
                pre_launch: Some("a\nb".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(bad_hook.validate().is_err());
        let json = serde_json::to_value(InstanceSettings::default()).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }
}
