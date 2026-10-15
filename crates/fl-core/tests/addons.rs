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
