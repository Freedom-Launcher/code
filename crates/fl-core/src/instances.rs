use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
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
}

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
        if self.after_launch.as_deref().is_some_and(|a| !AFTER_LAUNCH.contains(&a)) {
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
pub const LOADERS: &[&str] = &["vanilla", "fabric", "forge", "neoforge"];

impl Instance {
    /// Id of the version this instance launches (its `versions/<id>/` dir).
    pub fn version_id(&self) -> String {
        match self.loader.as_str() {
            "fabric" => format!("fabric-loader-{}-{}", self.loader_version, self.version),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_validation() {
        assert!(InstanceSettings::default().validate().is_ok());
        let ok = InstanceSettings { memory_mb: Some(4096), width: Some(1280), height: Some(720), ..Default::default() };
        assert!(ok.validate().is_ok());
        assert_eq!(ok.resolution(), Some((1280, 720)));
        assert!(InstanceSettings { memory_mb: Some(100), ..Default::default() }.validate().is_err());
        assert!(InstanceSettings { width: Some(1280), ..Default::default() }.validate().is_err());
        assert!(InstanceSettings { after_launch: Some("x".into()), ..Default::default() }.validate().is_err());
    }
}
