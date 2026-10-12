use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub addons_enabled: bool,
    pub reduce_motion: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "default".into(),
            addons_enabled: true,
            reduce_motion: false,
        }
    }
}
