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
}

fn vanilla() -> String {
    "vanilla".into()
}

/// Loaders an instance can be created with.
pub const LOADERS: &[&str] = &["vanilla", "fabric"];

impl Instance {
    /// Id of the version this instance launches (its `versions/<id>/` dir).
    pub fn version_id(&self) -> String {
        match self.loader.as_str() {
            "fabric" => format!("fabric-loader-{}-{}", self.loader_version, self.version),
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
