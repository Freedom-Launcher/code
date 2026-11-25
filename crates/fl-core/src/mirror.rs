//! Download sources: Mojang's own servers or a mirror.
//!
//! Game files come from a few Mojang hosts that are slow or unreachable from
//! some networks. A mirror serves the same files under its own host. Every file
//! still has its SHA1 checked, so a mirror cannot pass off broken or tampered
//! content: a mismatch is rejected exactly as for the official source.
//!
//! `settings.mirror` is one of:
//!
//! - `"auto"` (default): the official source first; the mirror is the last
//!   attempt of a download the official source could not deliver;
//! - `"mojang"`: the official source only;
//! - `"bmclapi"`: the public BMCLAPI mirror first, the official source as the retry;
//! - the https base of another mirror with the same URL layout as BMCLAPI.
//!
//! Only the game-file hosts of [`OFFICIAL`] are rewritten. Version JSONs are
//! deliberately left out: BMCLAPI serves them rewritten (their download links
//! point back at the mirror), which no longer matches the SHA1 in the version
//! manifest, and that SHA1 is what the launcher insists on.

use std::time::Duration;

use crate::{Core, CoreError};

/// The public BMCLAPI mirror (https://bmclapi.bangbang93.com).
pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";

/// How long a mirror may take to answer a request. It bounds the wait for the
/// response only, not the download itself: a mirror that is down must not hold
/// up the official attempt queued behind it.
pub const MIRROR_ANSWER: Duration = Duration::from_secs(15);

/// When a download may go to a mirror, and what its base is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Official sources only.
    Never,
    /// Official first; the mirror is the last resort of a failing download.
    Auto(String),
    /// The mirror first; the official source is the retry.
    Always(String),
}

impl Rule {
    /// The mirror's base, when the rule uses one.
    pub fn base(&self) -> Option<&str> {
        match self {
            Rule::Never => None,
            Rule::Auto(b) | Rule::Always(b) => Some(b),
        }
    }
}

/// The rule the settings ask for. Unreadable settings mean "auto".
pub fn rule(core: &Core) -> Rule {
    let saved = core.settings().ok();
    let mode = saved.as_ref().map_or("auto", |s| s.mirror.as_str());
    match mode {
        "mojang" => Rule::Never,
        "bmclapi" => Rule::Always(BMCLAPI.to_owned()),
        "auto" => Rule::Auto(BMCLAPI.to_owned()),
        custom => Rule::Always(custom.to_owned()),
    }
}

/// A mirror base a user may type: plain https, no credentials, no path or query.
pub fn valid_base(u: &str) -> bool {
    let Ok(p) = reqwest::Url::parse(u) else {
        return false;
    };
    u.len() <= 200
        && u.bytes()
            .all(|b| b > 0x20 && b < 0x7f && !matches!(b, b'"' | b'\\' | b'<' | b'>'))
        && p.scheme() == "https"
        && p.host_str().is_some_and(|h| !h.is_empty())
        && p.username().is_empty()
        && p.password().is_none()
        && (p.path() == "/" || p.path().is_empty())
        && p.query().is_none()
        && p.fragment().is_none()
}

/// Official source prefixes a mirror serves, with their path on the mirror.
const OFFICIAL: [(&str, &str); 9] = [
    ("https://resources.download.minecraft.net/", "assets/"),
    ("https://libraries.minecraft.net/", "maven/"),
    ("https://maven.fabricmc.net/", "maven/"),
    ("https://maven.minecraftforge.net/", "maven/"),
    ("https://meta.fabricmc.net/", "fabric-meta/"),
    ("https://piston-data.mojang.com/", ""),
    (
        "https://piston-meta.mojang.com/v1/products/",
        "v1/products/",
    ),
    ("https://piston-meta.mojang.com/mc/game/", "mc/game/"),
    ("https://launchermeta.mojang.com/", ""),
];

/// The mirror URL for an official download, or `None` when this URL is not one
/// of the known game sources or already points at `base`. Quilt and Adoptium
/// are left out on purpose: their mirror routes are neither documented nor
/// checked here, and those downloads are small.
pub fn rewrite(url: &str, base: &str) -> Option<String> {
    if url.starts_with(base) {
        return None;
    }
    let base = base.trim_end_matches('/');
    for (official, mirror_path) in OFFICIAL {
        if let Some(rest) = url.strip_prefix(official) {
            if rest.is_empty() {
                return None;
            }
            return Some(format!("{base}/{mirror_path}{rest}"));
        }
    }
    None
}

/// The mirror's URL for `official` under this rule, when there is one.
fn mirrored(rule: &Rule, official: &str) -> Option<String> {
    rule.base().and_then(|b| rewrite(official, b))
}

/// The URLs one download is tried against, in order. The mirror gets a single
/// attempt on the side the rule puts it, so a broken mirror costs one try and
/// never the download: the official attempts stay as they were.
pub fn attempts(rule: &Rule, official: &str) -> Vec<String> {
    let official = official.to_owned();
    match (rule, mirrored(rule, &official)) {
        (Rule::Always(_), Some(m)) => vec![m, official.clone(), official],
        (Rule::Auto(_), Some(m)) => vec![official.clone(), official, m],
        _ => vec![official; crate::game::download::ATTEMPTS as usize],
    }
}

/// The URLs of a small metadata request: one try per side, in the rule's order.
pub fn meta_urls(rule: &Rule, official: &str) -> Vec<String> {
    let official = official.to_owned();
    match (rule, mirrored(rule, &official)) {
        (Rule::Always(_), Some(m)) => vec![m, official],
        (Rule::Auto(_), Some(m)) => vec![official, m],
        _ => vec![official],
    }
}

/// One GET, as far as the response. With `bounded`, a mirror that has not
/// answered after `limit` counts as failed.
pub async fn get(
    http: &reqwest::Client,
    url: &str,
    limit: Option<Duration>,
) -> Result<reqwest::Response, CoreError> {
    let send = http.get(url).send();
    let Some(limit) = limit else {
        return Ok(send.await?);
    };
    tokio::time::timeout(limit, send)
        .await
        .map_err(|_| CoreError::msg(format!("зеркало не ответило: {url}")))?
        .map_err(CoreError::from)
}

/// A metadata GET with the mirror fallback, for the direct requests outside
/// the bulk downloader (version manifest, version JSON, loader meta, Java index).
pub async fn get_with_fallback(
    http: &reqwest::Client,
    rule: &Rule,
    official: &str,
) -> Result<Vec<u8>, CoreError> {
    let mut last = None;
    for url in meta_urls(rule, official) {
        let bounded = (url != official).then_some(MIRROR_ANSWER);
        let got: Result<Vec<u8>, CoreError> = async {
            Ok(get(http, &url, bounded)
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec())
        }
        .await;
        match got {
            Ok(b) => return Ok(b),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| CoreError::msg("request failed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sources_are_rewritten() {
        let m = |u: &str| rewrite(u, BMCLAPI);
        assert_eq!(
            m("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json").as_deref(),
            Some("https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json")
        );
        assert_eq!(
            m("https://piston-data.mojang.com/v1/objects/ab/client.jar").as_deref(),
            Some("https://bmclapi2.bangbang93.com/v1/objects/ab/client.jar")
        );
        assert_eq!(
            m("https://piston-meta.mojang.com/v1/products/java-runtime/ab/all.json").as_deref(),
            Some("https://bmclapi2.bangbang93.com/v1/products/java-runtime/ab/all.json")
        );
        assert_eq!(
            m("https://launchermeta.mojang.com/v1/products/java-runtime/ab/all.json").as_deref(),
            Some("https://bmclapi2.bangbang93.com/v1/products/java-runtime/ab/all.json")
        );
        assert_eq!(
            m("https://libraries.minecraft.net/com/mojang/logging/1.1.1/logging-1.1.1.jar")
                .as_deref(),
            Some(
                "https://bmclapi2.bangbang93.com/maven/com/mojang/logging/1.1.1/logging-1.1.1.jar"
            )
        );
        assert_eq!(
            m("https://resources.download.minecraft.net/ab/abcdef").as_deref(),
            Some("https://bmclapi2.bangbang93.com/assets/ab/abcdef")
        );
        assert_eq!(
            m("https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar").as_deref(),
            Some("https://bmclapi2.bangbang93.com/maven/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar")
        );
        assert_eq!(
            m("https://maven.minecraftforge.net/net/minecraftforge/forge/1.0/forge-1.0-installer.jar").as_deref(),
            Some("https://bmclapi2.bangbang93.com/maven/net/minecraftforge/forge/1.0/forge-1.0-installer.jar")
        );
        assert_eq!(
            m("https://meta.fabricmc.net/v2/versions/loader/1.21.4").as_deref(),
            Some("https://bmclapi2.bangbang93.com/fabric-meta/v2/versions/loader/1.21.4")
        );
    }

    #[test]
    fn unknown_and_unchecked_urls_stay() {
        for url in [
            // Served rewritten by the mirror: it would fail the manifest's SHA1.
            "https://piston-meta.mojang.com/v1/packages/ab12/cd.json",
            "https://api.modrinth.com/v2/project/sodium/version",
            "https://cdn.modrinth.com/data/df/ab.zip",
            "https://api.adoptium.net/v3/assets/latest/21/hotspot",
            "https://meta.quiltmc.org/v3/versions/loader/1.21.4",
            "https://maven.neoforged.net/releases/net/neoforged/forge.jar",
            "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
            "https://resources.download.minecraft.net", // the bare host, no file
            "https://bmclapi2.bangbang93.com/maven/a.jar", // already the mirror
            "http://libraries.minecraft.net/com/x.jar", // not https
        ] {
            assert_eq!(rewrite(url, BMCLAPI), None, "{url}");
        }
    }

    #[test]
    fn a_custom_base_is_used_verbatim() {
        let base = "https://mirror.example.org/";
        assert_eq!(
            rewrite("https://libraries.minecraft.net/a/b.jar", base).as_deref(),
            Some("https://mirror.example.org/maven/a/b.jar")
        );
    }

    #[test]
    fn attempt_orders() {
        let url = "https://libraries.minecraft.net/a.jar";
        let via = "https://bmclapi2.bangbang93.com/maven/a.jar";
        assert_eq!(
            attempts(&Rule::Auto(BMCLAPI.into()), url),
            vec![url, url, via]
        );
        assert_eq!(
            attempts(&Rule::Always(BMCLAPI.into()), url),
            vec![via, url, url]
        );
        assert_eq!(attempts(&Rule::Never, url), vec![url.to_owned(); 3]);
        // Not a game source: the rule adds nothing, whichever it is.
        let other = "https://cdn.modrinth.com/data/x.zip";
        for rule in [Rule::Auto(BMCLAPI.into()), Rule::Always(BMCLAPI.into())] {
            assert_eq!(attempts(&rule, other), vec![other.to_owned(); 3]);
            assert_eq!(meta_urls(&rule, other), vec![other.to_owned()]);
        }
        assert_eq!(
            meta_urls(&Rule::Auto(BMCLAPI.into()), url),
            vec![url.to_owned(), via.to_owned()]
        );
        assert_eq!(meta_urls(&Rule::Never, url), vec![url.to_owned()]);
    }

    #[test]
    fn bases_are_checked() {
        assert!(valid_base("https://mirror.example.org"));
        assert!(valid_base("https://mirror.example.org/"));
        assert!(valid_base("https://mirror.example.org:5210"));
        for bad in [
            "",
            "auto",
            "http://mirror.example.org",
            "https://u:p@mirror.example.org/",
            "https://mirror.example.org/maven",
            "https://mirror.example.org/?x=1",
            "https://mirror.example.org/#x",
            "ftp://mirror.example.org",
            "https://",
            "https://mirror example.org",
        ] {
            assert!(!valid_base(bad), "{bad:?}");
        }
    }

    /// A mirror that accepts the connection and never answers must not hold up
    /// the attempt queued behind it: the bounded GET gives up on its own.
    #[tokio::test]
    async fn a_silent_mirror_is_given_up() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/a.jar", listener.local_addr().unwrap());
        let hold = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let http = reqwest::Client::builder().no_proxy().build().unwrap();
        let silent = get(&http, &url, Some(Duration::from_millis(300))).await;
        assert!(silent.is_err(), "a silent mirror must fail");
        assert!(
            silent.unwrap_err().to_string().contains("не ответило"),
            "and say so"
        );
        hold.abort();
    }
}
