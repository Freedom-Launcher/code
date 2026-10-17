//! Themes: tokens + CSS + optional screen templates. No code, ever.
//!
//! A theme is a folder `<data>/themes/<id>/` with a `theme.json`. Everything is
//! checked on load and again when a file is served, so a theme cannot run script
//! or reach the network (the CSP blocks both as a second line of defence).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::settings::UiCustom;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    #[default]
    Dark,
    Light,
}

/// Built-in themes: `(id, name, base)`. They live in `ui/tokens.css`.
pub const BUILTIN: [(&str, &str, Base); 2] = [
    ("default", "Тёмная", Base::Dark),
    ("light", "Светлая", Base::Light),
];

/// Screens a theme may replace. Settings are never themeable, so recovery always works.
pub const SCREENS: [&str; 5] = [
    "view.instance",
    "view.empty",
    "view.create",
    "view.accounts",
    "view.discover",
];

/// Files a theme may ship next to its CSS.
pub const ASSETS: [&str; 10] = [
    "css", "png", "jpg", "jpeg", "webp", "gif", "svg", "woff2", "woff", "ttf",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub base: Base,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
    #[serde(default)]
    pub css: Vec<String>,
    #[serde(default)]
    pub templates: BTreeMap<String, String>,
    /// Interface editor edits shipped with the theme (numbers and known words only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiCustom>,
}

type R<T> = Result<T, String>;

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("themes")
}

pub fn builtin(id: &str) -> Option<(&'static str, &'static str, Base)> {
    BUILTIN.iter().copied().find(|b| b.0 == id)
}

pub fn valid_id(id: &str) -> bool {
    (1..=40).contains(&id.len())
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Relative path inside the theme folder with one of `exts`. No `..`, no schemes.
pub fn safe_rel(p: &str, exts: &[&str]) -> bool {
    let lower = p.to_ascii_lowercase();
    !p.is_empty()
        && p.len() <= 120
        && p.split('/').all(|s| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
        && exts.iter().any(|e| lower.ends_with(&format!(".{e}")))
}

pub fn check_token(k: &str, v: &str) -> R<()> {
    let key_ok = k.len() > 2
        && k.len() <= 50
        && k.starts_with("--")
        && k[2..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !key_ok {
        return Err(format!("токен {k}: имя должно быть вида --name"));
    }
    let l = v.to_ascii_lowercase();
    let bad_char = v
        .chars()
        .any(|c| matches!(c, ';' | '{' | '}' | '<' | '>' | '\\') || c.is_control());
    let bad_word = [
        "url(",
        "image-set(",
        "src(",
        "expression",
        "javascript:",
        "@",
    ]
    .iter()
    .any(|b| l.contains(b));
    if v.len() > 200 || bad_char || bad_word {
        return Err(format!("токен {k}: недопустимое значение"));
    }
    Ok(())
}

pub fn check_css(css: &str) -> R<()> {
    if css.len() > 256 * 1024 {
        return Err("CSS больше 256 КБ".into());
    }
    // Comments are dropped first (to nothing, so `u/**/rl(` turns into `url(` and is checked:
    // stricter than the browser, never looser).
    let l = strip_comments(&css.to_ascii_lowercase());
    // Backslash is banned outright: CSS escapes could spell `url(` invisibly.
    for bad in [
        "\\",
        "@import",
        "expression(",
        "javascript:",
        "behavior:",
        "-moz-binding",
        "image-set(",
        "src(",
        "</",
    ] {
        if l.contains(bad) {
            return Err(format!("CSS: запрещено «{bad}»"));
        }
    }
    let mut rest = l.as_str();
    while let Some(i) = rest.find("url(") {
        let after = &rest[i + 4..];
        let end = after.find(')').ok_or("CSS: незакрытый url(")?;
        let arg = after[..end].trim().trim_matches(|c| c == '"' || c == '\'');
        if !arg.starts_with("data:image/") && !safe_rel(arg, &ASSETS) {
            return Err(format!("CSS: url({arg}) — можно только файлы темы"));
        }
        rest = &after[end..];
    }
    Ok(())
}

/// Removes `/* … */`; an unterminated comment runs to the end, as in browsers.
fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(e) => rest = &rest[i + 2 + e + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

pub fn check_template(html: &str) -> R<()> {
    if html.len() > 128 * 1024 {
        return Err("шаблон больше 128 КБ".into());
    }
    let l = html.to_ascii_lowercase();
    for tag in [
        "<script",
        "<iframe",
        "<frame",
        "<object",
        "<embed",
        "<link",
        "<meta",
        "<base",
        "<style",
        "<form",
        "<foreignobject",
        "<animate",
        "<set",
        "<math",
        "<portal",
    ] {
        if l.contains(tag) {
            return Err(format!("шаблон: тег {tag}> запрещён"));
        }
    }
    for bad in [
        "javascript:",
        "vbscript:",
        "srcdoc",
        "srcset",
        "formaction",
        "xlink:href",
    ] {
        if l.contains(bad) {
            return Err(format!("шаблон: «{bad}» запрещено"));
        }
    }
    // Walk real tags and their attributes (like the HTML tokenizer), so text such as
    // `alt="one = two"` or prose outside tags is never mistaken for an attribute.
    let b = l.as_bytes();
    let n = b.len();
    let ws = |c: u8| c.is_ascii_whitespace();
    let mut i = 0;
    while i < n {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        i += 1;
        if l[i..].starts_with("!--") {
            match l[i..].find("-->") {
                Some(e) => {
                    i += e + 3;
                    continue;
                }
                None => break,
            }
        }
        while i < n && !(ws(b[i]) || b[i] == b'/' || b[i] == b'>') {
            i += 1;
        }
        loop {
            while i < n && (ws(b[i]) || b[i] == b'/') {
                i += 1;
            }
            if i >= n || b[i] == b'>' {
                i += 1;
                break;
            }
            let s = i;
            while i < n && !(ws(b[i]) || matches!(b[i], b'/' | b'>' | b'=')) {
                i += 1;
            }
            let name = &l[s..i];
            let mut j = i;
            while j < n && ws(b[j]) {
                j += 1;
            }
            let mut val = "";
            if j < n && b[j] == b'=' {
                j += 1;
                while j < n && ws(b[j]) {
                    j += 1;
                }
                if j < n && (b[j] == b'"' || b[j] == b'\'') {
                    let q = b[j] as char;
                    let vs = j + 1;
                    let ve = l[vs..].find(q).map_or(n, |e| vs + e);
                    val = &l[vs..ve];
                    i = (ve + 1).min(n);
                } else {
                    let vs = j;
                    while j < n && !(ws(b[j]) || b[j] == b'>') {
                        j += 1;
                    }
                    val = &l[vs..j];
                    i = j;
                }
            }
            check_attr(name, val)?;
        }
    }
    Ok(())
}

fn check_attr(name: &str, val: &str) -> R<()> {
    if name.starts_with("on") {
        return Err(format!("шаблон: обработчик {name}= запрещён"));
    }
    if name == "style" {
        return Err("шаблон: атрибут style запрещён — стили только в CSS темы".into());
    }
    if matches!(
        name,
        "href" | "src" | "action" | "background" | "poster" | "data"
    ) {
        let v = val.trim();
        if !(v.is_empty() || v.starts_with('#') || safe_rel(v, &ASSETS)) {
            return Err(format!("шаблон: {name}=\"{v}\" — только файлы темы или #"));
        }
    }
    Ok(())
}

fn read_text(data_dir: &Path, id: &str, rel: &str, max: u64) -> R<String> {
    let p = file(data_dir, id, rel).ok_or(format!("{rel}: файл не найден"))?;
    let len = fs::metadata(&p).map_err(|e| format!("{rel}: {e}"))?.len();
    if len > max {
        return Err(format!("{rel}: слишком большой файл"));
    }
    fs::read_to_string(&p).map_err(|e| format!("{rel}: {e}"))
}

/// A file of theme `id`, only if it really lies inside that theme's folder.
pub fn file(data_dir: &Path, id: &str, rel: &str) -> Option<PathBuf> {
    if !valid_id(id) || !(safe_rel(rel, &ASSETS) || safe_rel(rel, &["html", "json"])) {
        return None;
    }
    let root = dir(data_dir).join(id).canonicalize().ok()?;
    let p = root.join(rel).canonicalize().ok()?;
    (p.starts_with(&root) && p.is_file()).then_some(p)
}

/// Loads and fully checks an installed theme.
pub fn load(data_dir: &Path, id: &str) -> R<Manifest> {
    if !valid_id(id) {
        return Err("неверный id темы".into());
    }
    let text = read_text(data_dir, id, "theme.json", 64 * 1024)?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("theme.json: {e}"))?;
    if m.id != id {
        return Err("id в theme.json не совпадает с именем папки".into());
    }
    if builtin(id).is_some() {
        return Err("id занят встроенной темой".into());
    }
    if m.name.trim().is_empty() || m.name.chars().count() > 60 {
        return Err("name: от 1 до 60 символов".into());
    }
    let mut m = m;
    if let Some(u) = m.ui.take() {
        m.ui = Some(u.checked().map_err(|e| format!("theme.json: {e}"))?);
    }
    if m.tokens.len() > 300 || m.css.len() > 8 {
        return Err("слишком много токенов или CSS-файлов".into());
    }
    for (k, v) in &m.tokens {
        check_token(k, v)?;
    }
    for c in &m.css {
        if !safe_rel(c, &["css"]) {
            return Err(format!("css: {c} — нужен относительный путь к .css"));
        }
        check_css(&read_text(data_dir, id, c, 256 * 1024)?)?;
    }
    for (screen, t) in &m.templates {
        if !SCREENS.contains(&screen.as_str()) {
            return Err(format!("templates: экран {screen} нельзя заменить"));
        }
        if !safe_rel(t, &["html"]) {
            return Err(format!("templates: {t} — нужен относительный путь к .html"));
        }
        check_template(&read_text(data_dir, id, t, 128 * 1024)?)?;
    }
    Ok(m)
}

fn builtin_json(b: (&str, &str, Base)) -> Value {
    json!({ "id": b.0, "name": b.1, "base": b.2, "builtin": true,
            "tokens": {}, "css": [], "templates": [], "ui": null })
}

/// The theme the UI should apply now. A broken theme falls back to the default one.
pub fn current(data_dir: &Path, active: &str) -> Value {
    if let Some(b) = builtin(active) {
        return builtin_json(b);
    }
    match load(data_dir, active) {
        Ok(m) => json!({
            "id": m.id, "name": m.name, "base": m.base, "builtin": false,
            "tokens": m.tokens, "css": m.css, "ui": m.ui,
            "templates": m.templates.keys().collect::<Vec<_>>(),
        }),
        Err(e) => {
            let mut v = builtin_json(BUILTIN[0]);
            v["fallback"] = json!({ "theme": active, "error": e });
            v
        }
    }
}

/// Built-in themes first, then installed ones (broken ones listed with `error`).
pub fn list(data_dir: &Path) -> Vec<Value> {
    let mut out: Vec<Value> = BUILTIN.iter().map(|b| builtin_json(*b)).collect();
    let mut ids: Vec<String> = fs::read_dir(dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_id(n))
        .collect();
    ids.sort();
    for id in ids {
        out.push(match load(data_dir, &id) {
            Ok(m) => json!({
                "id": m.id, "name": m.name, "version": m.version, "author": m.author,
                "base": m.base, "builtin": false,
                "templates": m.templates.keys().collect::<Vec<_>>(),
            }),
            Err(e) => json!({ "id": id, "name": id, "builtin": false, "error": e }),
        });
    }
    out
}

/// Checked HTML of the active theme's template for `screen`, if it has one.
pub fn template(data_dir: &Path, active: &str, screen: &str) -> R<Option<String>> {
    if builtin(active).is_some() {
        return Ok(None);
    }
    let m = load(data_dir, active)?;
    let Some(rel) = m.templates.get(screen) else {
        return Ok(None);
    };
    let html = read_text(data_dir, active, rel, 128 * 1024)?;
    check_template(&html)?;
    Ok(Some(html))
}

/// Folder-safe id from a display name; unique among installed and built-in themes.
pub fn new_id(data_dir: &Path, name: &str) -> String {
    const RU: [(&str, &str); 33] = [
        ("а", "a"),
        ("б", "b"),
        ("в", "v"),
        ("г", "g"),
        ("д", "d"),
        ("е", "e"),
        ("ё", "e"),
        ("ж", "zh"),
        ("з", "z"),
        ("и", "i"),
        ("й", "y"),
        ("к", "k"),
        ("л", "l"),
        ("м", "m"),
        ("н", "n"),
        ("о", "o"),
        ("п", "p"),
        ("р", "r"),
        ("с", "s"),
        ("т", "t"),
        ("у", "u"),
        ("ф", "f"),
        ("х", "h"),
        ("ц", "c"),
        ("ч", "ch"),
        ("ш", "sh"),
        ("щ", "sch"),
        ("ъ", ""),
        ("ы", "y"),
        ("ь", ""),
        ("э", "e"),
        ("ю", "yu"),
        ("я", "ya"),
    ];
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        let mut b = [0u8; 4];
        let cs: &str = c.encode_utf8(&mut b);
        let t = RU.iter().find(|(k, _)| *k == cs).map(|(_, v)| *v);
        match t {
            Some(v) => slug.push_str(v),
            None if c.is_ascii_alphanumeric() => slug.push(c),
            None => {
                if !slug.ends_with('-') {
                    slug.push('-')
                }
            }
        }
    }
    let base: String = slug.trim_matches('-').chars().take(32).collect();
    let base = if base.trim_matches('-').is_empty() {
        "theme".to_string()
    } else {
        base.trim_end_matches('-').to_string()
    };
    let free = |id: &str| builtin(id).is_none() && !dir(data_dir).join(id).exists();
    if free(&base) {
        return base;
    }
    (2..1000)
        .map(|n| format!("{base}-{n}"))
        .find(|id| free(id))
        .unwrap_or_else(|| format!("{base}-x"))
}

fn copy_tree(from: &Path, to: &Path, depth: u8) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        let ft = e.file_type()?;
        if ft.is_dir() && depth < 4 {
            copy_tree(&src, &dst, depth + 1)?;
        } else if ft.is_file() {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Saves the active theme plus `ui` as a new installed theme named `name`; returns its id.
/// Saves the active theme plus the editor's edits (`ui`) and the user's base
/// colours (`tokens`, already resolved to CSS custom properties) as a new theme.
pub fn save(
    data_dir: &Path,
    active: &str,
    name: &str,
    ui: UiCustom,
    tokens: BTreeMap<String, String>,
) -> R<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err("название: от 1 до 60 символов".into());
    }
    if tokens.len() > 60 {
        return Err("слишком много цветов".into());
    }
    for (k, v) in &tokens {
        check_token(k, v)?;
    }
    let id = new_id(data_dir, name);
    let to = dir(data_dir).join(&id);
    let mut m = match builtin(active) {
        Some(b) => Manifest {
            id: String::new(),
            name: String::new(),
            version: String::new(),
            author: String::new(),
            base: b.2,
            tokens: BTreeMap::new(),
            css: vec![],
            templates: BTreeMap::new(),
            ui: None,
        },
        None => {
            let m = load(data_dir, active)?;
            copy_tree(&dir(data_dir).join(active), &to, 0)
                .map_err(|e| format!("копирование темы: {e}"))?;
            m
        }
    };
    let ui = m.ui.take().unwrap_or_default().layered(&ui).checked()?;
    m.ui = (ui != UiCustom::default()).then_some(ui);
    m.tokens.extend(tokens);
    m.id = id.clone();
    m.name = name.to_string();
    m.version = "1.0.0".into();
    let res = fs::create_dir_all(&to)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::to_vec_pretty(&m).map_err(|e| e.to_string()))
        .and_then(|j| fs::write(to.join("theme.json"), j).map_err(|e| e.to_string()))
        .and_then(|_| load(data_dir, &id).map(|_| ()));
    if let Err(e) = res {
        let _ = fs::remove_dir_all(&to);
        return Err(e);
    }
    Ok(id)
}

#[cfg(test)]
mod save_tests {
    use super::*;

    #[test]
    fn saves_editor_edits_as_a_theme() {
        let d = std::env::temp_dir().join(format!("fl-theme-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let ui: UiCustom = serde_json::from_value(json!({ "radius": { "card": 150 } })).unwrap();
        let id = save(&d, "light", "Моя тема", ui, BTreeMap::new()).unwrap();
        assert_eq!(id, "moya-tema");
        let m = load(&d, &id).unwrap();
        assert_eq!(m.base, Base::Light);
        assert_eq!(m.ui.unwrap().radius["card"], 150);
        let over: UiCustom = serde_json::from_value(json!({ "radius": { "card": 200 } })).unwrap();
        let id2 = save(&d, &id, "Моя тема", over, BTreeMap::new()).unwrap();
        assert_eq!(id2, "moya-tema-2");
        assert_eq!(load(&d, &id2).unwrap().ui.unwrap().radius["card"], 300);
        assert!(save(&d, "default", "  ", UiCustom::default(), BTreeMap::new()).is_err());
        let t: BTreeMap<String, String> = [
            ("--accent".to_string(), "#ff8800".to_string()),
            ("--glass".to_string(), "rgba(14, 16, 22, var(--glass-a))".to_string()),
        ]
        .into();
        let id3 = save(&d, "default", "Цвета", UiCustom::default(), t).unwrap();
        let m3 = load(&d, &id3).unwrap();
        assert_eq!(m3.tokens["--accent"], "#ff8800");
        assert!(m3.tokens["--glass"].contains("var(--glass-a)"));
        let bad: BTreeMap<String, String> =
            [("--accent".to_string(), "url(x)".to_string())].into();
        assert!(save(&d, "default", "Плохо", UiCustom::default(), bad).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
