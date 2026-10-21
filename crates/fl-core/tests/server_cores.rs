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
    // No core installed by the launcher: the launcher still starts it, with this memory.
    let r = c.dispatch(&Caller::BuiltinUi, "servers.add", json!({ "dir": srv.to_string_lossy() })).await.unwrap();
    let id = r["id"].as_str().unwrap().to_owned();
    assert!(c.dispatch(&a, "servers.memory.set", json!({ "id": id, "memory_mb": 100 })).await.is_err());
    c.dispatch(&a, "servers.memory.set", json!({ "id": id, "memory_mb": 3072 })).await.unwrap();
    let info = c.dispatch(&a, "servers.info", json!({ "id": id })).await.unwrap();
    assert_eq!(info["memory_mb"], 3072);
    assert_eq!(info["run"]["state"], "stopped");
    // Start rules: EULA first, then a server jar.
    let e = c.dispatch(&a, "servers.start", json!({ "id": id })).await.unwrap_err().to_string();
    assert!(e.contains("EULA"), "{e}");
    let e = c.dispatch(&a, "servers.start", json!({ "id": id, "eula": true })).await.unwrap_err().to_string();
    assert!(e.contains("jar"), "{e}");
    assert!(fs::read_to_string(srv.join("eula.txt")).unwrap().contains("eula=true"));
    assert!(c.dispatch(&a, "servers.stop", json!({ "id": id })).await.is_err());
    assert!(c.dispatch(&a, "servers.command", json!({ "id": id, "command": "list" })).await.is_err());
    let con = c.dispatch(&a, "servers.console", json!({ "id": id })).await.unwrap();
    assert_eq!(con["lines"], json!([]));
    // Java arguments files are out of reach for plugins.
    for f in ["user_jvm_args.txt", "libraries/net/x/1/unix_args.txt"] {
        assert!(c.dispatch(&a, "servers.files.write", json!({ "id": id, "path": f, "text": "-javaagent:x.jar" })).await.is_err(), "{f}");
    }
    // Backups: none before there is a world, then one, restore, delete.
    assert!(c.dispatch(&a, "servers.backup.create", json!({ "id": id })).await.is_err());
    fs::create_dir_all(srv.join("world/region")).unwrap();
    fs::write(srv.join("world/level.dat"), b"one").unwrap();
    let b = c.dispatch(&a, "servers.backup.create", json!({ "id": id })).await.unwrap();
    let name = b["name"].as_str().unwrap().to_owned();
    fs::write(srv.join("world/level.dat"), b"two").unwrap();
    let r = c.dispatch(&a, "servers.backup.restore", json!({ "id": id, "name": name })).await.unwrap();
    assert!(r["safety"].as_str().unwrap().contains("before-restore"));
    assert_eq!(fs::read(srv.join("world/level.dat")).unwrap(), b"one");
    let l = c.dispatch(&a, "servers.backups", json!({ "id": id })).await.unwrap();
    assert_eq!(l["backups"].as_array().unwrap().len(), 2);
    assert!(c.dispatch(&a, "servers.backup.delete", json!({ "id": id, "name": "../servers.json" })).await.is_err());
    c.dispatch(&a, "servers.backup.delete", json!({ "id": id, "name": name })).await.unwrap();
    assert_eq!(c.dispatch(&a, "servers.backups", json!({ "id": id })).await.unwrap()["backups"].as_array().unwrap().len(), 1);
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

/// Real servers through the launcher: start, console, a backup while running,
/// stop. `FL_CORES_DATA=<data with installed servers> FL_RUN=paper cargo test -- --ignored run_servers_online`.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn run_servers_online() {
    let data = PathBuf::from(std::env::var("FL_CORES_DATA").expect("FL_CORES_DATA"));
    let want = std::env::var("FL_RUN").unwrap_or_default();
    let c = Core::new(data, Instant::now(), false).unwrap();
    let a = addon();
    let list = c.dispatch(&a, "servers.list", Value::Null).await.unwrap();
    for s in list.as_array().unwrap() {
        let id = s["id"].as_str().unwrap().to_owned();
        let core = s["core"]["core"].as_str().unwrap_or("").to_owned();
        if !want.is_empty() && !want.split(',').any(|w| w == core) {
            continue;
        }
        let t0 = Instant::now();
        c.dispatch(&a, "servers.start", json!({ "id": id, "eula": true })).await.unwrap();
        let mut cursor = 0;
        let mut text = String::new();
        let mut state = String::new();
        while t0.elapsed().as_secs() < 400 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let v = c.dispatch(&a, "servers.console", json!({ "id": id, "from": cursor })).await.unwrap();
            cursor = v["cursor"].as_u64().unwrap();
            for l in v["lines"].as_array().unwrap() {
                text.push_str(l[1].as_str().unwrap());
                text.push('\n');
            }
            state = v["status"]["state"].as_str().unwrap().to_owned();
            if state == "running" || state == "crashed" {
                break;
            }
        }
        let ready = t0.elapsed().as_secs();
        assert_eq!(state, "running", "{core}: {}", text.lines().rev().take(12).collect::<Vec<_>>().join("\n"));
        assert!(c.dispatch(&a, "servers.start", json!({ "id": id })).await.is_err(), "no double start");
        c.dispatch(&a, "servers.command", json!({ "id": id, "command": "/list" })).await.unwrap();
        let b = c.dispatch(&a, "servers.backup.create", json!({ "id": id })).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let v = c.dispatch(&a, "servers.console", json!({ "id": id, "from": cursor })).await.unwrap();
        let tail: Vec<&str> = v["lines"].as_array().unwrap().iter().filter_map(|l| l[1].as_str()).collect();
        assert!(tail.iter().any(|l| l.contains("players online")), "{core}: list answered: {tail:?}");
        assert!(c.dispatch(&a, "servers.core.install", json!({ "id": id, "core": "paper", "mc": "1.21.4" })).await.is_err());
        c.dispatch(&a, "servers.stop", json!({ "id": id })).await.unwrap();
        let t1 = Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let st = c.dispatch(&a, "servers.status", json!({ "id": id })).await.unwrap();
            if st["state"] == "stopped" {
                break;
            }
            assert!(t1.elapsed().as_secs() < 100, "{core}: did not stop: {st}");
        }
        c.dispatch(&a, "servers.backup.delete", json!({ "id": id, "name": b["name"] })).await.unwrap();
        println!("{core}: ready {ready}s, backup {} KB of {:?}, stopped in {}s", b["size"].as_u64().unwrap() / 1024, b["worlds"], t1.elapsed().as_secs());
    }
}
