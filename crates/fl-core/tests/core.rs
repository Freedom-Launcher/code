use std::{collections::HashSet, time::Instant};

use fl_core::{accounts, Caller, Core, Permission};
use serde_json::json;

fn temp_core(safe: bool) -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join(format!("fl-test-{}", uuid::Uuid::new_v4()));
    Core::new(dir, Instant::now(), safe).unwrap()
}

#[tokio::test]
async fn builtin_ui_can_call_everything() {
    let c = temp_core(false);
    let v = c
        .dispatch(&Caller::BuiltinUi, "accounts.list", json!(null))
        .await
        .unwrap();
    assert_eq!(v["offline_available"], false);
}

#[tokio::test]
async fn themes_set_and_safe_mode() {
    let c = temp_core(false);
    let ui = Caller::BuiltinUi;
    let l = c.dispatch(&ui, "themes.list", json!(null)).await.unwrap();
    assert_eq!(l.as_array().unwrap().len(), 2);
    assert!(c
        .dispatch(&ui, "themes.set", json!({"theme": "nope"}))
        .await
        .is_err());
    let v = c
        .dispatch(&ui, "themes.set", json!({"theme": "light"}))
        .await
        .unwrap();
    assert_eq!(v["base"], "light");
    let cur = c
        .dispatch(&ui, "themes.current", json!(null))
        .await
        .unwrap();
    assert_eq!(cur["id"], "light");
    // Themes can never switch themes themselves.
    let t = Caller::Theme { id: "x".into() };
    assert!(c
        .dispatch(&t, "themes.set", json!({"theme": "default"}))
        .await
        .is_err());
    // Safe mode loads no themes at all.
    let s = temp_core(true);
    assert!(s
        .dispatch(&ui, "themes.current", json!(null))
        .await
        .is_err());
}

#[tokio::test]
async fn unknown_action_is_rejected() {
    let c = temp_core(false);
    assert!(c
        .dispatch(&Caller::BuiltinUi, "nope.nothing", json!(null))
        .await
        .is_err());
}

#[tokio::test]
async fn addon_needs_grant() {
    let c = temp_core(false);
    let none = Caller::Addon {
        id: "example".into(),
        granted: HashSet::new(),
    };
    assert!(c
        .dispatch(&none, "accounts.list", json!(null))
        .await
        .is_err());
    assert!(c.dispatch(&none, "app.info", json!(null)).await.is_ok());
    let read = Caller::Addon {
        id: "example".into(),
        granted: [Permission::AccountsRead].into(),
    };
    assert!(c
        .dispatch(&read, "accounts.list", json!(null))
        .await
        .is_ok());
}

#[tokio::test]
async fn net_fetch_only_for_addons_and_declared_hosts() {
    let c = temp_core(false);
    for (id, host) in [
        ("netdemo", "api.example.org"),
        ("other", "evil.example.org"),
    ] {
        let d = c.data_dir.join("addons").join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("index.html"), "<p>x</p>").unwrap();
        let m = json!({"id": id, "name": id, "permissions": ["network"], "hosts": [host]});
        std::fs::write(d.join("addon.json"), m.to_string()).unwrap();
    }
    let err = |r: Result<serde_json::Value, fl_core::CoreError>| r.unwrap_err().to_string();
    // The built-in UI has no business fetching through an addon's hosts.
    let e = err(c
        .dispatch(
            &Caller::BuiltinUi,
            "net.fetch",
            json!({"url": "https://api.example.org/"}),
        )
        .await);
    assert!(e.contains("только для плагинов"), "{e}");
    let addon = Caller::Addon {
        id: "netdemo".into(),
        granted: [Permission::Network].into(),
    };
    // Without the network grant the call never reaches the handler.
    let bare = Caller::Addon {
        id: "netdemo".into(),
        granted: HashSet::new(),
    };
    assert!(err(c
        .dispatch(
            &bare,
            "net.fetch",
            json!({"url": "https://api.example.org/"})
        )
        .await)
    .contains("Network"));
    // Spoofing another addon's identity is overwritten by the core.
    let spoof = json!({"$addon": "other", "url": "https://evil.example.org/"});
    let e = err(c.dispatch(&addon, "net.fetch", spoof).await);
    assert!(e.contains("не указан в hosts"), "{e}");
    let e = err(c
        .dispatch(
            &addon,
            "net.fetch",
            json!({"url": "http://api.example.org/"}),
        )
        .await);
    assert!(e.contains("https"), "{e}");
}

/// Live request; run with `cargo test -p fl-core -- --ignored net_fetch_live`.
#[tokio::test]
#[ignore]
async fn net_fetch_live() {
    let c = temp_core(false);
    let d = c.data_dir.join("addons").join("live");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("index.html"), "<p>x</p>").unwrap();
    let m = json!({"id": "live", "name": "live", "permissions": ["network"], "hosts": ["api.modrinth.com"]});
    std::fs::write(d.join("addon.json"), m.to_string()).unwrap();
    let addon = Caller::Addon {
        id: "live".into(),
        granted: [Permission::Network].into(),
    };
    let r = c
        .dispatch(
            &addon,
            "net.fetch",
            json!({"url": "https://api.modrinth.com/v2/tag/loader"}),
        )
        .await
        .unwrap();
    assert_eq!(r["status"], 200, "{r}");
    assert!(r["body"].as_str().unwrap().contains("fabric"));
}

#[tokio::test]
async fn theme_only_bindable() {
    let c = temp_core(false);
    let theme = Caller::Theme { id: "t".into() };
    assert!(c
        .dispatch(&theme, "accounts.microsoft.wait", json!(null))
        .await
        .is_err());
    assert!(c.dispatch(&theme, "app.info", json!(null)).await.is_ok());
}

#[tokio::test]
async fn offline_profile_requires_microsoft_account() {
    let c = temp_core(false);
    let err = c
        .dispatch(
            &Caller::BuiltinUi,
            "accounts.offline.create",
            json!({"name": "Steve"}),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"));

    // Simulate a signed-in Microsoft account with the game.
    std::fs::write(
        c.path("accounts.json"),
        json!({"selected": null, "accounts": [{
            "id": "a1", "kind": {"type": "microsoft"}, "name": "Owner",
            "uuid": "00000000000000000000000000000001", "owns_game": true
        }]})
        .to_string(),
    )
    .unwrap();
    let v = c
        .dispatch(
            &Caller::BuiltinUi,
            "accounts.offline.create",
            json!({"name": "Steve"}),
        )
        .await
        .unwrap();
    assert_eq!(v["accounts"].as_array().unwrap().len(), 2);
    // Secrets never leave the core.
    assert!(!v.to_string().contains("refresh_token"));
}

#[tokio::test]
async fn safe_mode_has_no_game_actions() {
    let c = temp_core(true);
    assert!(c
        .dispatch(&Caller::BuiltinUi, "accounts.list", json!(null))
        .await
        .is_err());
    let s = c
        .dispatch(&Caller::BuiltinUi, "safe.restore_theme", json!(null))
        .await
        .unwrap();
    assert_eq!(s["theme"], "default");
}

#[test]
fn profile_names() {
    assert!(accounts::validate_profile_name("Steve_01").is_ok());
    assert!(accounts::validate_profile_name("ab").is_err());
    assert!(accounts::validate_profile_name("bad name").is_err());
    assert!(accounts::validate_profile_name("abcdefghijklmnopq").is_err());
}

#[test]
fn offline_uuid_matches_game() {
    assert_eq!(
        accounts::offline_uuid("Notch"),
        "b50ad385-829d-3141-a216-7e7d7539ba7f"
    );
}

#[test]
fn addon_namespace() {
    use fl_core::actions::addon_may_register;
    assert!(addon_may_register("skins", "skins.upload"));
    assert!(!addon_may_register("skins", "accounts.list"));
    assert!(!addon_may_register("skins", "skinsx.upload"));
}

#[tokio::test]
async fn instance_manage_roundtrip() {
    let c = temp_core(false);
    let ui = Caller::BuiltinUi;
    let a = c
        .dispatch(
            &ui,
            "instances.create",
            json!({"name": "Main", "version": "1.21.4"}),
        )
        .await
        .unwrap();
    let id = a["id"].as_str().unwrap().to_owned();
    assert_eq!(a["dir"], "Main");
    let inst_dir = |d: &str| c.data_dir.join("instances").join(d);
    std::fs::write(inst_dir("Main").join("options.txt"), "x").unwrap();

    let u = c
        .dispatch(
            &ui,
            "instances.update",
            json!({"instance": id, "name": "Renamed",
            "settings": {"memory_mb": 3072, "width": 1280, "height": 720}}),
        )
        .await
        .unwrap();
    assert_eq!(u["name"], "Renamed");
    assert_eq!(u["dir"], "Renamed");
    assert!(inst_dir("Renamed").join("options.txt").exists());
    assert!(!inst_dir("Main").exists());
    assert_eq!(u["settings"]["memory_mb"], 3072);
    assert!(c
        .dispatch(
            &ui,
            "instances.update",
            json!({"instance": id, "settings": {"memory_mb": 10}})
        )
        .await
        .is_err());

    let d = c
        .dispatch(&ui, "instances.duplicate", json!({"instance": id}))
        .await
        .unwrap();
    assert_ne!(d["id"], id.as_str());
    assert_eq!(d["name"], "Renamed (2)");
    assert_eq!(d["settings"]["width"], 1280);
    assert_eq!(d["dir"], "Renamed (2)");
    assert!(inst_dir("Renamed (2)").join("options.txt").exists());

    c.dispatch(&ui, "instances.delete", json!({"instance": id}))
        .await
        .unwrap();
    assert!(!inst_dir("Renamed").exists());
    assert!(inst_dir("Renamed (2)").exists());
    let list = c
        .dispatch(&ui, "instances.list", json!(null))
        .await
        .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn legacy_folders_migrate_to_names() {
    let c = temp_core(false);
    let id = "0b9c7a52-legacy";
    std::fs::create_dir_all(c.data_dir.join("instances").join(id)).unwrap();
    std::fs::write(
        c.data_dir.join("instances").join(id).join("options.txt"),
        "x",
    )
    .unwrap();
    std::fs::write(
        c.data_dir.join("instances.json"),
        json!({"instances": [{"id": id, "name": "Old: one", "version": "1.21.4", "created_at": 1}]}).to_string(),
    )
    .unwrap();
    let list = c
        .dispatch(&Caller::BuiltinUi, "instances.list", json!(null))
        .await
        .unwrap();
    assert_eq!(list[0]["dir"], "Old one");
    assert!(c
        .data_dir
        .join("instances")
        .join("Old one")
        .join("options.txt")
        .exists());
    assert!(!c.data_dir.join("instances").join(id).exists());
}

#[tokio::test]
async fn settings_set_patches_look_and_motion() {
    let c = temp_core(false);
    let ui = Caller::BuiltinUi;
    let s = c.dispatch(&ui, "settings.get", json!(null)).await.unwrap();
    assert!(s["look"].is_null());
    let s = c
        .dispatch(
            &ui,
            "settings.set",
            json!({"look": {"palette_pos": "bottom"}, "reduce_motion": true}),
        )
        .await
        .unwrap();
    assert_eq!(s["look"]["palette_pos"], "bottom");
    assert_eq!(s["look"]["wall_blur"], 22);
    assert_eq!(s["reduce_motion"], true);
    assert_eq!(s["onboarded"], false);
    let s = c
        .dispatch(&ui, "settings.set", json!({"onboarded": true}))
        .await
        .unwrap();
    assert_eq!(s["onboarded"], true);
    assert_eq!(s["look"]["palette_pos"], "bottom");
    let s = c
        .dispatch(&ui, "settings.set", json!({"look": {"wall": "off"}}))
        .await
        .unwrap();
    assert_eq!(s["look"]["palette_pos"], "bottom");
    assert_eq!(s["look"]["wall"], "off");
    assert!(c
        .dispatch(&ui, "settings.set", json!({"look": {"wall": "x"}}))
        .await
        .is_err());
    assert!(c
        .dispatch(&ui, "settings.set", json!({"theme": "light"}))
        .await
        .is_err());
    // Themes cannot change settings; safe mode resets the look.
    let t = Caller::Theme { id: "x".into() };
    assert!(c
        .dispatch(&t, "settings.set", json!({"reduce_motion": false}))
        .await
        .is_err());
    c.dispatch(&ui, "safe.reset_settings", json!(null))
        .await
        .unwrap();
    let s = c.dispatch(&ui, "settings.get", json!(null)).await.unwrap();
    assert!(s["look"].is_null());
}
