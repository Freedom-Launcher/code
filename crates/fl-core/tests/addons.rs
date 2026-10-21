use std::{fs, path::PathBuf, time::Instant};

use fl_core::{addons, Caller, Core, Permission};
use serde_json::json;

fn tmp(files: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "fl-addons-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    for (p, c) in files {
        let p = d.join("addons").join(p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, c).unwrap();
    }
    fs::create_dir_all(&d).unwrap();
    d
}

const GOOD: &str = r#"{"id":"stats","name":"Статистика","version":"1.0",
  "permissions":["instances_read","network"],"slots":["instance.end"],
  "hosts":["api.modrinth.com"]}"#;

#[test]
fn manifest_checks() {
    let d = tmp(&[
        ("stats/addon.json", GOOD),
        ("stats/index.html", "<p>hi</p>"),
        ("game/addon.json", r#"{"id":"game","name":"x"}"#),
        ("game/index.html", ""),
        (
            "ext/addon.json",
            r#"{"id":"ext","name":"x","permissions":["process"]}"#,
        ),
        ("ext/index.html", ""),
        (
            "int/addon.json",
            r#"{"id":"int","name":"x","permissions":["internal"]}"#,
        ),
        ("int/index.html", ""),
        (
            "net/addon.json",
            r#"{"id":"net","name":"x","hosts":["a.com"]}"#,
        ),
        ("net/index.html", ""),
        (
            "slot/addon.json",
            r#"{"id":"slot","name":"x","slots":["view.settings"]}"#,
        ),
        ("slot/index.html", ""),
        ("noentry/addon.json", r#"{"id":"noentry","name":"x"}"#),
    ]);
    assert!(addons::load(&d, "stats").is_ok());
    for bad in ["game", "ext", "int", "net", "slot", "noentry", "../x"] {
        assert!(addons::load(&d, bad).is_err(), "{bad} must be rejected");
    }
    assert!(addons::file(&d, "stats", "../stats/addon.json").is_none());
}

#[tokio::test]
async fn approval_gates_the_caller() {
    let d = tmp(&[
        ("stats/addon.json", GOOD),
        ("stats/index.html", "<p>hi</p>"),
    ]);
    let c = Core::new(d.clone(), Instant::now(), false).unwrap();
    assert!(c.addon_caller("stats").is_none(), "not approved yet");
    let ui = Caller::BuiltinUi;
    let list = c.dispatch(&ui, "addons.list", json!(null)).await.unwrap();
    assert_eq!(list[0]["pending"], true);
    c.dispatch(&ui, "addons.approve", json!({"id": "stats"}))
        .await
        .unwrap();
    let a = c.addon_caller("stats").expect("approved");
    assert!(c.dispatch(&a, "instances.list", json!(null)).await.is_ok());
    assert!(c.dispatch(&a, "accounts.list", json!(null)).await.is_err());
    // Addons can never manage the launcher itself.
    assert!(c
        .dispatch(&a, "addons.approve", json!({"id": "stats"}))
        .await
        .is_err());
    assert!(c
        .dispatch(&a, "themes.set", json!({"theme": "light"}))
        .await
        .is_err());
    let sneaky = Caller::Addon {
        id: "x".into(),
        granted: [Permission::Internal].into(),
    };
    assert!(c
        .dispatch(&sneaky, "addons.list", json!(null))
        .await
        .is_err());
    // A theme click cannot run recovery actions either.
    let t = Caller::Theme { id: "t".into() };
    assert!(c
        .dispatch(&t, "safe.reset_settings", json!(null))
        .await
        .is_err());
    c.dispatch(
        &ui,
        "addons.set_enabled",
        json!({"id": "stats", "enabled": false}),
    )
    .await
    .unwrap();
    assert!(c.addon_caller("stats").is_none(), "disabled");
    // Safe mode never runs addons.
    let s = Core::new(d, Instant::now(), true).unwrap();
    assert!(s.addon_caller("stats").is_none());
}

#[test]
fn example_addon_is_valid() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/addons/example");
    let d = tmp(&[]);
    let dst = d.join("addons/example");
    fs::create_dir_all(&dst).unwrap();
    for f in fs::read_dir(&src).unwrap().flatten() {
        fs::copy(f.path(), dst.join(f.file_name())).unwrap();
    }
    let m = addons::load(&d, "example").unwrap();
    assert!(m.permissions.contains(&Permission::InstancesRead));
}

const RICH: &str = r#"{"id":"rich","name":"Богатый","version":"1.0",
  "permissions":["instances_read","ui_pages","ui_overlay","storage","instance_files"],
  "pages":[{"id":"grid","title":"Сетка"}],"launch_overlay":"launch.html"}"#;

fn rich() -> PathBuf {
    let d = tmp(&[
        ("rich/addon.json", RICH),
        ("rich/index.html", ""),
        ("rich/grid.html", ""),
        ("rich/launch.html", ""),
        ("other/addon.json", r#"{"id":"other","name":"x","permissions":["storage"]}"#),
        ("other/index.html", ""),
    ]);
    fs::write(
        d.join("instances.json"),
        r#"{"instances":[{"id":"i1","name":"A","dir":"a","version":"1.20.1","created_at":0}]}"#,
    )
    .unwrap();
    fs::create_dir_all(d.join("instances/a/config")).unwrap();
    d
}

#[test]
fn pages_and_overlay_checks() {
    let d = tmp(&[
        ("nop/addon.json", r#"{"id":"nop","name":"x","pages":[{"id":"p","title":"P"}]}"#),
        ("nop/index.html", ""),
        ("nop/p.html", ""),
        ("nofile/addon.json", r#"{"id":"nofile","name":"x","permissions":["ui_pages"],"pages":[{"id":"p","title":"P"}]}"#),
        ("nofile/index.html", ""),
        ("ovl/addon.json", r#"{"id":"ovl","name":"x","launch_overlay":"l.html"}"#),
        ("ovl/index.html", ""),
        ("ovl/l.html", ""),
        ("dup/addon.json", r#"{"id":"dup","name":"x","permissions":["ui_pages"],"pages":[{"id":"p","title":"P"},{"id":"p","title":"Q"}]}"#),
        ("dup/index.html", ""),
        ("dup/p.html", ""),
        ("esc/addon.json", r#"{"id":"esc","name":"x","permissions":["ui_overlay"],"launch_overlay":"../x.html"}"#),
        ("esc/index.html", ""),
    ]);
    for bad in ["nop", "nofile", "ovl", "dup", "esc"] {
        assert!(addons::load(&d, bad).is_err(), "{bad} must be rejected");
    }
    let r = rich();
    let m = addons::load(&r, "rich").unwrap();
    assert_eq!(m.pages[0].entry, "grid.html");
}

#[tokio::test]
async fn partial_approval_and_declined() {
    let d = rich();
    let c = Core::new(d, Instant::now(), false).unwrap();
    let ui = Caller::BuiltinUi;
    let list = c
        .dispatch(&ui, "addons.approve", json!({"id": "rich", "permissions": ["instances_read", "ui_pages"]}))
        .await
        .unwrap();
    let r = list.as_array().unwrap().iter().find(|a| a["id"] == "rich").unwrap();
    assert_eq!(r["pending"], false, "declined ones are not asked again");
    assert_eq!(r["pages"].as_array().unwrap().len(), 1);
    assert!(r["launch_overlay"].is_null(), "overlay not granted");
    let a = c.addon_caller("rich").unwrap();
    assert!(c.dispatch(&a, "instances.list", json!(null)).await.is_ok());
    assert!(c.dispatch(&a, "storage.list", json!(null)).await.is_err());
    assert!(c
        .dispatch(&a, "files.list", json!({"instance": "i1"}))
        .await
        .is_err());
}

#[tokio::test]
async fn storage_is_per_addon() {
    let d = rich();
    let c = Core::new(d, Instant::now(), false).unwrap();
    let ui = Caller::BuiltinUi;
    for id in ["rich", "other"] {
        c.dispatch(&ui, "addons.approve", json!({"id": id})).await.unwrap();
    }
    let a = c.addon_caller("rich").unwrap();
    let b = c.addon_caller("other").unwrap();
    c.dispatch(&a, "storage.set", json!({"key": "k", "value": {"x": 1}}))
        .await
        .unwrap();
    let got = c.dispatch(&a, "storage.get", json!({"key": "k"})).await.unwrap();
    assert_eq!(got["value"]["x"], 1);
    // A forged `$addon` is overwritten by the core.
    let other = c
        .dispatch(&b, "storage.get", json!({"key": "k", "$addon": "rich"}))
        .await
        .unwrap();
    assert!(other["value"].is_null());
    // The built-in UI is not an addon: no storage of its own.
    assert!(c.dispatch(&ui, "storage.list", json!(null)).await.is_err());
    let big = "x".repeat(1100 * 1024);
    assert!(c
        .dispatch(&a, "storage.set", json!({"key": "big", "value": big}))
        .await
        .is_err());
    c.dispatch(&a, "storage.delete", json!({"key": "k"})).await.unwrap();
    let keys = c.dispatch(&a, "storage.list", json!(null)).await.unwrap();
    assert_eq!(keys["keys"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn instance_files_stay_inside() {
    let d = rich();
    let c = Core::new(d.clone(), Instant::now(), false).unwrap();
    c.dispatch(&Caller::BuiltinUi, "addons.approve", json!({"id": "rich"}))
        .await
        .unwrap();
    let a = c.addon_caller("rich").unwrap();
    c.dispatch(&a, "files.write", json!({"instance": "i1", "path": "config/x.json", "text": "{}"}))
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(d.join("instances/a/config/x.json")).unwrap(), "{}");
    let r = c
        .dispatch(&a, "files.read", json!({"instance": "i1", "path": "config/x.json"}))
        .await
        .unwrap();
    assert_eq!(r["text"], "{}");
    let l = c.dispatch(&a, "files.list", json!({"instance": "i1", "path": "config"})).await.unwrap();
    assert_eq!(l[0]["name"], "x.json");
    for (path, why) in [
        ("../../addons.json", "escape"),
        ("mods/evil.txt", "mods dir"),
        ("evil.jar", "code"),
        ("run.bat", "script"),
        ("freedom-blocked.json", "launcher file"),
        ("/etc/passwd", "absolute"),
    ] {
        assert!(
            c.dispatch(&a, "files.write", json!({"instance": "i1", "path": path, "text": "x"}))
                .await
                .is_err(),
            "{why}: {path}"
        );
    }
    assert!(c
        .dispatch(&a, "files.read", json!({"instance": "i1", "path": "../../addons.json"}))
        .await
        .is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&d, d.join("instances/a/link")).unwrap();
        assert!(c
            .dispatch(&a, "files.read", json!({"instance": "i1", "path": "link/addons.json"}))
            .await
            .is_err());
        assert!(c
            .dispatch(&a, "files.write", json!({"instance": "i1", "path": "link/z.txt", "text": "x"}))
            .await
            .is_err());
    }
    c.dispatch(&a, "files.delete", json!({"instance": "i1", "path": "config/x.json"}))
        .await
        .unwrap();
    assert!(!d.join("instances/a/config/x.json").exists());
}
