use std::{fs, path::PathBuf, time::Instant};

use fl_core::{Caller, Core, Permission};
use serde_json::{json, Value};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fl-cores-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn addon() -> Caller {
    Caller::Addon { id: "server-manager".into(), granted: [Permission::Servers].into() }
}

#[tokio::test]
async fn create_rules() {
    let data = tmp("rules");
    let c = Core::new(data.clone(), Instant::now(), false).unwrap();
    let a = addon();
    let cores = c.dispatch(&a, "servers.cores", Value::Null).await.unwrap();
    assert_eq!(cores.as_array().unwrap().len(), 6);
    // A plugin never names a folder, and the folder variant is built-in only.
    let dir = tmp("rules-dir");
    let e = c.dispatch(&a, "servers.create", json!({ "core": "paper", "mc": "1.21.4", "dir": dir.to_string_lossy() })).await;
    assert!(e.is_err());
    let e = c.dispatch(&a, "servers.create.at", json!({ "core": "paper", "mc": "1.21.4", "dir": dir.to_string_lossy() })).await;
    assert!(e.is_err());
    // Bad input is refused before anything is downloaded or created.
    for bad in [
        json!({ "core": "bukkit", "mc": "1.21.4" }),
        json!({ "core": "paper", "mc": "../../x" }),
        json!({ "core": "paper", "mc": "1.21.4", "build": "1 2" }),
        json!({ "core": "paper", "mc": "1.21.4", "memory_mb": 100 }),
        json!({ "core": "paper", "mc": "1.21.4", "job": "a/b" }),
    ] {
        assert!(c.dispatch(&a, "servers.create", bad.clone()).await.is_err(), "{bad}");
    }
    assert!(!data.join("servers").exists() || fs::read_dir(data.join("servers")).unwrap().next().is_none());
    // Files are deleted only for servers the launcher created.
    let srv = tmp("rules-srv");
    let r = c.dispatch(&Caller::BuiltinUi, "servers.add", json!({ "dir": srv.to_string_lossy() })).await.unwrap();
    let id = r["id"].as_str().unwrap();
    assert!(c.dispatch(&a, "servers.remove", json!({ "id": id, "delete_files": true })).await.is_err());
    assert!(srv.exists());
    c.dispatch(&a, "servers.remove", json!({ "id": id })).await.unwrap();
    assert!(srv.exists());
    // No core installed by the launcher: memory lives in the user's own script.
    let r = c.dispatch(&Caller::BuiltinUi, "servers.add", json!({ "dir": srv.to_string_lossy() })).await.unwrap();
    assert!(c.dispatch(&a, "servers.memory.set", json!({ "id": r["id"], "memory_mb": 2048 })).await.is_err());
}

/// Real downloads: `FL_CORES="paper:1.21.4 fabric:1.21.4" cargo test -- --ignored`.
#[tokio::test]
#[ignore]
async fn install_cores_online() {
    let data = std::env::var("FL_CORES_DATA").map(PathBuf::from).unwrap_or_else(|_| tmp("online"));
    fs::create_dir_all(&data).unwrap();
    let c = Core::new(data.clone(), Instant::now(), false).unwrap();
    let a = addon();
    let list = std::env::var("FL_CORES").unwrap_or_else(|_| "vanilla:1.21.4 paper:1.21.4".into());
    for item in list.split_whitespace() {
        let (core, mc) = item.split_once(':').unwrap();
        let v = c.dispatch(&a, "servers.core.versions", json!({ "core": core })).await.unwrap();
        assert!(v.as_array().unwrap().iter().any(|x| x["id"] == mc), "{core}: no {mc}");
        let b = c.dispatch(&a, "servers.core.builds", json!({ "core": core, "mc": mc })).await.unwrap();
        println!("{core} {mc}: recommended {}", b["recommended"]);
        let t = Instant::now();
        let r = c
            .dispatch(&a, "servers.create", json!({ "name": format!("{core} {mc}"), "core": core, "mc": mc, "memory_mb": 1536, "eula": true, "job": format!("j-{core}") }))
            .await;
        let r = r.unwrap_or_else(|e| panic!("{core} {mc}: {e}"));
        let p = c.dispatch(&a, "servers.progress", json!({ "job": format!("j-{core}") })).await.unwrap();
        assert_eq!(p["stage"], json!("done"));
        let info = c.dispatch(&a, "servers.info", json!({ "id": r["id"] })).await.unwrap();
        println!("{core} {mc}: {} in {:?} -> {} launch {}", info["software"], t.elapsed(), r["dir"], info["core"]["launch"]);
        assert_eq!(info["eula"], json!(true));
        assert!(info["script"].is_string());
    }
}
/// Changing the core of a server keeps the world and settings and removes the old jar.
#[tokio::test]
#[ignore]
async fn change_core_online() {
    let data = tmp("change");
    let c = Core::new(data.clone(), Instant::now(), false).unwrap();
    let a = addon();
    let r = c
        .dispatch(&a, "servers.create", json!({ "name": "Смена", "core": "paper", "mc": "1.21.4", "eula": true }))
        .await
        .unwrap();
    let dir = PathBuf::from(r["dir"].as_str().unwrap());
    fs::create_dir_all(dir.join("world")).unwrap();
    fs::write(dir.join("world/level.dat"), b"w").unwrap();
    let ch = c
        .dispatch(&a, "servers.core.install", json!({ "id": r["id"], "core": "purpur", "mc": "1.21.4", "job": "chg" }))
        .await
        .unwrap();
    assert!(ch["removed"].as_str().unwrap().starts_with("paper-1.21.4-"));
    let info = c.dispatch(&a, "servers.info", json!({ "id": r["id"] })).await.unwrap();
    assert_eq!(info["software"]["core"], json!("purpur"));
    assert!(dir.join("world/level.dat").exists() && dir.join("eula.txt").exists());
    let m = c.dispatch(&a, "servers.memory.set", json!({ "id": r["id"], "memory_mb": 3072 })).await.unwrap();
    assert!(m["script"].is_string());
    assert!(fs::read_to_string(dir.join(fl_core::servers::SCRIPT)).unwrap().contains("-Xmx3072M -jar purpur-1.21.4-"));
    c.dispatch(&a, "servers.remove", json!({ "id": r["id"], "delete_files": true })).await.unwrap();
    assert!(!dir.exists());
}
