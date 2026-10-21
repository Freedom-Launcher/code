//! Plugins that ship inside the launcher (repo folder `addons/`). On start they
//! are copied into `<data>/addons/` when missing or older. They are never
//! enabled on their own: the user approves them like any other plugin. A
//! plugin the user deleted comes back only when a newer version ships, and a
//! folder with the same id that the launcher did not put there is left alone.
//!
//! A built-in plugin that stops shipping goes to `RETIRED`. On start its folder,
//! storage and approval are removed, but only while the folder is still exactly
//! the signed copy the launcher installed. A copy the user changed stays theirs.

use std::{collections::BTreeMap, fs, path::Path};

use crate::{addons, signing, store, Trust};

pub struct Bundle {
    pub id: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

#[allow(unused_macros)]
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

/// Shipped plugins: `bundle!("id": "addon.json", "signature.json", ...)`.
pub const BUNDLED: &[Bundle] = &[];

/// Plugins that used to ship: (id, the `sig` of every `signature.json` shipped for it).
pub const RETIRED: &[(&str, &[&str])] = &[
    ("play-stats", &["8ebc88ca812e8a23399b09098242d93daa5099cd1951013fee4290dd0745721fb7ddaf36f193bfcbbf567b05d04ccb575f3a301f41462adfd558e98e89e6c303"]),
    (
        "server-manager",
        &[
            "3a30a1f48cd18f2a6dd188de62db6e356ba24b3bfef0a16c2d15c897ac2fd960b221f8eb19981a55b3a6d661ff82e63142705cf6b4c566ad8e3716dc516bcb00",
            "9f2ee78dc5b94eb033ec1a1682be527cb546acc0a23f56d10e6b595dc65c76a1d5a80953678c834d4caf18062a495f7c9f018ba703ca9b2fc081ecc20d34400a",
            "a4c0242a4e5655f46042df0c5e108a99fb95d9b605f1e2a3635216caf25f95a8e6638833f3e32d093d43462809ac27b03afdb85abad9d8026362ab1dedd8d002",
        ],
    ),
];

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

/// Installs or updates the bundled plugins and removes retired ones; returns the ids written.
pub fn install(data_dir: &Path) -> std::io::Result<Vec<String>> {
    install_with(data_dir, BUNDLED, RETIRED)
}

/// The folder is untouched since the launcher wrote it: an Official signature we shipped.
fn shipped_copy(dir: &Path, id: &str, sigs: &[&str]) -> bool {
    let Some(s) = fs::read(dir.join(signing::FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<signing::Sig>(&b).ok())
    else {
        return false;
    };
    sigs.contains(&s.sig.as_str()) && signing::check(dir, id) == Ok(Some(Trust::Official))
}

/// Drops the user's approval of `id` from `addons.json`.
fn forget(data_dir: &Path, id: &str) {
    let p = data_dir.join(addons::STATE);
    if let Ok(mut st) = store::load::<addons::State>(&p) {
        if st.addons.remove(id).is_some() {
            let _ = store::save(&p, &st, false);
        }
    }
}

pub(crate) fn install_with(
    data_dir: &Path,
    bundles: &[Bundle],
    retired: &[(&str, &[&str])],
) -> std::io::Result<Vec<String>> {
    let mark_path = data_dir.join(MARK);
    let mut mark: BTreeMap<String, String> = fs::read(&mark_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let before = mark.clone();
    let root = addons::dir(data_dir);
    let mut done = Vec::new();
    for b in bundles {
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
    for (id, sigs) in retired {
        if bundles.iter().any(|b| b.id == *id) || mark.remove(*id).is_none() {
            continue; // still shipping, or never installed by the launcher
        }
        let dst = root.join(id);
        if !dst.exists() || !shipped_copy(&dst, id, sigs) {
            continue; // already deleted, or changed by the user and theirs now
        }
        fs::remove_dir_all(&dst)?;
        let _ = fs::remove_dir_all(data_dir.join("addon-data").join(id));
        forget(data_dir, id);
    }
    if mark != before {
        fs::write(&mark_path, serde_json::to_vec_pretty(&mark)?)?;
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fl-bundled-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    const MANIFEST: &[u8] = br#"{"id":"demo","name":"Demo","version":"1.0.0"}"#;
    const PAGE: &[u8] = b"<p>hi</p>";
    const DEMO: &[Bundle] = &[Bundle {
        id: "demo",
        files: &[("addon.json", MANIFEST), ("index.html", PAGE)],
    }];

    #[test]
    fn installs_updates_and_respects_the_user() {
        let d = tmp();
        assert_eq!(install_with(&d, DEMO, &[]).unwrap(), vec!["demo"]);
        assert!(d.join("addons/demo/index.html").exists());
        // Never enabled on its own.
        assert!(!d.join(addons::STATE).exists());
        // Same version again: nothing to do.
        assert!(install_with(&d, DEMO, &[]).unwrap().is_empty());
        // Deleted by the user: not brought back for the same version.
        fs::remove_dir_all(d.join("addons/demo")).unwrap();
        assert!(install_with(&d, DEMO, &[]).unwrap().is_empty());
        assert!(!d.join("addons/demo").exists());
        // An older installed version is replaced whole.
        fs::write(d.join(MARK), r#"{"demo":"0.0.1"}"#).unwrap();
        assert_eq!(install_with(&d, DEMO, &[]).unwrap().len(), 1);
        assert!(d.join("addons/demo/index.html").exists());
    }

    #[test]
    fn leaves_a_foreign_folder_alone() {
        let d = tmp();
        fs::create_dir_all(d.join("addons/demo")).unwrap();
        fs::write(d.join("addons/demo/addon.json"), r#"{"id":"demo","name":"Мой","version":"9.9"}"#).unwrap();
        assert!(install_with(&d, DEMO, &[]).unwrap().is_empty());
        assert!(fs::read_to_string(d.join("addons/demo/addon.json")).unwrap().contains("9.9"));
    }

    /// An Official-signed copy of a plugin as the launcher would have installed it.
    fn shipped(d: &Path) -> String {
        let root = d.join("addons/old");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("addon.json"), r#"{"id":"old","name":"Old","version":"1.0.0"}"#).unwrap();
        fs::write(root.join("index.html"), "<p>old</p>").unwrap();
        let s = signing::sign(&root, "old", Trust::Official, "test", &signing::TEST_SEED).unwrap();
        fs::write(root.join(signing::FILE), serde_json::to_string(&s).unwrap()).unwrap();
        s.sig
    }

    #[test]
    fn retired_plugin_is_removed_only_while_untouched() {
        // Untouched copy the launcher installed: folder, storage and approval go.
        let d = tmp();
        let sig = shipped(&d);
        let sigs = [sig.as_str()];
        fs::write(d.join(MARK), r#"{"old":"1.0.0"}"#).unwrap();
        fs::create_dir_all(d.join("addon-data/old")).unwrap();
        fs::write(d.join("addon-data/old/storage.json"), "{}").unwrap();
        fs::write(d.join(addons::STATE), r#"{"addons":{"old":{"enabled":true},"keep":{"enabled":true}}}"#).unwrap();
        install_with(&d, &[], &[("old", &sigs)]).unwrap();
        assert!(!d.join("addons/old").exists());
        assert!(!d.join("addon-data/old").exists());
        assert!(!fs::read_to_string(d.join(MARK)).unwrap().contains("old"));
        let st = fs::read_to_string(d.join(addons::STATE)).unwrap();
        assert!(!st.contains("\"old\"") && st.contains("keep"));

        // Changed by the user: stays.
        let d = tmp();
        let sig = shipped(&d);
        let sigs = [sig.as_str()];
        fs::write(d.join(MARK), r#"{"old":"1.0.0"}"#).unwrap();
        fs::write(d.join("addons/old/index.html"), "<p>mine</p>").unwrap();
        install_with(&d, &[], &[("old", &sigs)]).unwrap();
        assert!(d.join("addons/old/index.html").exists());

        // Not put there by the launcher: stays.
        let d = tmp();
        let sig = shipped(&d);
        let sigs = [sig.as_str()];
        install_with(&d, &[], &[("old", &sigs)]).unwrap();
        assert!(d.join("addons/old/addon.json").exists());
    }
}