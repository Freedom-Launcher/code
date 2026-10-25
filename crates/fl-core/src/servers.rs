//! The game's own multiplayer list (`servers.dat`, uncompressed NBT) and the
//! Server List Ping that shows a server's MOTD, players and latency.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::CoreError;

// ---------- NBT ----------

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    /// Element type id and the elements.
    List(u8, Vec<Tag>),
    /// Kept in file order, so a rewrite changes nothing it does not mean to.
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

const MAX_DEPTH: usize = 512;

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::ByteArray(_) => 7,
            Tag::String(_) => 8,
            Tag::List(..) => 9,
            Tag::Compound(_) => 10,
            Tag::IntArray(_) => 11,
            Tag::LongArray(_) => 12,
        }
    }
    fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(v) => v.iter().find(|(k, _)| k == key).map(|(_, t)| t),
            _ => None,
        }
    }
    fn str(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Tag::String(s)) => Some(s),
            _ => None,
        }
    }
    fn byte(&self, key: &str) -> Option<i8> {
        match self.get(key) {
            Some(Tag::Byte(b)) => Some(*b),
            _ => None,
        }
    }
    fn set(&mut self, key: &str, val: Tag) {
        if let Tag::Compound(v) = self {
            match v.iter_mut().find(|(k, _)| k == key) {
                Some(e) => e.1 = val,
                None => v.push((key.to_owned(), val)),
            }
        }
    }
    fn remove(&mut self, key: &str) {
        if let Tag::Compound(v) = self {
            v.retain(|(k, _)| k != key);
        }
    }
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

type R<T> = Result<T, String>;

impl<'a> Rd<'a> {
    fn left(&self) -> usize {
        self.b.len() - self.p
    }
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if n > self.left() {
            return Err("unexpected end".into());
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn arr<const N: usize>(&mut self) -> R<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn len(&mut self, unit: usize) -> R<usize> {
        let n = i32::from_be_bytes(self.arr()?);
        if n < 0 || (n as usize).saturating_mul(unit) > self.left() {
            return Err("bad length".into());
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> R<String> {
        let n = u16::from_be_bytes(self.arr()?) as usize;
        Ok(mutf8_decode(self.take(n)?))
    }
    fn payload(&mut self, id: u8, depth: usize) -> R<Tag> {
        if depth > MAX_DEPTH {
            return Err("too deep".into());
        }
        Ok(match id {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(i16::from_be_bytes(self.arr()?)),
            3 => Tag::Int(i32::from_be_bytes(self.arr()?)),
            4 => Tag::Long(i64::from_be_bytes(self.arr()?)),
            5 => Tag::Float(f32::from_be_bytes(self.arr()?)),
            6 => Tag::Double(f64::from_be_bytes(self.arr()?)),
            7 => {
                let n = self.len(1)?;
                Tag::ByteArray(self.take(n)?.iter().map(|b| *b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let el = self.u8()?;
                let n = self.len(1)?;
                if el == 0 && n > 0 || el > 12 {
                    return Err("bad list".into());
                }
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.payload(el, depth + 1)?);
                }
                Tag::List(el, v)
            }
            10 => {
                let mut v = Vec::new();
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        break;
                    }
                    let k = self.string()?;
                    v.push((k, self.payload(t, depth + 1)?));
                }
                Tag::Compound(v)
            }
            11 => {
                let n = self.len(4)?;
                Tag::IntArray((0..n).map(|_| i32::from_be_bytes(self.arr().unwrap())).collect())
            }
            12 => {
                let n = self.len(8)?;
                Tag::LongArray((0..n).map(|_| i64::from_be_bytes(self.arr().unwrap())).collect())
            }
            _ => return Err(format!("bad tag {id}")),
        })
    }
}

/// Reads a named root tag; returns the name and the tag.
pub fn read_nbt(b: &[u8]) -> R<(String, Tag)> {
    let mut r = Rd { b, p: 0 };
    let id = r.u8()?;
    if id != 10 {
        return Err("root is not a compound".into());
    }
    let name = r.string()?;
    Ok((name, r.payload(10, 0)?))
}

pub fn write_nbt(name: &str, t: &Tag) -> Vec<u8> {
    let mut o = vec![t.id()];
    put_mutf8(&mut o, name);
    put_payload(&mut o, t);
    o
}

fn put_payload(o: &mut Vec<u8>, t: &Tag) {
    match t {
        Tag::Byte(v) => o.push(*v as u8),
        Tag::Short(v) => o.extend(v.to_be_bytes()),
        Tag::Int(v) => o.extend(v.to_be_bytes()),
        Tag::Long(v) => o.extend(v.to_be_bytes()),
        Tag::Float(v) => o.extend(v.to_be_bytes()),
        Tag::Double(v) => o.extend(v.to_be_bytes()),
        Tag::ByteArray(v) => {
            o.extend((v.len() as i32).to_be_bytes());
            o.extend(v.iter().map(|b| *b as u8));
        }
        Tag::String(s) => put_mutf8(o, s),
        Tag::List(el, v) => {
            o.push(v.first().map(Tag::id).unwrap_or(*el));
            o.extend((v.len() as i32).to_be_bytes());
            for x in v {
                put_payload(o, x);
            }
        }
        Tag::Compound(v) => {
            for (k, x) in v {
                o.push(x.id());
                put_mutf8(o, k);
                put_payload(o, x);
            }
            o.push(0);
        }
        Tag::IntArray(v) => {
            o.extend((v.len() as i32).to_be_bytes());
            for x in v {
                o.extend(x.to_be_bytes());
            }
        }
        Tag::LongArray(v) => {
            o.extend((v.len() as i32).to_be_bytes());
            for x in v {
                o.extend(x.to_be_bytes());
            }
        }
    }
}



/// Java's "modified UTF-8": NUL as C0 80, characters outside the BMP as two 3-byte surrogates.
fn mutf8_decode(b: &[u8]) -> String {
    let mut u = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c < 0x80 {
            u.push(c as u16);
            i += 1;
        } else if c & 0xe0 == 0xc0 && i + 1 < b.len() {
            u.push(((c as u16 & 0x1f) << 6) | (b[i + 1] as u16 & 0x3f));
            i += 2;
        } else if c & 0xf0 == 0xe0 && i + 2 < b.len() {
            u.push(((c as u16 & 0x0f) << 12) | ((b[i + 1] as u16 & 0x3f) << 6) | (b[i + 2] as u16 & 0x3f));
            i += 3;
        } else {
            u.push(0xfffd);
            i += 1;
        }
    }
    String::from_utf16_lossy(&u)
}

fn put_mutf8(o: &mut Vec<u8>, s: &str) {
    let mut b = Vec::with_capacity(s.len());
    for c in s.encode_utf16() {
        match c {
            0x0001..=0x007f => b.push(c as u8),
            0 | 0x0080..=0x07ff => {
                b.push(0xc0 | (c >> 6) as u8);
                b.push(0x80 | (c & 0x3f) as u8);
            }
            _ => {
                b.push(0xe0 | (c >> 12) as u8);
                b.push(0x80 | ((c >> 6) & 0x3f) as u8);
                b.push(0x80 | (c & 0x3f) as u8);
            }
        }
    }
    b.truncate(u16::MAX as usize);
    o.extend((b.len() as u16).to_be_bytes());
    o.extend(b);
}

// ---------- servers.dat ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Server {
    /// Position in the file's list, hidden entries included.
    pub index: usize,
    pub name: String,
    pub ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept_textures: Option<bool>,
}

fn file(gd: &Path) -> PathBuf {
    gd.join("servers.dat")
}

fn unreadable() -> CoreError {
    CoreError::msg("Не удалось прочитать список серверов")
}

pub fn read_root(gd: &Path) -> Result<(String, Tag), CoreError> {
    let b = match std::fs::read(file(gd)) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    if b.is_empty() {
        return Ok((String::new(), Tag::Compound(vec![])));
    }
    read_nbt(&b).map_err(|_| unreadable())
}

pub fn write_root(gd: &Path, name: &str, root: &Tag) -> Result<(), CoreError> {
    std::fs::create_dir_all(gd)?;
    let f = file(gd);
    if f.is_file() {
        let _ = std::fs::copy(&f, gd.join("servers.dat_old"));
    }
    let tmp = gd.join("servers.dat.fl-tmp");
    std::fs::write(&tmp, write_nbt(name, root))?;
    std::fs::rename(&tmp, &f)?;
    Ok(())
}

fn entries(root: &Tag) -> &[Tag] {
    match root.get("servers") {
        Some(Tag::List(_, v)) => v,
        _ => &[],
    }
}

fn entries_mut(root: &mut Tag) -> &mut Vec<Tag> {
    if !matches!(root.get("servers"), Some(Tag::List(..))) {
        root.set("servers", Tag::List(10, vec![]));
    }
    match root {
        Tag::Compound(v) => match v.iter_mut().find(|(k, _)| k == "servers") {
            Some((_, Tag::List(_, l))) => l,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

fn to_server(index: usize, t: &Tag) -> Server {
    Server {
        index,
        name: t.str("name").unwrap_or_default().to_owned(),
        ip: t.str("ip").unwrap_or_default().to_owned(),
        icon: t.str("icon").filter(|s| !s.is_empty()).map(str::to_owned),
        accept_textures: t.byte("acceptTextures").map(|b| b != 0),
    }
}

/// The servers the game shows (hidden LAN-preview entries are skipped, but kept on write).
pub fn list(gd: &Path) -> Result<Vec<Server>, CoreError> {
    let (_, root) = read_root(gd)?;
    Ok(entries(&root)
        .iter()
        .enumerate()
        .filter(|(_, t)| t.byte("hidden").unwrap_or(0) == 0)
        .map(|(i, t)| to_server(i, t))
        .collect())
}

/// Allowed in the game's address field: a host or IP with an optional port.
pub fn valid_address(a: &str) -> bool {
    !a.is_empty()
        && a.len() <= 255
        && !a.starts_with('-')
        && !a.chars().any(|c| c.is_whitespace() || c.is_control() || c == '/' || c == '\\')
        && split(a).is_some()
}

/// `host`, `host:port`, `[v6]`, `[v6]:port`, or a bare IPv6 address.
pub fn split(a: &str) -> Option<(String, Option<u16>)> {
    let port = |p: &str| p.parse::<u16>().ok().filter(|p| *p != 0);
    if let Some(rest) = a.strip_prefix('[') {
        let (h, tail) = rest.split_once(']')?;
        if h.is_empty() {
            return None;
        }
        return match tail {
            "" => Some((h.to_owned(), None)),
            t => Some((h.to_owned(), Some(port(t.strip_prefix(':')?)?))),
        };
    }
    match a.matches(':').count() {
        0 => (!a.is_empty()).then(|| (a.to_owned(), None)),
        1 => {
            let (h, p) = a.split_once(':')?;
            (!h.is_empty()).then_some(())?;
            Some((h.to_owned(), Some(port(p)?)))
        }
        _ => a.parse::<std::net::Ipv6Addr>().ok().map(|_| (a.to_owned(), None)),
    }
}

pub fn check(name: &str, ip: &str) -> Result<(String, String), CoreError> {
    let ip = ip.trim();
    if ip.is_empty() {
        return Err(CoreError::msg("Укажите адрес сервера"));
    }
    if !valid_address(ip) {
        return Err(CoreError::msg("Неверный адрес сервера"));
    }
    let name = name.trim();
    let name: String = if name.is_empty() { "Сервер Minecraft".into() } else { name.chars().take(64).collect() };
    Ok((name, ip.to_owned()))
}

fn stale() -> CoreError {
    CoreError::msg("Список серверов изменился — обновите его")
}

/// Adds a server (`index` None) or edits the one at `index`, which must still have `ip_was`.
pub fn save(gd: &Path, index: Option<usize>, ip_was: Option<&str>, name: &str, ip: &str) -> Result<Server, CoreError> {
    let (name, ip) = check(name, ip)?;
    let (root_name, mut root) = read_root(gd)?;
    let list = entries_mut(&mut root);
    let i = match index {
        Some(i) => {
            let e = list.get_mut(i).ok_or_else(stale)?;
            let old = e.str("ip").unwrap_or_default().to_owned();
            if ip_was.is_some_and(|w| w != old) {
                return Err(stale());
            }
            e.set("name", Tag::String(name));
            if old != ip {
                e.set("ip", Tag::String(ip));
                e.remove("icon");
            }
            i
        }
        None => {
            list.push(Tag::Compound(vec![
                ("name".into(), Tag::String(name)),
                ("ip".into(), Tag::String(ip)),
            ]));
            list.len() - 1
        }
    };
    let out = to_server(i, &list[i]);
    write_root(gd, &root_name, &root)?;
    Ok(out)
}

pub fn remove(gd: &Path, index: usize, ip: &str) -> Result<(), CoreError> {
    let (root_name, mut root) = read_root(gd)?;
    let list = entries_mut(&mut root);
    match list.get(index) {
        Some(e) if e.str("ip").unwrap_or_default() == ip => {
            list.remove(index);
        }
        _ => return Err(stale()),
    }
    write_root(gd, &root_name, &root)
}

// ---------- Server List Ping ----------

const MAX_STATUS: usize = 2 << 20;

pub fn varint(v: i32, o: &mut Vec<u8>) {
    let mut u = v as u32;
    loop {
        let b = (u & 0x7f) as u8;
        u >>= 7;
        if u == 0 {
            o.push(b);
            return;
        }
        o.push(b | 0x80);
    }
}

fn slice_varint(b: &[u8], p: &mut usize) -> Option<i32> {
    let mut v: u32 = 0;
    for i in 0..5 {
        let c = *b.get(*p)?;
        *p += 1;
        v |= ((c & 0x7f) as u32) << (7 * i);
        if c & 0x80 == 0 {
            return Some(v as i32);
        }
    }
    None
}

async fn read_varint<S: AsyncRead + Unpin>(s: &mut S) -> io::Result<i32> {
    let mut v: u32 = 0;
    for i in 0..5 {
        let c = s.read_u8().await?;
        v |= ((c & 0x7f) as u32) << (7 * i);
        if c & 0x80 == 0 {
            return Ok(v as i32);
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "varint"))
}

pub fn packet(id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(payload.len() + 5);
    varint(id, &mut body);
    body.extend_from_slice(payload);
    let mut o = Vec::with_capacity(body.len() + 5);
    varint(body.len() as i32, &mut o);
    o.extend(body);
    o
}

async fn read_packet<S: AsyncRead + Unpin>(s: &mut S) -> io::Result<Vec<u8>> {
    let n = read_varint(s).await?;
    if n <= 0 || n as usize > MAX_STATUS {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "length"));
    }
    let mut b = vec![0; n as usize];
    s.read_exact(&mut b).await?;
    Ok(b)
}

enum Fail {
    Connect,
    Bad,
}

impl From<io::Error> for Fail {
    fn from(_: io::Error) -> Self {
        Fail::Bad
    }
}

/// `_minecraft._tcp` SRV lookup over DNS-over-HTTPS; None when there is no record.
async fn srv(http: &reqwest::Client, host: &str) -> Option<(String, u16)> {
    let url = reqwest::Url::parse_with_params(
        "https://cloudflare-dns.com/dns-query",
        &[("name", format!("_minecraft._tcp.{host}")), ("type", "SRV".into())],
    )
    .ok()?;
    let req = http.get(url).header("accept", "application/dns-json").send();
    let v: Value = tokio::time::timeout(Duration::from_secs(3), async { req.await.ok()?.json().await.ok() })
        .await
        .ok()??;
    v["Answer"]
        .as_array()?
        .iter()
        .filter(|a| a["type"] == 33)
        .filter_map(|a| {
            let d: Vec<&str> = a["data"].as_str()?.split_whitespace().collect();
            let (prio, port, target) = (d.first()?.parse::<u32>().ok()?, d.get(2)?.parse::<u16>().ok()?, d.get(3)?);
            let target = target.trim_end_matches('.');
            (!target.is_empty() && port != 0).then(|| (prio, target.to_owned(), port))
        })
        .min_by_key(|x| x.0)
        .map(|(_, t, p)| (t, p))
}

async fn status(host: &str, port: u16) -> Result<(Value, u64), Fail> {
    let mut s = TcpStream::connect((host, port)).await.map_err(|_| Fail::Connect)?;
    let _ = s.set_nodelay(true);
    let mut hs = Vec::new();
    varint(-1, &mut hs);
    let hb = host.as_bytes();
    varint(hb.len() as i32, &mut hs);
    hs.extend_from_slice(hb);
    hs.extend(port.to_be_bytes());
    varint(1, &mut hs);
    let mut out = packet(0, &hs);
    out.extend(packet(0, &[]));
    s.write_all(&out).await?;
    let t0 = Instant::now();
    let b = read_packet(&mut s).await?;
    let status_ms = t0.elapsed().as_millis() as u64;
    let mut p = 0;
    if slice_varint(&b, &mut p) != Some(0) {
        return Err(Fail::Bad);
    }
    let n = slice_varint(&b, &mut p).ok_or(Fail::Bad)?;
    let js = b.get(p..p + n.max(0) as usize).ok_or(Fail::Bad)?;
    let v: Value = serde_json::from_slice(js).map_err(|_| Fail::Bad)?;
    // Latency from ping/pong; servers that hang up after the status keep the status round trip.
    let pong = async {
        let t1 = Instant::now();
        s.write_all(&packet(1, &42i64.to_be_bytes())).await.ok()?;
        let r = read_packet(&mut s).await.ok()?;
        (r.first() == Some(&1)).then(|| t1.elapsed().as_millis() as u64)
    };
    let ms = tokio::time::timeout(Duration::from_millis(1500), pong).await.ok().flatten().unwrap_or(status_ms);
    Ok((v, ms))
}

fn shape(v: &Value, ms: u64) -> Value {
    let sample: Vec<Value> = v["players"]["sample"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p["name"].as_str()).take(12).map(Value::from).collect())
        .unwrap_or_default();
    let fav = v["favicon"].as_str().filter(|f| f.len() < 200_000 && f.starts_with("data:image/"));
    json!({
        "online": true,
        "latency_ms": ms,
        "version": v["version"]["name"].as_str(),
        "protocol": v["version"]["protocol"].as_i64(),
        "players": { "online": v["players"]["online"].as_i64(), "max": v["players"]["max"].as_i64(), "sample": sample },
        "motd": v.get("description").cloned().unwrap_or(Value::Null),
        "favicon": fav,
    })
}

/// Pings a server like the multiplayer screen does. Never errors: offline is `{online:false, error}`.
pub async fn ping(http: &reqwest::Client, addr: &str) -> Value {
    let addr = addr.trim();
    let Some((host, port)) = split(addr).filter(|_| valid_address(addr)) else {
        return json!({ "online": false, "error": "Неверный адрес сервера" });
    };
    let work = async {
        let (host, port) = match port {
            Some(p) => (host, p),
            None if host.parse::<std::net::IpAddr>().is_err() && host.contains('.') => {
                srv(http, &host).await.unwrap_or((host, 25565))
            }
            None => (host, 25565),
        };
        status(&host, port).await
    };
    match tokio::time::timeout(Duration::from_secs(6), work).await {
        Ok(Ok((v, ms))) => shape(&v, ms),
        Ok(Err(Fail::Connect)) => json!({ "online": false, "error": "Не удалось подключиться к серверу" }),
        Ok(Err(Fail::Bad)) => json!({ "online": false, "error": "Сервер ответил непонятно" }),
        Err(_) => json!({ "online": false, "error": "Сервер не отвечает" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-servers-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn nbt_roundtrip() {
        let t = Tag::Compound(vec![
            ("servers".into(), Tag::List(10, vec![Tag::Compound(vec![
                ("name".into(), Tag::String("Привет \u{0} 𝄞".into())),
                ("ip".into(), Tag::String("mc.example.com".into())),
                ("hidden".into(), Tag::Byte(0)),
            ])])),
            ("n".into(), Tag::LongArray(vec![1, -2])),
            ("e".into(), Tag::List(0, vec![])),
            ("d".into(), Tag::Double(1.5)),
        ]);
        let b = write_nbt("", &t);
        assert_eq!(read_nbt(&b).unwrap(), (String::new(), t));
        assert!(read_nbt(&b[..b.len() - 1]).is_err());
        // NUL is C0 80 in modified UTF-8.
        let mut o = Vec::new();
        put_mutf8(&mut o, "\u{0}");
        assert_eq!(o, [0, 2, 0xc0, 0x80]);
    }

    #[test]
    fn deep_nesting_is_refused() {
        let mut b = vec![10, 0, 0];
        for _ in 0..600 {
            b.extend([10, 0, 1, b'a']);
        }
        assert!(read_nbt(&b).is_err());
    }

    #[test]
    fn list_save_remove() {
        let d = tmp();
        assert!(list(&d).unwrap().is_empty());
        save(&d, None, None, "", "play.example.org").unwrap();
        save(&d, None, None, "Two", "1.2.3.4:25570").unwrap();
        // A hidden entry the game keeps survives edits but is not listed.
        let (n, mut root) = read_root(&d).unwrap();
        entries_mut(&mut root).push(Tag::Compound(vec![
            ("name".into(), Tag::String("lan".into())),
            ("ip".into(), Tag::String("10.0.0.2".into())),
            ("hidden".into(), Tag::Byte(1)),
        ]));
        entries_mut(&mut root)[0].set("icon", Tag::String("AAAA".into()));
        write_root(&d, &n, &root).unwrap();
        let l = list(&d).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "Сервер Minecraft");
        assert_eq!(l[0].icon.as_deref(), Some("AAAA"));
        // Renaming keeps the icon, a new address drops it.
        let s = save(&d, Some(0), Some("play.example.org"), "One", "play.example.org").unwrap();
        assert_eq!(s.icon.as_deref(), Some("AAAA"));
        let s = save(&d, Some(0), Some("play.example.org"), "One", "other.example.org").unwrap();
        assert_eq!(s.icon, None);
        assert!(save(&d, Some(0), Some("play.example.org"), "x", "y.org").is_err());
        assert!(remove(&d, 1, "nope").is_err());
        remove(&d, 1, "1.2.3.4:25570").unwrap();
        let (_, root) = read_root(&d).unwrap();
        assert_eq!(entries(&root).len(), 2);
        assert_eq!(list(&d).unwrap().len(), 1);
        assert!(d.join("servers.dat_old").is_file());
        assert!(save(&d, None, None, "", "  ").is_err());
        assert!(save(&d, None, None, "", "a b").is_err());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn addresses() {
        assert_eq!(split("a.b"), Some(("a.b".into(), None)));
        assert_eq!(split("a.b:25570"), Some(("a.b".into(), Some(25570))));
        assert_eq!(split("[::1]:5"), Some(("::1".into(), Some(5))));
        assert_eq!(split("[::1]"), Some(("::1".into(), None)));
        assert_eq!(split("::1"), Some(("::1".into(), None)));
        assert_eq!(split("a:0"), None);
        assert_eq!(split("a:x"), None);
        assert!(!valid_address("-oProxy"));
        assert!(!valid_address("a/b"));
        assert!(valid_address("mc.hypixel.net"));
    }

    #[test]
    fn varint_minus_one() {
        let mut o = Vec::new();
        varint(-1, &mut o);
        assert_eq!(o, [0xff, 0xff, 0xff, 0xff, 0x0f]);
        let mut p = 0;
        assert_eq!(slice_varint(&o, &mut p), Some(-1));
    }

    #[tokio::test]
    async fn ping_fake_server() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let hs = read_packet(&mut s).await.unwrap();
            assert_eq!(hs[0], 0);
            let req = read_packet(&mut s).await.unwrap();
            assert_eq!(req, [0]);
            let js = br#"{"version":{"name":"1.21.4","protocol":769},"players":{"max":20,"online":3,"sample":[{"name":"Alex","id":"x"}]},"description":{"text":"Hi"},"favicon":"data:image/png;base64,AA=="}"#;
            let mut body = Vec::new();
            varint(js.len() as i32, &mut body);
            body.extend_from_slice(js);
            s.write_all(&packet(0, &body)).await.unwrap();
            let p = read_packet(&mut s).await.unwrap();
            s.write_all(&packet(1, &p[1..])).await.unwrap();
        });
        let v = ping(&reqwest::Client::new(), &format!("127.0.0.1:{port}")).await;
        assert_eq!(v["online"], true);
        assert_eq!(v["version"], "1.21.4");
        assert_eq!(v["players"]["online"], 3);
        assert_eq!(v["players"]["sample"][0], "Alex");
        assert_eq!(v["motd"]["text"], "Hi");
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cp = closed.local_addr().unwrap().port();
        drop(closed);
        let v = ping(&reqwest::Client::new(), &format!("127.0.0.1:{cp}")).await;
        assert_eq!(v["online"], false);
        assert_eq!(ping(&reqwest::Client::new(), "bad addr").await["online"], false);
    }
}