use serde::{Deserialize, Serialize};

use crate::CoreError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version: String,
    pub created_at: u64,
    /// Unix time of the last successful launch (0 = never played).
    #[serde(default)]
    pub last_played: u64,
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
