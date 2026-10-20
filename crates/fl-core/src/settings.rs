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
    /// Java location per major version ("8" → path), as in Modrinth's
    /// "Java N location". Used when an instance sets no Java of its own.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub javas: BTreeMap<String, String>,
    /// Base colors picked by the user, per theme base ("dark" / "light"):
    /// token name (without `--`) → `#rrggbb` or `#rrggbbaa`. Laid over the active theme.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, BTreeMap<String, String>>,
}

/// Tokens the color editor may set. Shades (hover, press, soft, on-accent) are derived by the UI.
pub const COLOR_TOKENS: [&str; 13] = [
    "accent", "bg", "surface-1", "surface-2", "surface-3", "text", "text-2", "text-3",
    "line", "line-strong", "danger", "warn", "glass",
];

fn is_hex_color(v: &str) -> bool {
    let h = v.strip_prefix('#').unwrap_or("");
    v.starts_with('#') && (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit())
}

/// Checks a `colors` map from the UI: bases dark/light, known tokens, hex values only.
pub fn check_colors(m: &BTreeMap<String, BTreeMap<String, String>>) -> Result<(), String> {
    for (base, t) in m {
        if base != "dark" && base != "light" {
            return Err(format!("colors: «{base}» — ожидается dark или light"));
        }
        for (k, v) in t {
            if !COLOR_TOKENS.contains(&k.as_str()) {
                return Err(format!("colors.{base}: неизвестный цвет «{k}»"));
            }
            if !is_hex_color(v) {
                return Err(format!("colors.{base}.{k}: «{v}» — ожидается #rrggbb или #rrggbbaa"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod colors_tests {
    use super::*;
    #[test]
    fn colors_checked() {
        let mut t = BTreeMap::new();
        t.insert("accent".to_string(), "#8ab4ff".to_string());
        t.insert("line".to_string(), "#ffffff14".to_string());
        let mut m = BTreeMap::new();
        m.insert("dark".to_string(), t.clone());
        assert!(check_colors(&m).is_ok());
        t.insert("accent".to_string(), "red".to_string());
        m.insert("dark".to_string(), t.clone());
        assert!(check_colors(&m).is_err());
        t.remove("accent");
        t.insert("--x".to_string(), "#000000".to_string());
        m.insert("dark".to_string(), t);
        assert!(check_colors(&m).is_err());
        let mut m = BTreeMap::new();
        m.insert("sepia".to_string(), BTreeMap::new());
        assert!(check_colors(&m).is_err());
        for bad in ["#12345", "#1234567", "#gggggg", "#ffffff;", "#ffffffff00"] {
            assert!(!is_hex_color(bad), "{bad}");
        }
    }
}

/// Checks a `javas` map from the UI: majors 8–40, paths up to 1024 chars, no control chars.
pub fn check_javas(m: &BTreeMap<String, String>) -> Result<(), String> {
    if m.len() > 16 {
        return Err("javas: слишком много значений".into());
    }
    for (k, v) in m {
        let ok_major = k.parse::<u32>().is_ok_and(|n| (8..=40).contains(&n));
        if !ok_major || v.trim().is_empty() || v.len() > 1024 || v.chars().any(char::is_control) {
            return Err(format!("javas: bad entry {k}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod javas_tests {
    use super::*;
    #[test]
    fn javas_checked() {
        let mut m = BTreeMap::new();
        m.insert("21".to_string(), "C:/jdk/bin/java.exe".to_string());
        assert!(check_javas(&m).is_ok());
        m.insert("x".to_string(), "p".to_string());
        assert!(check_javas(&m).is_err());
        let mut m = BTreeMap::new();
        m.insert("17".to_string(), "a\nb".replace("\\n", "\n"));
        m.insert("17".to_string(), "bad\u{7}".to_string());
        assert!(check_javas(&m).is_err());
    }
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
            javas: BTreeMap::new(),
            colors: BTreeMap::new(),
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
    /// Catalog results: `pages` (numbered, as in Modrinth) | `scroll` (endless).
    pub browse: String,
    /// Results per catalog page: 10, 20 or 50.
    pub per_page: u32,
    /// Menus and dropdown lists: density, percent (100 = opaque).
    pub menu_density: u32,
    /// Blur behind menus and dropdown lists, px.
    pub menu_blur: u32,
    /// Instance name on cards: size, percent of the default.
    pub plate_size: u32,
    /// Instance name font: `ui` | `system` | `mono` | `serif`.
    pub plate_font: String,
    /// Instance name weight, 400–800.
    pub plate_weight: u32,
    /// Show the line under the name (loader, version).
    pub plate_meta: bool,
    /// Launcher font: `inter` | `system` | `serif` | `mono` | `custom` (the user's file).
    pub ui_font: String,
    /// Cache-buster of the user's own font file; 0 = none picked yet.
    pub font_stamp: u64,
    /// Display name of the user's font (its file name), up to 64 characters.
    pub font_name: String,
    /// Window layout: `auto` (separate panels when a wallpaper is on) | `on` (always
    /// separate panels) | `off` (everything flush to the window edges).
    #[serde(default = "panels_auto")]
    pub panels: String,
}

fn panels_auto() -> String {
    "auto".into()
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
            browse: "pages".into(),
            per_page: 20,
            panels: panels_auto(),
            menu_density: 92,
            menu_blur: 24,
            plate_size: 100,
            plate_font: "ui".into(),
            plate_weight: 600,
            plate_meta: true,
            ui_font: "inter".into(),
            font_stamp: 0,
            font_name: String::new(),
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
        one_of("browse", &l.browse, &["pages", "scroll"])?;
        one_of("panels", &l.panels, &["auto", "on", "off"])?;
        if ![10, 20, 50].contains(&l.per_page) {
            return Err(format!("per_page: «{}» — ожидается 10, 20 или 50", l.per_page));
        }
        l.wall_blur = l.wall_blur.min(40);
        l.glass_blur = l.glass_blur.min(60);
        l.glass_density = l.glass_density.min(100);
        one_of("plate_font", &l.plate_font, &["ui", "system", "mono", "serif"])?;
        l.menu_density = l.menu_density.min(100);
        l.menu_blur = l.menu_blur.min(60);
        l.plate_size = l.plate_size.clamp(80, 160);
        l.plate_weight = (l.plate_weight.clamp(400, 800) / 100) * 100;
        one_of("ui_font", &l.ui_font, &["inter", "system", "serif", "mono", "custom"])?;
        l.font_name = l.font_name.chars().filter(|c| !c.is_control()).take(64).collect();
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
    /// `Some(false)` shows an element a theme hides.
    pub hide: Option<bool>,
    /// Background, `#rrggbb` or `#rrggbbaa`.
    pub bg: Option<String>,
    /// Opacity, percent (10–100).
    pub opacity: Option<u16>,
    /// Brightness, contrast, saturation: percent (100 = unchanged).
    pub bright: Option<u16>,
    pub contrast: Option<u16>,
    pub saturate: Option<u16>,
    /// Background blur (frosted glass), px.
    pub blur: Option<u16>,
    /// Border width, px, and its colour.
    pub border: Option<u16>,
    pub border_color: Option<String>,
    /// Shadow strength, 0–4.
    pub shadow: Option<u16>,
    /// Box size set by dragging its edges, px.
    pub w: Option<u16>,
    pub h: Option<u16>,
    /// Free offset from its place, px.
    pub x: Option<i16>,
    pub y: Option<i16>,
}

/// `(field, min, max, neutral)` for the per-element numbers above.
pub const UI_EL_NUM: [(&str, u16, u16, u16); 7] = [
    ("opacity", 10, 100, 100),
    ("bright", 30, 200, 100),
    ("contrast", 30, 200, 100),
    ("saturate", 0, 300, 100),
    ("blur", 0, 40, 0),
    ("border", 0, 6, 0),
    ("shadow", 0, 4, 0),
];

pub const UI_RADIUS: [&str; 8] = [
    "btn", "field", "card", "icon", "menu", "chip", "nav", "dialog",
];
/// `(group, min %, max %)`.
pub const UI_SIZE: [(&str, u16, u16); 7] = [
    ("btn", 70, 150),
    ("field", 70, 150),
    ("text", 85, 130),
    ("gap", 60, 160),
    ("rail", 80, 150),
    ("top", 80, 180),
    ("card", 60, 200),
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
            b.w = e.w.or(b.w);
            b.h = e.h.or(b.h);
            b.x = e.x.or(b.x);
            b.y = e.y.or(b.y);
            if e.hide.is_some() {
                b.hide = e.hide;
            }
            b.bg = e.bg.clone().or(b.bg.take());
            b.border_color = e.border_color.clone().or(b.border_color.take());
            for f in [
                (&mut b.opacity, e.opacity),
                (&mut b.bright, e.bright),
                (&mut b.contrast, e.contrast),
                (&mut b.saturate, e.saturate),
                (&mut b.blur, e.blur),
                (&mut b.border, e.border),
                (&mut b.shadow, e.shadow),
            ] {
                *f.0 = f.1.or(*f.0);
            }
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
        // A bad id (a file name with spaces, say) is dropped, never fails the whole save:
        // one such edit used to make every later save fail, so all sizes were lost on restart.
        self.els.retain(|id, _| ui_id(id));
        for (id, e) in &mut self.els {
            if keep_visible(id) {
                e.hide = None;
            }
            e.radius = e.radius.map(|r| r.min(64));
            e.scale = e.scale.map(|s| s.clamp(50, 200));
            e.w = e.w.map(|v| v.clamp(24, 4000));
            e.h = e.h.map(|v| v.clamp(24, 4000));
            e.x = e.x.map(|v| v.clamp(-4000, 4000)).filter(|v| *v != 0);
            e.y = e.y.map(|v| v.clamp(-4000, 4000)).filter(|v| *v != 0);
            for c in [&mut e.bg, &mut e.border_color] {
                if let Some(v) = c.as_mut() {
                    if !is_hex_color(v) {
                        return Err(format!("ui.els.{id}: цвет «{v}» — ожидается #rrggbb"));
                    }
                    *v = v.to_ascii_lowercase();
                }
            }
            for (f, (_, lo, hi, neutral)) in [
                &mut e.opacity,
                &mut e.bright,
                &mut e.contrast,
                &mut e.saturate,
                &mut e.blur,
                &mut e.border,
                &mut e.shadow,
            ]
            .into_iter()
            .zip(UI_EL_NUM)
            {
                *f = f.map(|x| x.clamp(lo, hi)).filter(|x| *x != neutral);
            }
        }
        self.els.retain(|_, e| *e != UiEl::default());
        self.order.retain(|id, o| {
            if !ui_id(id) || o.len() > 64 {
                return false;
            }
            let mut seen = vec![false; o.len()];
            o.iter().all(|&i| {
                let i = usize::from(i);
                i < o.len() && !std::mem::replace(&mut seen[i], true)
            })
        });
        Ok(self)
    }
}

#[cfg(test)]
mod ui_more_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn card_size_and_unhide() {
        let u: UiCustom = serde_json::from_value(json!({ "size": { "card": 500 }, "els": { "home.recent": { "hide": false } } })).unwrap();
        let u = u.checked().unwrap();
        assert_eq!(u.size["card"], 200);
        let theme: UiCustom = serde_json::from_value(json!({ "els": { "home.recent": { "hide": true } } })).unwrap();
        assert_eq!(theme.layered(&u).els["home.recent"].hide, Some(false));
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
        let ok = |v: Value| serde_json::from_value::<UiCustom>(v).unwrap().checked().unwrap();
        let u = ok(json!({
            "size": { "btn": 120 },
            "els": { "a\"]{": { "radius": 4 }, "content.mods.Big Mod+1.jar": { "radius": 4 }, "home.recent": { "radius": 4 } },
            "order": { "rail": [0, 0], "nav": [1, 0] }
        }));
        assert_eq!(u.size["btn"], 120);
        assert_eq!(u.els.keys().collect::<Vec<_>>(), ["home.recent"]);
        assert_eq!(u.order.keys().collect::<Vec<_>>(), ["nav"]);
        let u = ok(json!({ "els": { "home.recent": { "w": 10, "h": 9000, "x": -5000, "y": 0 } } }));
        let e = &u.els["home.recent"];
        assert_eq!((e.w, e.h, e.x, e.y), (Some(24), Some(4000), Some(-4000), None));
        assert!(bad(json!({ "css": "x" })));
        assert!(bad(json!({ "els": { "x": { "bg": "red; x" } } })));
        assert!(bad(json!({ "els": { "x": { "border_color": "url(a)" } } })));
        let e: UiCustom = serde_json::from_value(json!({ "els": {
            "a": { "bg": "#FF0000AA", "opacity": 5, "bright": 999, "blur": 12, "shadow": 9 },
            "b": { "contrast": 100, "border": 0 }
        } }))
        .unwrap();
        let e = e.checked().unwrap();
        let a = &e.els["a"];
        assert_eq!(a.bg.as_deref(), Some("#ff0000aa"));
        assert_eq!((a.opacity, a.bright, a.blur, a.shadow), (Some(10), Some(200), Some(12), Some(4)));
        assert!(!e.els.contains_key("b"), "neutral values are dropped");
        let top: UiCustom = serde_json::from_value(json!({ "els": { "a": { "blur": 4 } } })).unwrap();
        let l = e.layered(&top);
        assert_eq!((l.els["a"].blur, l.els["a"].bg.as_deref()), (Some(4), Some("#ff0000aa")));
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
        assert_eq!(u.els["x"].hide, Some(true));
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
        let m = Look::default()
            .patch(&json!({"menu_density": 300, "menu_blur": 99, "plate_size": 10, "plate_weight": 950}))
            .unwrap();
        assert_eq!((m.menu_density, m.menu_blur, m.plate_size, m.plate_weight), (100, 60, 80, 800));
        assert!(Look::default().patch(&json!({"plate_font": "comic"})).is_err());
    }

    #[test]
    fn patch_font() {
        let name = format!("My\nFont{}", "x".repeat(90));
        let l = Look::default()
            .patch(&json!({"ui_font": "custom", "font_stamp": 7, "font_name": name}))
            .unwrap();
        assert_eq!((l.ui_font.as_str(), l.font_stamp), ("custom", 7));
        assert!(l.font_name.starts_with("MyFont") && l.font_name.chars().count() == 64);
        assert!(Look::default().patch(&json!({"ui_font": "comic"})).is_err());
        assert_eq!(Look::default().ui_font, "inter");
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
