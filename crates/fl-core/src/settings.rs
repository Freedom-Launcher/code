use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
    /// Loader order for picking a version when several fit (first wins).
    pub loader_priority: Vec<String>,
    /// Interface editor overrides (corners, sizes, placement).
    pub ui: UiCustom,
    /// Launcher-wide game defaults; each instance overrides what it sets.
    pub game: crate::instances::InstanceSettings,
    /// Folder that holds every instance; `None` = `<data_dir>/instances`.
    /// Changed only by `instances.move_root`, which moves the folders too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instances_dir: Option<String>,
}

pub const LOADER_ORDER: [&str; 4] = ["neoforge", "fabric", "quilt", "forge"];

/// The saved order made safe: known loaders only, no repeats, missing ones appended.
pub fn loader_order(saved: &[String]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for l in saved.iter().map(String::as_str).chain(LOADER_ORDER) {
        if let Some(n) = crate::modrinth::loader_name(l) {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "default".into(),
            addons_enabled: true,
            reduce_motion: false,
            onboarded: false,
            look: None,
            loader_priority: LOADER_ORDER.iter().map(|s| s.to_string()).collect(),
            ui: UiCustom::default(),
            game: Default::default(),
            instances_dir: None,
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

/// Interface editor. Only numbers and known words, so nothing here can inject
/// CSS: the UI turns it into CSS variables and `[data-id]` rules itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiCustom {
    /// Corner scale per group, percent (100 = as the theme draws it).
    pub radius: BTreeMap<String, u16>,
    /// Size scale per group, percent.
    pub size: BTreeMap<String, u16>,
    /// Side of movable panels: `left` | `right`.
    pub layout: BTreeMap<String, String>,
    /// Per-element overrides by `data-id`.
    pub els: BTreeMap<String, UiEl>,
    /// Child order of a container (by `data-id`): a permutation of child indices.
    pub order: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiEl {
    /// Corner radius, px.
    pub radius: Option<u16>,
    /// Size, percent.
    pub scale: Option<u16>,
    pub hide: bool,
}

pub const UI_RADIUS: [&str; 8] = [
    "btn", "field", "card", "icon", "menu", "chip", "nav", "dialog",
];
/// `(group, min %, max %)`.
pub const UI_SIZE: [(&str, u16, u16); 6] = [
    ("btn", 70, 150),
    ("field", 70, 150),
    ("text", 85, 130),
    ("gap", 60, 160),
    ("rail", 80, 150),
    ("top", 80, 180),
];
pub const UI_LAYOUT: [&str; 3] = ["rail", "filters", "project"];

fn ui_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
}

/// Elements that always stay visible, so the settings can be reached again.
fn keep_visible(id: &str) -> bool {
    matches!(id, "app" | "main" | "rail" | "rail.settings" | "safe")
        || id.starts_with("settings")
        || id.starts_with("view.settings")
        || id.starts_with("palette")
}

impl UiCustom {
    /// `over` on top of `self` (a theme's edits): corner and size percents multiply,
    /// so 100 % always means "as the theme draws it"; the rest is replaced.
    pub fn layered(&self, over: &UiCustom) -> UiCustom {
        let mul = |b: Option<&u16>, v: u16| {
            ((u32::from(*b.unwrap_or(&100)) * u32::from(v) + 50) / 100).min(1000) as u16
        };
        let mut u = self.clone();
        for (k, v) in &over.radius {
            let x = mul(u.radius.get(k), *v);
            u.radius.insert(k.clone(), x);
        }
        for (k, v) in &over.size {
            let x = mul(u.size.get(k), *v);
            u.size.insert(k.clone(), x);
        }
        u.layout.extend(over.layout.clone());
        for (id, e) in &over.els {
            let b = u.els.entry(id.clone()).or_default();
            b.radius = e.radius.or(b.radius);
            b.scale = e.scale.or(b.scale);
            b.hide |= e.hide;
        }
        u.order.extend(over.order.clone());
        u.radius.retain(|_, v| *v != 100);
        u.size.retain(|_, v| *v != 100);
        u
    }

    /// Rejects unknown names, clamps numbers, drops no-op entries.
    pub fn checked(mut self) -> Result<Self, String> {
        for (k, v) in &mut self.radius {
            if !UI_RADIUS.contains(&k.as_str()) {
                return Err(format!("ui.radius: неизвестная группа «{k}»"));
            }
            *v = (*v).min(300);
        }
        for (k, v) in &mut self.size {
            let Some(&(_, lo, hi)) = UI_SIZE.iter().find(|g| g.0 == k) else {
                return Err(format!("ui.size: неизвестная группа «{k}»"));
            };
            *v = (*v).clamp(lo, hi);
        }
        for (k, v) in &self.layout {
            if !UI_LAYOUT.contains(&k.as_str()) {
                return Err(format!("ui.layout: неизвестная панель «{k}»"));
            }
            one_of(&format!("ui.layout.{k}"), v, &["left", "right"])?;
        }
        if self.els.len() > 400 || self.order.len() > 100 {
            return Err("ui: слишком много правок".into());
        }
        for (id, e) in &mut self.els {
            if !ui_id(id) {
                return Err(format!("ui.els: неверный элемент «{id}»"));
            }
            if keep_visible(id) {
                e.hide = false;
            }
            e.radius = e.radius.map(|r| r.min(64));
            e.scale = e.scale.map(|s| s.clamp(50, 200));
        }
        self.els.retain(|_, e| *e != UiEl::default());
        for (id, o) in &self.order {
            if !ui_id(id) || o.len() > 64 {
                return Err(format!("ui.order: неверный контейнер «{id}»"));
            }
            let mut seen = vec![false; o.len()];
            for &i in o {
                let i = usize::from(i);
                if i >= o.len() || seen[i] {
                    return Err("ui.order: нужна перестановка".into());
                }
                seen[i] = true;
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui_custom_is_checked() {
        let u: UiCustom = serde_json::from_value(json!({
            "radius": { "btn": 900 }, "size": { "btn": 10 },
            "layout": { "rail": "right" },
            "els": { "settings.game": { "hide": true }, "instance.play": { "scale": 999 } },
            "order": { "rail": [2, 0, 1] }
        }))
        .unwrap();
        let u = u.checked().unwrap();
        assert_eq!(u.radius["btn"], 300);
        assert_eq!(u.size["btn"], 70);
        assert!(!u.els.contains_key("settings.game"));
        assert_eq!(u.els["instance.play"].scale, Some(200));
        let bad =
            |v: Value| serde_json::from_value::<UiCustom>(v).map_or(true, |u| u.checked().is_err());
        assert!(bad(json!({ "radius": { "x": 1 } })));
        assert!(bad(json!({ "layout": { "rail": "top" } })));
        assert!(bad(json!({ "order": { "rail": [0, 0] } })));
        assert!(bad(json!({ "els": { "a\"]{": {} } })));
        assert!(bad(json!({ "css": "x" })));
    }

    use super::*;
    use serde_json::json;

    #[test]
    fn ui_layers_multiply_percents() {
        let base: UiCustom = serde_json::from_value(
            json!({ "radius": { "btn": 200 }, "layout": { "rail": "right" } }),
        )
        .unwrap();
        let over: UiCustom = serde_json::from_value(
            json!({ "radius": { "btn": 50, "card": 150 }, "els": { "x": { "hide": true } } }),
        )
        .unwrap();
        let u = base.layered(&over).checked().unwrap();
        assert!(!u.radius.contains_key("btn"));
        assert_eq!(u.radius["card"], 150);
        assert_eq!(u.layout["rail"], "right");
        assert!(u.els["x"].hide);
    }

    #[test]
    fn loader_order_is_normalized() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            loader_order(&s(&["forge", "bogus", "forge"])),
            ["forge", "neoforge", "fabric", "quilt"]
        );
        assert_eq!(loader_order(&[]), LOADER_ORDER);
        assert_eq!(Settings::default().loader_priority, s(&LOADER_ORDER));
    }

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
