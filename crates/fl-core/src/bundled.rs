//! Plugins that ship inside the launcher (repo folder `addons/`). On start they
//! are copied into `<data>/addons/` when missing or older. They are never
//! enabled on their own: the user approves them like any other plugin. A
//! plugin the user deleted comes back only when a newer version ships, and a
//! folder with the same id that the launcher did not put there is left alone.

use std::{collections::BTreeMap, fs, path::Path};

pub struct Bundle {
    pub id: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

macro_rules! bundle {
    ($id:literal: $($f:literal),+ $(,)?) => {
        Bundle {
            id: $id,
            files: &[$((
                $f,
                include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../addons/", $id, "/", $f)),
            )),+],
        }
    };
}

pub const BUNDLED: &[Bundle] = &[bundle!("play-stats":
    "addon.json", "signature.json",
    "stats.html", "stats.js", "stats.css",
    "launch.html", "launch.js", "launch.css",
)];

/// Which bundled versions were installed: `{ id: version }`.
pub const MARK: &str = "addons-bundled.json";

fn version_of(json: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(json)
        .ok()?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

impl Bundle {
    pub fn version(&self) -> String {
        self.files
            .iter()
            .find(|f| f.0 == "addon.json")
            .and_then(|f| version_of(f.1))
            .unwrap_or_default()
    }
}

/// Installs or updates the bundled plugins; returns the ids written.
pub fn install(data_dir: &Path) -> std::io::Result<Vec<String>> {
    let mark_path = data_dir.join(MARK);
    let mut mark: BTreeMap<String, String> = fs::read(&mark_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let before = mark.clone();
    let root = crate::addons::dir(data_dir);
    let mut done = Vec::new();
    for b in BUNDLED {
        let v = b.version();
        if mark.get(b.id) == Some(&v) {
            continue;
        }
        let dst = root.join(b.id);
        if dst.exists() && !mark.contains_key(b.id) {
            // Not ours. The same version copied by hand is adopted, anything else is the user's.
            if fs::read(dst.join("addon.json")).ok().and_then(|j| version_of(&j)) == Some(v.clone()) {
                mark.insert(b.id.into(), v);
            }
            continue;
        }
        // Whole folder at once: a stray old file would break the signature.
        let tmp = root.join(format!(".{}.new", b.id));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp)?;
        for (name, bytes) in b.files {
            fs::write(tmp.join(name), bytes)?;
        }
        if dst.exists() {
            fs::remove_dir_all(&dst)?;
        }
        fs::rename(&tmp, &dst)?;
        mark.insert(b.id.into(), v);
        done.push(b.id.to_owned());
    }
    if mark != before {
        fs::write(&mark_path, serde_json::to_vec_pretty(&mark)?)?;
    }
    Ok(done)
}