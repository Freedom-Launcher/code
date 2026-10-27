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

/// Files a `.fltheme` archive may carry: the manifest, templates, assets and a readme.
const PACKAGE: [&str; 14] = [
    "json", "html", "css", "png", "jpg", "jpeg", "webp", "gif", "svg", "woff2", "woff", "ttf",
    "md", "txt",
];
const MAX_FILES: usize = 512;
const MAX_BYTES: u64 = 32 << 20;

/// Archive litter added by file managers; skipped, never installed.
fn junk(n: &str) -> bool {
    n.starts_with("__MACOSX/")
        || n.rsplit('/')
            .next()
            .is_some_and(|b| matches!(b, ".DS_Store" | "Thumbs.db" | "desktop.ini"))
}

/// Installs a theme from a `.fltheme` (a zip): `theme.json` at the top of the
/// archive or inside one top folder. An installed theme with the same id is
/// replaced only if the new one passes every check; otherwise the old one stays.
pub fn install_zip(data_dir: &Path, zip_path: &Path) -> R<String> {
    use std::io::Read;
    let f = fs::File::open(zip_path).map_err(|_| "файл не найден".to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "это не файл темы (.fltheme)".to_string())?;
    if z.len() > MAX_FILES {
        return Err(format!("в архиве больше {MAX_FILES} файлов"));
    }
    let names: Vec<String> = (0..z.len())
        .filter_map(|i| z.by_index(i).ok().map(|e| e.name().replace('\\', "/")))
        .filter(|n| !junk(n))
        .collect();
    let prefix = if names.iter().any(|n| n == "theme.json") {
        String::new()
    } else {
        let top = names
            .first()
            .and_then(|n| n.split('/').next())
            .unwrap_or_default()
            .to_owned();
        let p = format!("{top}/");
        if top.is_empty()
            || !names.iter().all(|n| n.starts_with(&p))
            || !names.iter().any(|n| *n == format!("{p}theme.json"))
        {
            return Err("в архиве нет theme.json".into());
        }
        p
    };
    let mut raw = Vec::new();
    z.by_name(&format!("{prefix}theme.json"))
        .map_err(|_| "в архиве нет theme.json".to_string())?
        .take(64 * 1024 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| format!("theme.json: {e}"))?;
    let id = serde_json::from_slice::<Value>(&raw)
        .ok()
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .filter(|id| valid_id(id) && builtin(id).is_none())
        .ok_or("theme.json: неверный id")?;
    let root = dir(data_dir);
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let tmp = root.join(format!(".install-{}", uuid::Uuid::new_v4()));
    if let Err(e) = extract(&mut z, &prefix, &tmp) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let dest = root.join(&id);
    let old = root.join(format!(".old-{}", uuid::Uuid::new_v4()));
    let had = dest.exists();
    if had {
        fs::rename(&dest, &old).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&tmp, &dest) {
        if had {
            let _ = fs::rename(&old, &dest);
        }
        let _ = fs::remove_dir_all(&tmp);
        return Err(e.to_string());
    }
    if let Err(e) = load(data_dir, &id) {
        let _ = fs::remove_dir_all(&dest);
        if had {
            let _ = fs::rename(&old, &dest);
        }
        return Err(e);
    }
    let _ = fs::remove_dir_all(&old);
    Ok(id)
}

fn extract(z: &mut zip::ZipArchive<fs::File>, prefix: &str, to: &Path) -> R<()> {
    use std::io::Read;
    let mut total = 0u64;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|_| "архив повреждён".to_string())?;
        let name = e.name().replace('\\', "/");
        if junk(&name) {
            continue;
        }
        let rel = name.strip_prefix(prefix).unwrap_or_default().to_owned();
        if e.is_dir() || rel.is_empty() {
            continue;
        }
        if e.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("ссылки в архиве не допускаются".into());
        }
        if !safe_rel(&rel, &PACKAGE) {
            return Err(format!("{rel}: недопустимое имя или тип файла"));
        }
        let mut data = Vec::new();
        (&mut e)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|_| "архив повреждён".to_string())?;
        total += data.len() as u64;
        if total > MAX_BYTES {
            return Err("тема больше 32 МБ".into());
        }
        let out = to.join(&rel);
        if let Some(d) = out.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        fs::write(&out, data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Packs installed theme `id` into a `.fltheme` file (a zip, `theme.json` on top).
pub fn export_zip(data_dir: &Path, id: &str, out: &Path) -> R<()> {
    use std::io::Write;
    load(data_dir, id)?;
    let root = dir(data_dir).join(id);
    let mut files = Vec::new();
    collect(&root, "", 0, &mut files).map_err(|e| e.to_string())?;
    let part = out.with_extension("fltheme-part");
    let res = (|| -> R<()> {
        let f = fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut w = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for rel in &files {
            let data = fs::read(root.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
            w.start_file(rel.as_str(), o).map_err(|e| e.to_string())?;
            w.write_all(&data).map_err(|e| e.to_string())?;
        }
        w.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    match res {
        Ok(()) => fs::rename(&part, out).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
}

fn collect(d: &Path, pre: &str, depth: u8, out: &mut Vec<String>) -> std::io::Result<()> {
    if depth > 6 {
        return Ok(());
    }
    let mut es: Vec<_> = fs::read_dir(d)?.flatten().collect();
    es.sort_by_key(|e| e.file_name());
    for e in es {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = if pre.is_empty() { name } else { format!("{pre}/{name}") };
        let t = e.file_type()?;
        if t.is_symlink() {
            continue;
        }
        if t.is_dir() {
            collect(&e.path(), &rel, depth + 1, out)?;
        } else if t.is_file() && safe_rel(&rel, &PACKAGE) {
            out.push(rel);
        }
        if out.len() > MAX_FILES {
            return Err(std::io::Error::other("слишком много файлов"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod install_tests {
    use super::*;
    use std::io::Write;

    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, b) in files {
            w.start_file(*n, o).unwrap();
            w.write_all(b.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    const M: &str = r##"{"id":"neon","name":"Неон","base":"dark","tokens":{"--accent":"#ff2a6d"},"css":["theme.css"]}"##;

    #[test]
    fn installs_replaces_rolls_back_and_exports() {
        let d = std::env::temp_dir().join(format!("fl-tinst-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        let z = d.join("a.fltheme");
        let css = |d: &Path| fs::read_to_string(dir(d).join("neon/theme.css")).unwrap();
        // One top folder, with macOS litter around it.
        zip_of(
            &z,
            &[("neon/theme.json", M), ("neon/theme.css", "a{color:red}"), ("__MACOSX/neon/._theme.css", "x")],
        );
        assert_eq!(install_zip(&d, &z).unwrap(), "neon");
        // Flat layout replaces in place.
        zip_of(&z, &[("theme.json", M), ("theme.css", "a{color:blue}"), ("README.md", "hi")]);
        assert_eq!(install_zip(&d, &z).unwrap(), "neon");
        assert!(css(&d).contains("blue"));
        // A broken update keeps the working theme.
        zip_of(&z, &[("theme.json", M), ("theme.css", "@import 'x.css';")]);
        assert!(install_zip(&d, &z).is_err());
        assert!(css(&d).contains("blue"));
        // Built-in ids, escaping paths, scripts and non-archives are refused.
        zip_of(&z, &[("theme.json", r#"{"id":"default","name":"x"}"#)]);
        assert!(install_zip(&d, &z).is_err());
        zip_of(&z, &[("theme.json", M), ("theme.css", "a{}"), ("../evil.css", "a{}")]);
        assert!(install_zip(&d, &z).is_err());
        zip_of(&z, &[("theme.json", M), ("theme.css", "a{}"), ("x.js", "alert(1)")]);
        assert!(install_zip(&d, &z).is_err());
        zip_of(&z, &[("readme.txt", "no manifest")]);
        assert!(install_zip(&d, &z).is_err());
        fs::write(&z, "nope").unwrap();
        assert!(install_zip(&d, &z).is_err());
        assert!(css(&d).contains("blue"));
        // Export round-trips.
        let out = d.join("neon.fltheme");
        export_zip(&d, "neon", &out).unwrap();
        fs::remove_dir_all(dir(&d).join("neon")).unwrap();
        assert_eq!(install_zip(&d, &out).unwrap(), "neon");
        assert_eq!(load(&d, "neon").unwrap().tokens["--accent"], "#ff2a6d");
        assert!(dir(&d).join("neon/README.md").is_file());
        assert!(export_zip(&d, "default", &out).is_err());
        // No temporary folders are left behind.
        let left: Vec<_> = fs::read_dir(dir(&d))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(left.is_empty());
        let _ = fs::remove_dir_all(&d);
    }
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
