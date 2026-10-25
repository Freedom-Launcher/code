//! Remote images in project descriptions (badges, screenshots from GitHub, imgur,
//! shields.io…). The WebView may load images only from allowlisted protocols, so
//! the core fetches them: https only, public addresses only (every redirect is
//! re-resolved), at most 8 MB, and only real image bytes. Results are cached on disk.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};

use reqwest::redirect::Policy;
use sha1::{Digest, Sha1};

pub const MAX_IMAGE: usize = 8 << 20;

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("FreedomLauncher/", env!("FL_VERSION")))
            .redirect(Policy::custom(|a| {
                if a.previous().len() >= 5 || a.url().scheme() != "https" {
                    a.stop()
                } else {
                    a.follow()
                }
            }))
            .no_proxy()
            .https_only(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .dns_resolver(Arc::new(crate::net::PublicOnly))
            .build()
            .expect("failed to build image HTTP client")
    })
}

/// The MIME type of image bytes, or `None` if they are not an image we show.
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if b.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("image/webp")
    } else if b.len() > 12 && &b[4..8] == b"ftyp" && (&b[8..12] == b"avif" || &b[8..12] == b"avis") {
        Some("image/avif")
    } else if b.starts_with(&[0, 0, 1, 0]) {
        Some("image/x-icon")
    } else if b.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        // SVG is inert inside <img>: no scripts, no external loads.
        let head = String::from_utf8_lossy(&b[..b.len().min(1024)]);
        let t = head.trim_start_matches('\u{feff}').trim_start();
        ((t.starts_with("<svg") || t.starts_with("<?xml") || t.starts_with("<!--")) && head.contains("<svg"))
            .then_some("image/svg+xml")
    }
}

fn cached(data_dir: &Path, url: &str) -> PathBuf {
    let h = Sha1::digest(url.as_bytes());
    let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
    data_dir.join("cache").join("rimg").join(hex)
}

/// Bytes and MIME type of the image at `url` (`http://` is upgraded to https).
pub async fn get(data_dir: &Path, url: &str) -> Result<(Vec<u8>, &'static str), String> {
    let url = match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    };
    let u = reqwest::Url::parse(&url).map_err(|_| "bad url")?;
    if u.scheme() != "https" || !u.username().is_empty() || u.password().is_some() {
        return Err("bad url".into());
    }
    let path = cached(data_dir, u.as_str());
    if let Ok(b) = std::fs::read(&path) {
        if let Some(m) = sniff(&b) {
            return Ok((b, m));
        }
    }
    let mut res = client()
        .get(u)
        .header("Accept", "image/avif,image/webp,image/svg+xml,image/*;q=0.8")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("HTTP {}", res.status().as_u16()));
    }
    if res.content_length().is_some_and(|n| n as usize > MAX_IMAGE) {
        return Err("too large".into());
    }
    let mut buf = Vec::new();
    while let Some(c) = res.chunk().await.map_err(|e| e.to_string())? {
        buf.extend_from_slice(&c);
        if buf.len() > MAX_IMAGE {
            return Err("too large".into());
        }
    }
    let mime = sniff(&buf).ok_or("not an image")?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = crate::game::download::write_atomic(&path, &buf);
    }
    Ok((buf, mime))
}

#[cfg(test)]
mod tests {
    use super::sniff;

    #[test]
    fn sniffs_images_only() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(sniff(b"GIF89a.."), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"), Some("image/svg+xml"));
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><svg></svg>"), Some("image/svg+xml"));
        assert_eq!(sniff(b"<!DOCTYPE html><html>"), None);
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><feed/>"), None);
    }
}