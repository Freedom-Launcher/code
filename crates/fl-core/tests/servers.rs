use std::{fs, path::PathBuf, time::Instant};

use fl_core::{bundled, Caller, Core, Permission};
use serde_json::{json, Value};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fl-servers-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[tokio::test]
async fn local_server_lifecycle() {
    let data = tmp("data");
    let srv = tmp("srv");
    fs::write(srv.join("server.properties"), "#Minecraft server properties\nmotd=\\u00A7aHi\nonline-mode=false\nmax-players=10\n").unwrap();
    fs::write(srv.join("paper-1.21.1-100.jar"), b"x").unwrap();
    fs::create_dir_all(srv.join("plugins")).unwrap();

    let c = Core::new(data.clone(), Instant::now(), false).unwrap();
    let ui = Caller::BuiltinUi;
    let addon = Caller::Addon { id: "server-manager".into(), granted: [Permission::Servers].into() };
    let none = Caller::Addon { id: "other".into(), granted: [Permission::InstanceFiles].into() };

    // A plugin cannot name a folder itself, and needs the permission for everything else.
    assert!(c.dispatch(&addon, "servers.add", json!({ "dir": srv.to_string_lossy() })).await.is_err());
    assert!(c.dispatch(&none, "servers.list", Value::Null).await.is_err());
    // The launcher's own data is never a server folder.
    assert!(c.dispatch(&ui, "servers.add", json!({ "dir": data.to_string_lossy() })).await.is_err());

    let r = c.dispatch(&ui, "servers.add", json!({ "name": "Тест", "dir": srv.to_string_lossy() })).await.unwrap();
    let id = r["id"].as_str().unwrap().to_owned();
    let again = c.dispatch(&ui, "servers.add", json!({ "dir": srv.to_string_lossy() })).await.unwrap();
    assert_eq!(again["existing"], json!(true));

    let info = c.dispatch(&addon, "servers.info", json!({ "id": id })).await.unwrap();
    assert_eq!(info["motd"], json!("§aHi"));
    assert_eq!(info["software"]["kind"], json!("Paper"));
    assert_eq!(info["software"]["version"], json!("1.21.1"));
    assert_eq!(info["eula"], json!(false));

    c.dispatch(&addon, "servers.properties.set", json!({ "id": id, "values": { "max-players": 50, "white-list": true } })).await.unwrap();
    let props = fs::read_to_string(srv.join("server.properties")).unwrap();
    assert!(props.starts_with("#Minecraft server properties\nmotd=\\u00A7aHi\n") && props.contains("max-players=50") && props.contains("white-list=true"));

    // Offline server: offline UUID, no network.
    let p = c.dispatch(&addon, "servers.players.edit", json!({ "id": id, "list": "whitelist", "op": "add", "name": "Steve" })).await.unwrap();
    assert_eq!(p["uuid"].as_str().unwrap().len(), 36);
    let pl = c.dispatch(&addon, "servers.players", json!({ "id": id })).await.unwrap();
    assert_eq!(pl["whitelist"][0]["name"], json!("Steve"));
    assert_eq!(pl["whitelist_enabled"], json!(true));
    c.dispatch(&addon, "servers.players.edit", json!({ "id": id, "list": "whitelist", "op": "remove", "name": "steve" })).await.unwrap();
    let pl = c.dispatch(&addon, "servers.players", json!({ "id": id })).await.unwrap();
    assert!(pl["whitelist"].as_array().unwrap().is_empty());

    // Files: inside the folder only, no code written.
    let ls = c.dispatch(&addon, "servers.files.list", json!({ "id": id })).await.unwrap();
    assert_eq!(ls[0]["name"], json!("plugins"));
    c.dispatch(&addon, "servers.files.write", json!({ "id": id, "path": "eula.txt", "text": "eula=true\n" })).await.unwrap();
    for (path, what) in [("../x.txt", "escape"), ("plugins/x.jar", "jar"), ("start.bat", "script"), ("mods/a.txt", "mods")] {
        assert!(c.dispatch(&addon, "servers.files.write", json!({ "id": id, "path": path, "text": "x" })).await.is_err(), "{what}");
    }
    assert!(c.dispatch(&addon, "servers.files.read", json!({ "id": id, "path": "../server.properties" })).await.is_err());

    // Icon: 64×64 PNG only.
    assert!(c.dispatch(&addon, "servers.icon.set", json!({ "id": id, "png": "aGVsbG8=" })).await.is_err());

    c.dispatch(&addon, "servers.update", json!({ "id": id, "name": "Новое имя" })).await.unwrap();
    c.dispatch(&addon, "servers.remove", json!({ "id": id })).await.unwrap();
    assert!(c.dispatch(&addon, "servers.list", Value::Null).await.unwrap().as_array().unwrap().is_empty());
    assert!(srv.join("server.properties").is_file(), "removing a server keeps its files");
}

#[test]
fn bundled_server_manager_is_official() {
    let data = tmp("bundle");
    let ids = bundled::install(&data).unwrap();
    assert!(ids.iter().any(|i| i == "server-manager"));
    let dir = data.join("addons").join("server-manager");
    assert_eq!(fl_core::signing::check(&dir, "server-manager"), Ok(Some(fl_core::Trust::Official)));
    let m = fl_core::addons::load(&data, "server-manager").unwrap();
    assert!(m.permissions.contains(&Permission::Servers));
}