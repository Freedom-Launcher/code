//! authlib-injector (github.com/yushijinhun/authlib-injector, AGPL-3.0): a Java
//! agent that points the game's sign-in, skin and cape lookups at the service
//! of an account-provider plugin (`account_provider.yggdrasil`). It does not
//! ship with the launcher: the official release jar is downloaded on first use
//! and must match the pinned SHA-256.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Core, CoreError};

pub const VERSION: &str = "1.2.8";
const URL: &str = "https://github.com/yushijinhun/authlib-injector/releases/download/v1.2.8/authlib-injector-1.2.8.jar";
const SHA256: &str = "9c7f4343e6c82034958ffb48c14a2cb0c85928be7283103ce17da00c6d5a7b10";

fn digest(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

pub fn path(core: &Core) -> PathBuf {
    core.path("agents").join(format!("authlib-injector-{VERSION}.jar"))
}

/// The verified jar, downloading it once.
pub async fn ensure(core: &Core) -> Result<PathBuf, CoreError> {
    let p = path(core);
    if std::fs::read(&p).is_ok_and(|b| digest(&b) == SHA256) {
        return Ok(p);
    }
    let err = |e: reqwest::Error| CoreError::msg(format!("authlib-injector: {e}"));
    let bytes = core
        .http()
        .get(URL)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(err)?
        .bytes()
        .await
        .map_err(err)?;
    if digest(&bytes) != SHA256 {
        return Err(CoreError::msg("authlib-injector: the download does not match the pinned hash"));
    }
    crate::game::download::write_atomic(&p, &bytes)?;
    Ok(p)
}

/// JVM arguments that load the agent for `root`.
pub fn args(jar: &Path, root: &str) -> Vec<String> {
    vec![
        format!("-javaagent:{}={root}", jar.display()),
        "-Dauthlibinjector.side=client".into(),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn agent_args() {
        let a = super::args(std::path::Path::new("/d/a.jar"), "https://auth.example.org/api");
        assert_eq!(a[0], "-javaagent:/d/a.jar=https://auth.example.org/api");
        assert_eq!(a[1], "-Dauthlibinjector.side=client");
    }
}
