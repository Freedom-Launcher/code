//! Network access for addons (M3): the `net.fetch` action. An addon iframe has
//! no network of its own; the core makes the request for it, only over https,
//! only to hosts its manifest declares, never to local or private addresses.

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, OnceLock},
    time::Duration,
};

use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    redirect::Policy,
    Method, Url,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{actions::ActionRegistry, addons, Caller, CoreError, Permission};

/// Set by the core on `net.*` calls: which addon is calling. Never from args.
pub const CALLER_KEY: &str = "$addon";
pub const MAX_REQUEST: usize = 1 << 20;
pub const MAX_RESPONSE: usize = 4 << 20;
const MAX_HEADERS: usize = 32;
const BLOCKED: [&str; 14] = [
    "host",
    "cookie",
    "connection",
    "content-length",
    "transfer-encoding",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
    "expect",
    "origin",
    "referer",
    "user-agent",
    "forwarded",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Req {
    #[serde(rename = "$addon")]
    addon: Option<String>,
    url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: Option<String>,
}

pub(crate) fn stamp_caller(caller: &Caller, action: &str, mut args: Value) -> Value {
    if action.starts_with("net.") {
        if let Value::Object(m) = &mut args {
            m.remove(CALLER_KEY);
            if let Caller::Addon { id, .. } = caller {
                m.insert(CALLER_KEY.into(), id.clone().into());
            }
        }
    }
    args
}

/// Public internet addresses only: no loopback, LAN, link-local, CGNAT, etc.
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_documentation()
                || v.is_multicast()
                || o[0] == 0
                || o[0] >= 240
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
                || (o[0] == 198 && (o[1] & 0xfe) == 18))
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = v.segments();
            if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [a, b] = [s[6].to_be_bytes(), s[7].to_be_bytes()];
                return public_ip(IpAddr::from([a[0], a[1], b[0], b[1]]));
            }
            !(v.is_loopback()
                || v.is_unspecified()
                || v.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
                || (s[0] == 0x2001 && s[1] == 0x0db8))
        }
    }
}

struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            if addrs.is_empty() {
                return Err("хост не найден".into());
            }
            if addrs.iter().any(|a| !public_ip(a.ip())) {
                return Err("локальные и частные адреса недоступны дополнениям".into());
            }
            let it: Addrs = Box::new(addrs.into_iter());
            Ok(it)
        })
    }
}

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!(
                "FreedomLauncher/",
                env!("CARGO_PKG_VERSION"),
                " (addon)"
            ))
            .redirect(Policy::none())
            .no_proxy()
            .https_only(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .dns_resolver(Arc::new(PublicOnly))
            .build()
            .expect("failed to build addon HTTP client")
    })
}

/// https, default port, no credentials, host exactly one of `hosts`.
pub fn check_url(url: &str, hosts: &[String]) -> Result<Url, String> {
    let mut u = Url::parse(url).map_err(|_| "url: неверный адрес".to_string())?;
    if u.scheme() != "https" {
        return Err("только https".into());
    }
    if !u.username().is_empty() || u.password().is_some() || u.port().is_some() {
        return Err("url: без логина, пароля и нестандартного порта".into());
    }
    let h = u.host_str().unwrap_or_default();
    if h.starts_with('[') || h.parse::<IpAddr>().is_ok() {
        return Err("url: нужно имя хоста, а не IP".into());
    }
    if !hosts.iter().any(|x| x == h) {
        return Err(format!("хост {h} не указан в hosts дополнения"));
    }
    u.set_fragment(None);
    Ok(u)
}

pub fn blocked_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    BLOCKED.contains(&n.as_str())
        || ["proxy-", "sec-", "x-forwarded-"]
            .iter()
            .any(|p| n.starts_with(p))
}

fn chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        s = format!("{s}: {c}");
        cur = c.source();
    }
    s
}

async fn fetch(hosts: &[String], req: Req) -> Result<Value, CoreError> {
    let url = check_url(&req.url, hosts).map_err(CoreError::Denied)?;
    let method = match req
        .method
        .as_deref()
        .unwrap_or("GET")
        .to_ascii_uppercase()
        .as_str()
    {
        "GET" => Method::GET,
        "HEAD" => Method::HEAD,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "PATCH" => Method::PATCH,
        "DELETE" => Method::DELETE,
        m => return Err(CoreError::BadArgs(format!("метод {m} не поддерживается"))),
    };
    if req.headers.len() > MAX_HEADERS {
        return Err(CoreError::BadArgs("слишком много заголовков".into()));
    }
    let mut rb = client().request(method, url);
    for (k, v) in &req.headers {
        if blocked_header(k) {
            return Err(CoreError::BadArgs(format!("заголовок {k} задаёт лаунчер")));
        }
        rb = rb.header(k.as_str(), v.as_str());
    }
    if let Some(b) = req.body {
        if b.len() > MAX_REQUEST {
            return Err(CoreError::BadArgs("тело запроса больше 1 МБ".into()));
        }
        rb = rb.body(b);
    }
    let net = |e: reqwest::Error| CoreError::msg(chain(&e));
    let mut resp = rb.send().await.map_err(net)?;
    let status = resp.status().as_u16();
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in resp.headers() {
        let (Ok(v), false) = (v.to_str(), k == reqwest::header::SET_COOKIE) else {
            continue;
        };
        headers
            .entry(k.as_str().into())
            .and_modify(|e| *e = format!("{e}, {v}"))
            .or_insert(v.into());
    }
    let too_big = || CoreError::msg("ответ больше 4 МБ");
    if resp
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE as u64)
    {
        return Err(too_big());
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(net)? {
        if body.len() + chunk.len() > MAX_RESPONSE {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(json!({
        "status": status, "ok": (200..300).contains(&status),
        "headers": headers, "body": String::from_utf8_lossy(&body),
    }))
}

pub(crate) fn register(r: &mut ActionRegistry) {
    r.register("net.fetch", Permission::Network, false, |c, a| async move {
        let req: Req = serde_json::from_value(a).map_err(|e| CoreError::BadArgs(e.to_string()))?;
        let id = req
            .addon
            .clone()
            .ok_or_else(|| CoreError::Denied("net.fetch — только для дополнений".into()))?;
        let m = addons::load(&c.data_dir, &id).map_err(CoreError::Msg)?;
        fetch(&m.hosts, req).await
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let h = vec!["api.example.org".to_string()];
        assert!(check_url("https://api.example.org/v1?q=1#x", &h).is_ok());
        for bad in [
            "http://api.example.org/",
            "https://api.example.org:8443/",
            "https://u:p@api.example.org/",
            "https://evil.example/",
            "https://api.example.org.evil.example/",
            "https://127.0.0.1/",
            "https://[::1]/",
            "file:///etc/passwd",
            "https://2130706433/",
        ] {
            assert!(check_url(bad, &h).is_err(), "{bad}");
        }
    }

    #[test]
    fn addresses() {
        for ok in ["1.1.1.1", "151.101.1.1", "2606:4700::1111"] {
            assert!(public_ip(ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fd00::1",
            "::ffff:127.0.0.1",
            "64:ff9b::a00:1",
        ] {
            assert!(!public_ip(bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn headers() {
        assert!(
            blocked_header("Cookie")
                && blocked_header("Proxy-Authorization")
                && blocked_header("sec-fetch-mode")
        );
        assert!(!blocked_header("Authorization") && !blocked_header("Accept"));
    }
}
