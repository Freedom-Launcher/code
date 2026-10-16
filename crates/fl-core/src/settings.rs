use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub addons_enabled: bool,
    pub reduce_motion: bool,
    /// The first-run guide was finished or skipped.
    pub onboarded: bool,
    /// Look of the built-in shell. `None` until the UI saves it once, so the UI
    /// knows to migrate the values it used to keep in localStorage.
    pub look: Option<Look>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "default".into(),
            addons_enabled: true,
            reduce_motion: false,
            onboarded: false,
            look: None,
        }
    }
}

/// Wallpaper, glass, cover and command-palette preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Look {
    /// `cover` | `custom` | `off`.
    pub wall: String,
    /// Cache-buster of the user's own wallpaper; 0 = none picked yet.
    pub wall_stamp: u64,
    /// Wallpaper blur, px.
    pub wall_blur: u32,
    /// Glass blur, px.
    pub glass_blur: u32,
    /// Glass density, percent: 0 = fully transparent, 100 = opaque.
    /// The UI shows it inverted, as «Прозрачность».
    pub glass_density: u32,
    /// Instance cover: `big` | `slim` | `none`.
    pub cover: String,
    /// Command palette position: `top` | `bottom`.
    pub palette_pos: String,
    pub palette_blur: bool,
    /// Screens fill the whole window instead of a centered column.
    pub full_width: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            wall: "cover".into(),
            wall_stamp: 0,
            wall_blur: 22,
            glass_blur: 28,
            glass_density: 45,
            cover: "big".into(),
            palette_pos: "top".into(),
            palette_blur: true,
            full_width: false,
        }
    }
}

fn one_of(field: &str, v: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&v) {
        Ok(())
    } else {
        Err(format!(
            "{field}: «{v}» — ожидается одно из: {}",
            allowed.join(", ")
        ))
    }
}

impl Look {
    /// Overlays a partial JSON object onto these values. Unknown keys and
    /// unknown variants are rejected; numbers are clamped to the slider ranges.
    pub fn patch(&self, patch: &Value) -> Result<Look, String> {
        let Value::Object(p) = patch else {
            return Err("look: ожидается объект".into());
        };
        let mut cur = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if let Value::Object(m) = &mut cur {
            for (k, v) in p {
                m.insert(k.clone(), v.clone());
            }
        }
        let mut l: Look = serde_json::from_value(cur).map_err(|e| e.to_string())?;
        one_of("wall", &l.wall, &["cover", "custom", "off"])?;
        one_of("cover", &l.cover, &["big", "slim", "none"])?;
        one_of("palette_pos", &l.palette_pos, &["top", "bottom"])?;
        l.wall_blur = l.wall_blur.min(40);
        l.glass_blur = l.glass_blur.min(60);
        l.glass_density = l.glass_density.min(100);
        Ok(l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn full_width_defaults_off_and_patches() {
        let l = Look::default();
        assert!(!l.full_width);
        let p = l.patch(&json!({"full_width": true})).unwrap();
        assert!(p.full_width);
        assert!(l.patch(&json!({"full_width": "yes"})).is_err());
    }

    #[test]
    fn old_settings_file_still_loads() {
        let s: Settings = serde_json::from_str(r#"{"theme":"x","addons_enabled":false}"#).unwrap();
        assert_eq!(s.theme, "x");
        assert!(s.look.is_none());
    }

    #[test]
    fn patch_overlays_only_given_keys() {
        let l = Look::default()
            .patch(&json!({"palette_pos": "bottom", "wall_blur": 10}))
            .unwrap();
        assert_eq!(l.palette_pos, "bottom");
        assert_eq!(l.wall_blur, 10);
        assert_eq!(l.glass_density, 45);
        assert!(l.palette_blur);
    }

    #[test]
    fn patch_clamps_ranges() {
        let l = Look::default()
            .patch(&json!({"wall_blur": 900, "glass_blur": 61, "glass_density": 300}))
            .unwrap();
        assert_eq!((l.wall_blur, l.glass_blur, l.glass_density), (40, 60, 100));
    }

    #[test]
    fn patch_rejects_unknown_keys_and_variants() {
        assert!(Look::default().patch(&json!({"glow": 1})).is_err());
        assert!(Look::default().patch(&json!({"wall": "video"})).is_err());
        assert!(Look::default()
            .patch(&json!({"palette_pos": "left"}))
            .is_err());
        assert!(Look::default().patch(&json!({"wall_blur": -1})).is_err());
        assert!(Look::default().patch(&json!("bottom")).is_err());
    }
}
