//! «Доктор сборки». Before the game starts it reads each mod's own metadata
//! (`fabric.mod.json`, `quilt.mod.json`, `mods.toml`) and finds what would stop
//! the instance: mods for another loader or game version, missing or switched
//! off dependencies, two copies of one mod, declared conflicts, OptiFine next to
//! Sodium, broken jars and packs dropped into `mods/`. Most findings carry a fix.
//! [`verify`] checks jars against Modrinth hashes and known malware markers.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::{
    modrinth::Kind,
    mods::{self, ModEntry},
    CoreError,
};

const OFF: &str = ".disabled";
/// Mod ids every loader provides on its own.
const BUILTIN: &[&str] = &[
    "minecraft", "java", "fabricloader", "fabric-loader", "quilt_loader", "quilted_fabric_loader",
    "forge", "neoforge", "fml", "javafml", "lowcodefml", "mixinextras", "mixin",
];
/// Mods that break when OptiFine is next to them.
const RENDERERS: &[&str] = &["sodium", "embeddium", "rubidium", "magnesium", "iris", "oculus"];
/// Strings planted by the fractureiser malware (June 2023).
const MARKERS: &[&[u8]] = &[b"85.217.144.130", b"107.189.3.101", b"files-8ie.pages.dev"];
const MAX_META: u64 = 1 << 20;
const MAX_NESTED: usize = 96;
const MAX_NESTED_BYTES: u64 = 16 << 20;

// ---------- Versions ----------

/// What a mod wants from another one: Fabric predicates (any of) or Maven ranges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Req {
    pub any: Vec<String>,
    pub maven: bool,
}

impl Req {
    fn fabric(v: &Value) -> Req {
        let any = match v {
            Value::String(s) => vec![s.clone()],
            Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
            _ => Vec::new(),
        };
        Req { any, maven: false }
    }
    fn maven(range: String) -> Req {
        Req { any: vec![range], maven: true }
    }
    fn anything(&self) -> bool {
        self.any.iter().all(|p| matches!(p.trim(), "" | "*"))
    }
    /// `Some(false)` only when the version surely does not fit.
    pub fn fits(&self, v: &[u64]) -> Option<bool> {
        if self.any.is_empty() {
            return Some(true);
        }
        let mut unknown = false;
        for p in &self.any {
            let r = if self.maven { maven_ok(p, v) } else { fabric_ok(p, v) };
            match r {
                Some(true) => return Some(true),
                None => unknown = true,
                Some(false) => {}
            }
        }
        if unknown {
            None
        } else {
            Some(false)
        }
    }
}

/// "1.20.1+build.3" → [1, 20, 1]; `None` for anything that is not plain numbers.
pub fn nums(v: &str) -> Option<Vec<u64>> {
    let core = v.trim().split(['-', '+', ' ']).next()?;
    if core.is_empty() {
        return None;
    }
    core.split('.').map(|p| p.parse().ok()).collect()
}

fn cmp(a: &[u64], b: &[u64]) -> Ordering {
    for i in 0..a.len().max(b.len()) {
        let o = a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0));
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// "1.20.x" → ([1, 20], true).
fn wild(v: &str) -> Option<(Vec<u64>, bool)> {
    let core = v.trim().split(['-', '+']).next()?;
    let mut out = Vec::new();
    for p in core.split('.') {
        if matches!(p, "x" | "X" | "*") {
            return Some((out, true));
        }
        out.push(p.parse().ok()?);
    }
    Some((out, false))
}

fn bump(w: &[u64], i: usize) -> Vec<u64> {
    let mut b = w[..=i].to_vec();
    b[i] += 1;
    b
}

fn fabric_one(p: &str, v: &[u64]) -> Option<bool> {
    let p = p.trim();
    if p.is_empty() || p == "*" {
        return Some(true);
    }
    let (op, rest) = [">=", "<=", ">", "<", "=", "~", "^"]
        .into_iter()
        .find_map(|o| p.strip_prefix(o).map(|r| (o, r)))
        .unwrap_or(("", p));
    let (w, is_wild) = wild(rest)?;
    if w.is_empty() {
        return Some(true);
    }
    let o = cmp(v, &w);
    let prefix = v.len() >= w.len() && v[..w.len()] == w[..];
    Some(match op {
        ">=" => o.is_ge(),
        ">" => o.is_gt(),
        "<=" => o.is_le() || (is_wild && prefix),
        "<" => o.is_lt(),
        "~" => o.is_ge() && cmp(v, &bump(&w, usize::from(w.len() >= 2))).is_lt(),
        "^" => {
            let i = w.iter().position(|&x| x != 0).unwrap_or(w.len() - 1);
            o.is_ge() && cmp(v, &bump(&w, i)).is_lt()
        }
        _ if is_wild => prefix,
        _ => o.is_eq(),
    })
}

/// A Fabric version predicate: space-separated parts must all hold, `||` joins alternatives.
fn fabric_ok(p: &str, v: &[u64]) -> Option<bool> {
    let mut unknown = false;
    for alt in p.split("||") {
        let mut toks: Vec<String> = Vec::new();
        let mut op = String::new();
        for t in alt.split_whitespace() {
            if t.chars().all(|c| "<>=~^".contains(c)) {
                op.push_str(t);
                continue;
            }
            toks.push(format!("{op}{t}"));
            op.clear();
        }
        let mut res = Some(true);
        for t in &toks {
            match fabric_one(t, v) {
                Some(false) => {
                    res = Some(false);
                    break;
                }
                None => res = None,
                _ => {}
            }
        }
        match res {
            Some(true) => return Some(true),
            None => unknown = true,
            _ => {}
        }
    }
    if unknown {
        None
    } else {
        Some(false)
    }
}

/// A Maven range like `[1.20.1,1.21)` or a union of them. A bare version is only a
/// recommendation in Forge, so it always fits.
fn maven_ok(r: &str, v: &[u64]) -> Option<bool> {
    let r = r.trim();
    if !r.starts_with(['[', '(']) {
        return Some(true);
    }
    let (mut rest, mut unknown) = (r, false);
    while let Some(start) = rest.find(['[', '(']) {
        let after = &rest[start..];
        let end = after.find([']', ')'])?;
        match maven_one(&after[..=end], v) {
            Some(true) => return Some(true),
            None => unknown = true,
            Some(false) => {}
        }
        rest = &after[end + 1..];
    }
    if unknown {
        None
    } else {
        Some(false)
    }
}

fn maven_one(seg: &str, v: &[u64]) -> Option<bool> {
    let (lo_in, hi_in) = (seg.starts_with('['), seg.ends_with(']'));
    let inner = &seg[1..seg.len() - 1];
    let Some((a, b)) = inner.split_once(',') else {
        return Some(cmp(v, &nums(inner)?).is_eq());
    };
    let (a, b) = (a.trim(), b.trim());
    if !a.is_empty() {
        let o = cmp(v, &nums(a)?);
        if o.is_lt() || (!lo_in && o.is_eq()) {
            return Some(false);
        }
    }
    if !b.is_empty() {
        let o = cmp(v, &nums(b)?);
        if o.is_gt() || (!hi_in && o.is_eq()) {
            return Some(false);
        }
    }
    Some(true)
}

// ---------- Jar metadata ----------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    /// `fabric`, `quilt`, `forge`, `neoforge`, `forgelike` (mods.toml that names
    /// neither) or `legacy` (mcmod.info, before 1.13).
    pub loaders: Vec<&'static str>,
    pub id: String,
    pub name: String,
    pub version: String,
    /// Other ids this jar answers to, nested jars included.
    pub provides: Vec<String>,
    pub mc: Option<Req>,
    /// Required mod ids (the loader's own ids left out).
    pub depends: Vec<String>,
    pub breaks: Vec<(String, Req)>,
    pub optifine: bool,
    pub neko: bool,
}

fn builtin(id: &str) -> bool {
    BUILTIN.contains(&id)
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_owned()
}

fn set_ids(m: &mut Meta, id: String, name: String, version: String) {
    if m.id.is_empty() {
        m.id = id;
    } else if !id.is_empty() && id != m.id {
        m.provides.push(id);
    }
    if m.name.is_empty() {
        m.name = name;
    }
    if m.version.is_empty() {
        m.version = version;
    }
}

fn fabric(v: &Value, m: &mut Meta) {
    m.loaders.push("fabric");
    set_ids(m, s(&v["id"]), s(&v["name"]), s(&v["version"]));
    for p in v["provides"].as_array().into_iter().flatten() {
        if let Some(p) = p.as_str() {
            m.provides.push(p.to_owned());
        }
    }
    if let Some(d) = v["depends"].as_object() {
        for (k, r) in d {
            if k == "minecraft" {
                m.mc.get_or_insert_with(|| Req::fabric(r));
            } else if !builtin(k) {
                m.depends.push(k.clone());
            }
        }
    }
    if let Some(b) = v["breaks"].as_object() {
        for (k, r) in b {
            m.breaks.push((k.clone(), Req::fabric(r)));
        }
    }
}

fn quilt(v: &Value, m: &mut Meta) {
    let q = &v["quilt_loader"];
    m.loaders.push("quilt");
    set_ids(m, s(&q["id"]), s(&q["metadata"]["name"]), s(&q["version"]));
    for p in q["provides"].as_array().into_iter().flatten() {
        let id = p.as_str().map_or_else(|| s(&p["id"]), str::to_owned);
        if !id.is_empty() {
            m.provides.push(id);
        }
    }
    let entry = |d: &Value| match d {
        Value::String(x) => (x.clone(), Value::Null, false),
        _ => (s(&d["id"]), d["versions"].clone(), d["optional"].as_bool().unwrap_or(false)),
    };
    for d in q["depends"].as_array().into_iter().flatten() {
        let (id, ver, optional) = entry(d);
        if optional || id.is_empty() {
            continue;
        }
        if id == "minecraft" {
            m.mc.get_or_insert_with(|| Req::fabric(&ver));
        } else if !builtin(&id) {
            m.depends.push(id);
        }
    }
    for d in q["breaks"].as_array().into_iter().flatten() {
        let (id, ver, _) = entry(d);
        if !id.is_empty() {
            m.breaks.push((id, Req::fabric(&ver)));
        }
    }
}

fn toml_value(v: &str) -> String {
    for q in ['"', '\''] {
        if let Some(r) = v.strip_prefix(q) {
            return r.split(q).next().unwrap_or("").to_owned();
        }
    }
    v.split('#').next().unwrap_or("").trim().to_owned()
}

/// Reads the parts of a Forge / NeoForge `mods.toml` the doctor needs.
fn toml(t: &str, m: &mut Meta, neo: bool) {
    #[derive(Default)]
    struct Dep {
        id: String,
        range: String,
        mandatory: bool,
        kind: String,
    }
    enum At {
        Top,
        Mods,
        Dep,
        Other,
    }
    let (mut at, mut deps, mut lang) = (At::Top, Vec::<Dep>::new(), String::new());
    let mut lines = t.lines();
    while let Some(line) = lines.next() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') {
            at = if l.starts_with("[[mods]]") {
                At::Mods
            } else if l.starts_with("[[dependencies") {
                deps.push(Dep::default());
                At::Dep
            } else {
                At::Other
            };
            continue;
        }
        let Some((k, v)) = l.split_once('=') else { continue };
        let (k, v) = (k.trim().trim_matches('"'), v.trim());
        // Multi-line strings run to their closing quotes.
        if let Some(q) = ["'''", "\"\"\""].into_iter().find(|q| v.starts_with(q) && !v[3..].contains(q)) {
            for l2 in lines.by_ref() {
                if l2.contains(q) {
                    break;
                }
            }
            continue;
        }
        let val = toml_value(v);
        match at {
            At::Top if k == "modLoader" => lang = val,
            At::Mods => match k {
                "modId" => set_ids(m, val, String::new(), String::new()),
                "displayName" if m.name.is_empty() => m.name = val,
                "version" if m.version.is_empty() => m.version = val,
                _ => {}
            },
            At::Dep => {
                let d = deps.last_mut().expect("pushed with the header");
                match k {
                    "modId" => d.id = val,
                    "versionRange" => d.range = val,
                    "mandatory" => d.mandatory = val == "true",
                    "type" => d.kind = val.to_ascii_lowercase(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let mut loader = if neo { "neoforge" } else { "forgelike" };
    for d in deps {
        if d.id.is_empty() {
            continue;
        }
        if d.kind == "incompatible" {
            m.breaks.push((d.id, Req::maven(d.range)));
            continue;
        }
        if !(d.mandatory || d.kind == "required") {
            continue;
        }
        match d.id.as_str() {
            "minecraft" => {
                m.mc.get_or_insert_with(|| Req::maven(d.range));
            }
            "neoforge" => loader = "neoforge",
            "forge" if !neo => loader = "forge",
            id if !builtin(id) => m.depends.push(d.id),
            _ => {}
        }
    }
    if lang == "kotlinforforge" {
        m.depends.push(lang);
    }
    m.loaders.push(loader);
}

fn text<R: Read + Seek>(z: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let f = z.by_name(name).ok()?;
    if f.size() > MAX_META {
        return None;
    }
    let mut b = Vec::new();
    f.take(MAX_META).read_to_end(&mut b).ok()?;
    Some(String::from_utf8_lossy(&b).trim_start_matches('\u{feff}').to_owned())
}

fn read_archive<R: Read + Seek>(z: &mut zip::ZipArchive<R>, m: &mut Meta, depth: u8) {
    let names: Vec<String> = z.file_names().map(str::to_owned).collect();
    if depth == 0 {
        m.optifine = names.iter().any(|n| n.starts_with("net/optifine/"));
        m.neko = names.iter().any(|n| n.starts_with("dev/neko/"));
    }
    if let Some(v) = text(z, "fabric.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        fabric(&v, m);
    }
    if let Some(v) = text(z, "quilt.mod.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        quilt(&v, m);
    }
    if let Some(t) = text(z, "META-INF/neoforge.mods.toml") {
        toml(&t, m, true);
    } else if let Some(t) = text(z, "META-INF/mods.toml") {
        toml(&t, m, false);
    }
    if m.loaders.is_empty() && names.iter().any(|n| n == "mcmod.info") {
        m.loaders.push("legacy");
    }
    if depth >= 2 {
        return;
    }
    let nested = names
        .iter()
        .filter(|n| (n.starts_with("META-INF/jars/") || n.starts_with("META-INF/jarjar/")) && n.ends_with(".jar"))
        .take(MAX_NESTED);
    for n in nested {
        let Ok(f) = z.by_name(n) else { continue };
        if f.size() > MAX_NESTED_BYTES {
            continue;
        }
        let mut b = Vec::with_capacity(f.size() as usize);
        if f.take(MAX_NESTED_BYTES).read_to_end(&mut b).is_err() {
            continue;
        }
        let Ok(mut inner) = zip::ZipArchive::new(Cursor::new(b)) else { continue };
        let mut sub = Meta::default();
        read_archive(&mut inner, &mut sub, depth + 1);
        if !sub.id.is_empty() {
            m.provides.push(sub.id);
        }
        m.provides.extend(sub.provides);
    }
}

fn read_uncached(p: &Path) -> Result<Meta, String> {
    let f = fs::File::open(p).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "not a jar".to_string())?;
    let mut m = Meta::default();
    read_archive(&mut z, &mut m, 0);
    Ok(m)
}

type Cached = (u64, u128, Result<Meta, String>);

fn cache() -> &'static Mutex<HashMap<PathBuf, Cached>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Cached>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// A jar's metadata, cached by path, size and modification time.
pub fn read_jar(p: &Path) -> Result<Meta, String> {
    let md = fs::metadata(p).map_err(|e| e.to_string())?;
    let stamp = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    if let Some((len, t, m)) = cache().lock().unwrap_or_else(|e| e.into_inner()).get(p) {
        if *len == md.len() && *t == stamp {
            return m.clone();
        }
    }
    let m = read_uncached(p);
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if c.len() > 4000 {
        c.clear();
    }
    c.insert(p.to_owned(), (md.len(), stamp, m.clone()));
    m
}

// ---------- Findings ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Fix {
    Disable { files: Vec<String> },
    Enable { files: Vec<String> },
    /// Moves a pack out of `mods/` into `resourcepacks` or `shaderpacks`.
    Move { file: String, to: String },
    /// Installs a catalog project (done by the UI through `mods.install`).
    Install { project: String },
    /// Opens the catalog search (done by the UI).
    Search { query: String },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Issue {
    /// `broken`, `wrong_loader`, `wrong_game`, `duplicate`, `conflict`, `optifine`,
    /// `missing_dep`, `dep_off`, `stray_pack`, `no_loader`.
    pub kind: &'static str,
    /// `error` stops the game, `warn` is worth a look.
    pub level: &'static str,
    pub files: Vec<String>,
    pub names: Vec<String>,
    /// The other loader, the game version, the missing mod, the target folder…
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub issues: Vec<Issue>,
    pub mods: usize,
    pub enabled: usize,
}

/// A finished offline pass; [`finish`] adds what needs catalog titles.
#[derive(Debug, Default)]
pub struct Scan {
    pub report: Report,
    /// Catalog project id → titles of the mods that need it.
    pub wanted: BTreeMap<String, Vec<String>>,
    missing: BTreeMap<String, Vec<String>>,
    present: HashSet<String>,
}

fn issue(kind: &'static str, level: &'static str, files: Vec<String>, names: Vec<String>, detail: String, fix: Option<Fix>) -> Issue {
    Issue { kind, level, files, names, detail, fix }
}

fn norm(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    if n == "fabric" {
        "fabricapi".into()
    } else {
        n
    }
}

fn push_unique(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) {
        v.push(s);
    }
}

struct Jar {
    e: ModEntry,
    meta: Result<Meta, String>,
}

impl Jar {
    fn m(&self) -> Option<&Meta> {
        self.meta.as_ref().ok()
    }
    fn title(&self) -> String {
        match self.m() {
            Some(m) if self.e.source.is_none() && !m.name.is_empty() => m.name.clone(),
            _ => self.e.title.clone(),
        }
    }
    fn answers(&self, id: &str) -> bool {
        self.m().is_some_and(|m| m.id == id || m.provides.iter().any(|p| p == id))
    }
}

fn files(js: &[&Jar]) -> Vec<String> {
    js.iter().map(|j| j.e.file.clone()).collect()
}
fn titles(js: &[&Jar]) -> Vec<String> {
    js.iter().map(|j| j.title()).collect()
}

/// Loaders whose mods run on this instance.
fn accepted(loader: &str, mc: Option<&[u64]>, connector: bool) -> Vec<&'static str> {
    let before = |v: &[u64]| mc.is_none_or(|m| cmp(m, v).is_lt());
    let mut a = match loader {
        "fabric" => vec!["fabric"],
        "quilt" => vec!["quilt", "fabric"],
        "forge" => {
            let mut a = vec!["forge", "forgelike"];
            if before(&[1, 13]) {
                a.push("legacy");
            }
            a
        }
        "neoforge" => {
            let mut a = vec!["neoforge", "forgelike"];
            if before(&[1, 20, 2]) {
                a.push("forge");
            }
            a
        }
        _ => Vec::new(),
    };
    if connector && matches!(loader, "forge" | "neoforge") {
        a.push("fabric");
    }
    a
}

fn family(l: &str) -> &'static str {
    match l {
        "fabric" => "fabric",
        "quilt" => "quilt",
        "neoforge" => "neoforge",
        _ => "forge",
    }
}

/// Packs that ended up in `mods/` by mistake.
fn stray_packs(gd: &Path, out: &mut Vec<Issue>) {
    let Ok(rd) = fs::read_dir(mods::dir(gd, Kind::Mod)) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if mods::check_file(Kind::ResourcePack, &name).is_err() {
            continue;
        }
        let Ok(z) = fs::File::open(e.path()).map_err(|_| ()).and_then(|f| zip::ZipArchive::new(f).map_err(|_| ())) else { continue };
        let has = |p: &str| z.file_names().any(|n| n.starts_with(p));
        let to = if z.file_names().any(|n| n == "pack.mcmeta") && (has("assets/") || !has("data/")) {
            "resourcepacks"
        } else if has("shaders/") {
            "shaderpacks"
        } else {
            continue;
        };
        let title = name.trim_end_matches(".zip").to_owned();
        out.push(issue("stray_pack", "warn", vec![name.clone()], vec![title], to.into(), Some(Fix::Move { file: name, to: to.into() })));
    }
}

fn newer(a: &Jar, b: &Jar) -> Ordering {
    let v = |j: &Jar| j.m().and_then(|m| nums(&m.version));
    match (v(a), v(b)) {
        (Some(x), Some(y)) if cmp(&x, &y) != Ordering::Equal => cmp(&x, &y),
        _ => a.e.added.cmp(&b.e.added),
    }
}

/// Everything that can be found without the network.
pub fn scan(gd: &Path, loader: &str, mc: &str) -> Result<Scan, CoreError> {
    let list = mods::list(gd, Kind::Mod)?;
    let mut s = Scan::default();
    s.report.mods = list.len();
    s.report.enabled = list.iter().filter(|m| m.enabled).count();
    let mut out = Vec::new();
    stray_packs(gd, &mut out);
    if loader == "vanilla" {
        if s.report.enabled > 0 {
            out.push(issue("no_loader", "warn", vec![], vec![], s.report.enabled.to_string(), None));
        }
        s.report.issues = out;
        return Ok(s);
    }
    let dir = mods::dir(gd, Kind::Mod);
    let jars: Vec<Jar> = list
        .into_iter()
        .map(|e| {
            let name = if e.enabled { e.file.clone() } else { format!("{}{OFF}", e.file) };
            let meta = read_jar(&dir.join(name));
            Jar { e, meta }
        })
        .collect();
    let on: Vec<&Jar> = jars.iter().filter(|j| j.e.enabled).collect();
    let off: Vec<&Jar> = jars.iter().filter(|j| !j.e.enabled).collect();

    let broken: Vec<&Jar> = on.iter().copied().filter(|j| j.meta.is_err()).collect();
    if !broken.is_empty() {
        out.push(issue("broken", "error", files(&broken), titles(&broken), String::new(), Some(Fix::Disable { files: files(&broken) })));
    }

    let mcv = if !mc.is_empty() && mc.bytes().all(|b| b.is_ascii_digit() || b == b'.') { nums(mc) } else { None };
    let connector = on.iter().any(|j| j.answers("connector"));
    let ok = accepted(loader, mcv.as_deref(), connector);
    let mut wrong: HashSet<&str> = HashSet::new();
    let mut by_loader: BTreeMap<&str, Vec<&Jar>> = BTreeMap::new();
    for j in &on {
        let Some(m) = j.m() else { continue };
        if !m.loaders.is_empty() && !m.loaders.iter().any(|l| ok.contains(l)) {
            by_loader.entry(family(m.loaders[0])).or_default().push(j);
            wrong.insert(&j.e.file);
        }
    }
    for (l, js) in by_loader {
        out.push(issue("wrong_loader", "error", files(&js), titles(&js), l.into(), Some(Fix::Disable { files: files(&js) })));
    }
    if let Some(v) = &mcv {
        let bad: Vec<&Jar> = on
            .iter()
            .copied()
            .filter(|j| !wrong.contains(j.e.file.as_str()))
            .filter(|j| j.m().and_then(|m| m.mc.as_ref()).is_some_and(|r| r.fits(v) == Some(false)))
            .collect();
        if !bad.is_empty() {
            wrong.extend(bad.iter().map(|j| j.e.file.as_str()));
            out.push(issue("wrong_game", "error", files(&bad), titles(&bad), mc.into(), Some(Fix::Disable { files: files(&bad) })));
        }
    }
    let live: Vec<&Jar> = on.iter().copied().filter(|j| j.m().is_some() && !wrong.contains(j.e.file.as_str())).collect();

    for j in &on {
        s.present.insert(norm(&j.e.file.trim_end_matches(".jar").split(['-', '_', '+']).next().unwrap_or("")));
        s.present.insert(norm(&j.title()));
        if let Some(src) = &j.e.source {
            s.present.insert(norm(&src.slug));
        }
        if let Some(m) = j.m() {
            s.present.insert(norm(&m.id));
            s.present.extend(m.provides.iter().map(|p| norm(p)));
        }
    }
    s.present.remove("");

    // Two copies of one mod.
    let mut by_id: BTreeMap<&str, Vec<&Jar>> = BTreeMap::new();
    for j in &live {
        if let Some(m) = j.m().filter(|m| !m.id.is_empty()) {
            by_id.entry(&m.id).or_default().push(j);
        }
    }
    for (id, mut js) in by_id {
        if js.len() < 2 {
            continue;
        }
        js.sort_by(|a, b| newer(b, a));
        let old = files(&js[1..]);
        out.push(issue("duplicate", "error", files(&js), vec![js[0].title()], id.into(), Some(Fix::Disable { files: old })));
    }

    // Declared conflicts and OptiFine next to Sodium.
    let mut pairs: HashSet<(String, String)> = HashSet::new();
    for j in &live {
        for (bid, req) in &j.m().expect("live jars have metadata").breaks {
            for k in &on {
                if std::ptr::eq(*j, *k) || !k.answers(bid) {
                    continue;
                }
                let hit = req.anything() || k.m().and_then(|m| nums(&m.version)).and_then(|v| req.fits(&v)) == Some(true);
                let (a, b) = (j.e.file.clone(), k.e.file.clone());
                let key = if a < b { (a, b) } else { (b, a) };
                if hit && pairs.insert(key) {
                    out.push(issue("conflict", "error", vec![j.e.file.clone(), k.e.file.clone()], vec![j.title(), k.title()], String::new(), None));
                }
            }
        }
    }
    let opti: Vec<&Jar> = on.iter().copied().filter(|j| j.m().is_some_and(|m| m.optifine)).collect();
    if let (false, Some(r)) = (opti.is_empty(), live.iter().find(|j| RENDERERS.iter().any(|x| j.answers(x)))) {
        let mut names = titles(&opti);
        names.push(r.title());
        out.push(issue("optifine", "error", files(&opti), names, String::new(), Some(Fix::Disable { files: files(&opti) })));
    }

    // Dependencies.
    let mut dep_off: BTreeMap<Vec<String>, (String, Vec<String>)> = BTreeMap::new();
    let ids_ok = loader != "quilt";
    for j in &live {
        let m = j.m().expect("live jars have metadata");
        if !ids_ok || (m.loaders.iter().all(|l| *l == "fabric") && matches!(loader, "forge" | "neoforge")) {
            continue;
        }
        for d in &m.depends {
            if builtin(d) || on.iter().any(|k| k.answers(d)) {
                continue;
            }
            let ps: Vec<&Jar> = off.iter().copied().filter(|k| k.answers(d)).collect();
            if ps.is_empty() {
                push_unique(s.missing.entry(d.clone()).or_default(), j.title());
            } else {
                let e = dep_off.entry(files(&ps)).or_insert_with(|| (ps[0].title(), Vec::new()));
                push_unique(&mut e.1, j.title());
            }
        }
    }
    let on_pids: HashSet<&str> = on.iter().filter_map(|j| j.e.source.as_ref()).map(|s| s.project_id.as_str()).collect();
    for j in &live {
        let Some(src) = &j.e.source else { continue };
        for d in &src.deps {
            if on_pids.contains(d.as_str()) {
                continue;
            }
            let ps: Vec<&Jar> = off.iter().copied().filter(|k| k.e.source.as_ref().is_some_and(|s| &s.project_id == d)).collect();
            if ps.is_empty() {
                push_unique(s.wanted.entry(d.clone()).or_default(), j.title());
            } else {
                let e = dep_off.entry(files(&ps)).or_insert_with(|| (ps[0].title(), Vec::new()));
                push_unique(&mut e.1, j.title());
            }
        }
    }
    for (fs_, (title, need)) in dep_off {
        out.push(issue("dep_off", "error", fs_.clone(), need, title, Some(Fix::Enable { files: fs_ })));
    }
    s.report.issues = out;
    Ok(s)
}

/// Adds missing dependencies once catalog titles are known (`project id → (title, slug)`);
/// projects that could not be looked up are left out rather than guessed.
pub fn finish(s: Scan, projects: &HashMap<String, (String, String)>) -> Report {
    let mut r = s.report;
    let mut covered: HashSet<String> = HashSet::new();
    for (pid, need) in s.wanted {
        let Some((title, slug)) = projects.get(&pid) else { continue };
        let keys = [norm(slug), norm(title)];
        if keys.iter().any(|k| !k.is_empty() && s.present.contains(k)) {
            continue;
        }
        covered.extend(keys);
        r.issues.push(issue("missing_dep", "error", vec![], need, title.clone(), Some(Fix::Install { project: pid })));
    }
    for (id, need) in s.missing {
        if covered.contains(&norm(&id)) {
            continue;
        }
        r.issues.push(issue("missing_dep", "error", vec![], need, id.clone(), Some(Fix::Search { query: id })));
    }
    r.issues.sort_by_key(|i| u8::from(i.level != "error"));
    r
}

/// Applies one fix; returns how many files changed.
pub fn apply(gd: &Path, fix: &Fix) -> Result<usize, CoreError> {
    match fix {
        Fix::Disable { files } | Fix::Enable { files } => {
            let on = matches!(fix, Fix::Enable { .. });
            for f in files {
                mods::set_enabled(gd, Kind::Mod, f, on)?;
            }
            Ok(files.len())
        }
        Fix::Move { file, to } => {
            if !matches!(to.as_str(), "resourcepacks" | "shaderpacks") {
                return Err(CoreError::BadArgs(format!("bad folder: {to}")));
            }
            mods::check_file(Kind::ResourcePack, file)?;
            let from = mods::dir(gd, Kind::Mod).join(file);
            let dir = gd.join(to);
            fs::create_dir_all(&dir)?;
            let stem = file.trim_end_matches(".zip");
            let mut dest = dir.join(file);
            let mut n = 2;
            while dest.exists() {
                dest = dir.join(format!("{stem} ({n}).zip"));
                n += 1;
            }
            fs::rename(from, dest)?;
            Ok(1)
        }
        Fix::Install { .. } | Fix::Search { .. } => Err(CoreError::BadArgs("this fix runs in the interface".into())),
    }
}

// ---------- Safety check ----------

pub struct JarFile {
    pub file: String,
    pub name: String,
    pub path: PathBuf,
    pub sha1: String,
    /// Came from the CurseForge catalog (hash-checked on download).
    pub cf: bool,
}

pub fn jar_files(gd: &Path) -> Result<Vec<JarFile>, CoreError> {
    let dir = mods::dir(gd, Kind::Mod);
    let mut out = Vec::new();
    for e in mods::list(gd, Kind::Mod)? {
        let path = dir.join(if e.enabled { e.file.clone() } else { format!("{}{OFF}", e.file) });
        let Ok(b) = fs::read(&path) else { continue };
        let sha1 = Sha1::digest(&b).iter().map(|x| format!("{x:02x}")).collect();
        let cf = e.source.as_ref().is_some_and(|s| crate::curseforge::is_cf(&s.project_id));
        out.push(JarFile { name: e.title, file: e.file, path, sha1, cf });
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub file: String,
    pub name: String,
    pub why: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Verify {
    pub total: usize,
    /// Whether Modrinth answered; without it every jar is scanned in full.
    pub online: bool,
    /// Jars no catalog knows.
    pub unknown: Vec<Found>,
    pub suspects: Vec<Found>,
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// `neko` or `fractureiser` when the jar carries a known infection.
fn infected(p: &Path, deep: bool) -> Option<&'static str> {
    let f = fs::File::open(p).ok()?;
    let mut z = zip::ZipArchive::new(f).ok()?;
    if z.file_names().any(|n| n.starts_with("dev/neko/")) {
        return Some("neko");
    }
    if !deep {
        return None;
    }
    let mut budget: u64 = 256 << 20;
    let mut buf = Vec::new();
    for i in 0..z.len().min(30_000) {
        let Ok(e) = z.by_index(i) else { continue };
        if !e.name().ends_with(".class") || e.size() > 4 << 20 {
            continue;
        }
        if e.size() > budget {
            break;
        }
        budget -= e.size();
        buf.clear();
        if e.take(4 << 20).read_to_end(&mut buf).is_err() {
            continue;
        }
        if MARKERS.iter().any(|m| contains(&buf, m)) {
            return Some("fractureiser");
        }
    }
    None
}

/// `known` holds the sha1 of jars Modrinth recognised.
pub fn verify(jars: Vec<JarFile>, known: &HashSet<String>, online: bool) -> Verify {
    let mut v = Verify { total: jars.len(), online, unknown: Vec::new(), suspects: Vec::new() };
    for j in jars {
        let catalog = known.contains(&j.sha1) || j.cf;
        if let Some(why) = infected(&j.path, !catalog) {
            v.suspects.push(Found { file: j.file.clone(), name: j.name.clone(), why });
        }
        if !catalog && online {
            v.unknown.push(Found { file: j.file, name: j.name, why: "" });
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn v(s: &str) -> Vec<u64> {
        nums(s).unwrap()
    }

    #[test]
    fn fabric_predicates() {
        assert_eq!(fabric_ok(">=1.20 <1.21", &v("1.20.1")), Some(true));
        assert_eq!(fabric_ok(">= 1.20.2", &v("1.20.1")), Some(false));
        assert_eq!(fabric_ok("1.20.x", &v("1.20.4")), Some(true));
        assert_eq!(fabric_ok("1.20.x", &v("1.21")), Some(false));
        assert_eq!(fabric_ok("~1.20", &v("1.20.6")), Some(true));
        assert_eq!(fabric_ok("~1.20", &v("1.21")), Some(false));
        assert_eq!(fabric_ok("1.20.1", &v("1.20.1")), Some(true));
        assert_eq!(fabric_ok("1.19.4 || 1.20.1", &v("1.20.1")), Some(true));
        assert_eq!(fabric_ok("*", &v("1.8.9")), Some(true));
        assert_eq!(fabric_ok("23w14a", &v("1.20")), None);
        let r = Req { any: vec!["1.19.4".into(), "1.20.1".into()], maven: false };
        assert_eq!(r.fits(&v("1.20.1")), Some(true));
        assert_eq!(r.fits(&v("1.21")), Some(false));
    }

    #[test]
    fn maven_ranges() {
        assert_eq!(maven_ok("[1.20.1,1.21)", &v("1.20.1")), Some(true));
        assert_eq!(maven_ok("[1.20.1,1.21)", &v("1.21")), Some(false));
        assert_eq!(maven_ok("[1.19,1.20),[1.21,)", &v("1.21.1")), Some(true));
        assert_eq!(maven_ok("[1.19,1.20),[1.21,)", &v("1.20")), Some(false));
        assert_eq!(maven_ok("[1.20.1]", &v("1.20.1")), Some(true));
        assert_eq!(maven_ok("1.20.1", &v("1.18")), Some(true));
        assert_eq!(maven_ok("${mc_range}", &v("1.18")), Some(true));
    }

    #[test]
    fn reads_mods_toml() {
        let t = r#"
modLoader="javafml"
loaderVersion="[47,)"
[[mods]]
modId="create"
version="0.5.1"
displayName="Create" # name
description='''
A [[mods]] line = inside a description
'''
[[dependencies.create]]
    modId="forge"
    mandatory=true
    versionRange="[47,)"
[[dependencies.create]]
    modId="minecraft"
    type="required"
    versionRange="[1.20.1,1.20.2)"
[[dependencies.create]]
    modId="flywheel"
    mandatory=true
[[dependencies.create]]
    modId="optifine"
    type="incompatible"
"#;
        let mut m = Meta::default();
        toml(t, &mut m, false);
        assert_eq!(m.id, "create");
        assert_eq!(m.name, "Create");
        assert_eq!(m.loaders, vec!["forge"]);
        assert_eq!(m.depends, vec!["flywheel".to_string()]);
        assert_eq!(m.mc.unwrap().fits(&v("1.20.1")), Some(true));
        assert_eq!(m.breaks.len(), 1);
    }

    fn zip_of(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, b) in files {
            w.start_file(*n, o).unwrap();
            w.write_all(b).unwrap();
        }
        w.finish().unwrap();
    }

    fn fab(id: &str, ver: &str, extra: &str) -> Vec<u8> {
        format!(r#"{{"schemaVersion":1,"id":"{id}","version":"{ver}","name":"{id}"{extra}}}"#).into_bytes()
    }

    #[test]
    fn finds_and_fixes() {
        let gd = std::env::temp_dir().join(format!("fl-doc-{}", uuid::Uuid::new_v4()));
        let md = gd.join("mods");
        fs::create_dir_all(&md).unwrap();
        let j = |n: &str, files: &[(&str, &[u8])]| zip_of(&md.join(n), files);
        j("a.jar", &[("fabric.mod.json", &fab("a", "1.0", r#","depends":{"minecraft":">=1.20","b":"*","zzz":"*"}"#))]);
        j("b.jar.disabled", &[("fabric.mod.json", &fab("b", "1.0", ""))]);
        j("c.jar", &[("META-INF/mods.toml", b"[[mods]]\nmodId=\"c\"\n[[dependencies.c]]\nmodId=\"forge\"\nmandatory=true\n")]);
        j("d.jar", &[("fabric.mod.json", &fab("d", "1.0", r#","depends":{"minecraft":"1.21"}"#))]);
        j("e-1.jar", &[("fabric.mod.json", &fab("e", "1.0", ""))]);
        j("e-2.jar", &[("fabric.mod.json", &fab("e", "2.0", ""))]);
        j("opt.jar", &[("net/optifine/Config.class", b"x")]);
        j("sodium.jar", &[("fabric.mod.json", &fab("sodium", "0.5", r#","breaks":{"x":"*"}"#))]);
        j("x.jar", &[("fabric.mod.json", &fab("x", "1.0", ""))]);
        zip_of(&md.join("pack.zip"), &[("pack.mcmeta", b"{}"), ("assets/a.png", b"x")]);
        fs::write(md.join("bad.jar"), b"not a zip").unwrap();

        let s = scan(&gd, "fabric", "1.20.1").unwrap();
        let r = finish(s, &HashMap::new());
        let kind = |k: &str| r.issues.iter().filter(|i| i.kind == k).collect::<Vec<_>>();
        assert_eq!(kind("broken")[0].files, vec!["bad.jar"]);
        assert_eq!(kind("wrong_loader")[0].files, vec!["c.jar"]);
        assert_eq!(kind("wrong_loader")[0].detail, "forge");
        assert_eq!(kind("wrong_game")[0].files, vec!["d.jar"]);
        assert_eq!(kind("duplicate")[0].fix, Some(Fix::Disable { files: vec!["e-1.jar".into()] }));
        assert_eq!(kind("dep_off")[0].fix, Some(Fix::Enable { files: vec!["b.jar".into()] }));
        assert_eq!(kind("missing_dep")[0].detail, "zzz");
        assert_eq!(kind("conflict")[0].names, vec!["sodium", "x"]);
        assert_eq!(kind("optifine")[0].files, vec!["opt.jar"]);
        let pack = &kind("stray_pack")[0];
        assert_eq!(pack.detail, "resourcepacks");
        assert_eq!(r.issues.last().unwrap().level, "warn");

        for i in &r.issues {
            if let Some(f @ (Fix::Disable { .. } | Fix::Enable { .. } | Fix::Move { .. })) = &i.fix {
                apply(&gd, f).unwrap();
            }
        }
        assert!(gd.join("resourcepacks/pack.zip").is_file());
        assert!(md.join("b.jar").is_file());
        assert!(md.join("e-1.jar.disabled").is_file());
        let left = finish(scan(&gd, "fabric", "1.20.1").unwrap(), &HashMap::new());
        let kinds: Vec<_> = left.issues.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, vec!["conflict", "missing_dep"]);
        assert!(apply(&gd, &Fix::Move { file: "../x.zip".into(), to: "resourcepacks".into() }).is_err());
        fs::remove_dir_all(&gd).ok();
    }

    #[test]
    fn nested_jars_provide_ids() {
        let gd = std::env::temp_dir().join(format!("fl-doc-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(gd.join("mods")).unwrap();
        let inner = gd.join("inner.jar");
        zip_of(&inner, &[("fabric.mod.json", &fab("fabric-api-base", "1.0", ""))]);
        let ib = fs::read(&inner).unwrap();
        zip_of(&gd.join("mods/api.jar"), &[("fabric.mod.json", &fab("fabric-api", "1.0", r#","provides":["fabric"]"#)), ("META-INF/jars/base.jar", &ib)]);
        zip_of(&gd.join("mods/m.jar"), &[("fabric.mod.json", &fab("m", "1.0", r#","depends":{"fabric-api-base":"*","fabric":"*"}"#))]);
        let r = finish(scan(&gd, "fabric", "1.20.1").unwrap(), &HashMap::new());
        assert!(r.issues.is_empty(), "{:?}", r.issues);
        let jars = jar_files(&gd).unwrap();
        let v = verify(jars, &HashSet::new(), true);
        assert_eq!(v.unknown.len(), 2);
        assert!(v.suspects.is_empty());
        fs::remove_dir_all(&gd).ok();
    }
}