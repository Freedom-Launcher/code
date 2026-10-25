//! Live creator-page checks against Modrinth (and CurseForge with a key):
//! `cargo test -p fl-core --test people_live -- --ignored`.
use std::time::Instant;

use fl_core::{people, Core};

fn core() -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join(format!("fl-people-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    Core::new(dir, Instant::now(), false).unwrap()
}

#[tokio::test]
#[ignore]
async fn modrinth_creators() {
    let c = core();
    let cr = people::credits(&c, "sodium").await.unwrap();
    assert_eq!(cr["org"]["name"], "CaffeineMC");
    assert!(cr["members"].as_array().unwrap().len() >= 2);
    let cr = people::credits(&c, "iris").await.unwrap();
    assert!(cr["org"].is_null());
    assert_eq!(cr["members"][0]["role"], "Owner");
    let o = people::profile(&c, "org", "caffeinemc").await.unwrap();
    assert!(o["projects"].as_array().unwrap().iter().any(|p| p["slug"] == "sodium"));
    assert!(o["members"].as_array().unwrap().len() >= 2);
    let u = people::profile(&c, "user", "coderbot").await.unwrap();
    assert!(u["projects"].as_array().unwrap().iter().any(|p| p["slug"] == "iris"));
    assert!(u["downloads"].as_u64().unwrap() > 1_000_000);
    assert!(people::profile(&c, "user", "../x").await.is_err());
}

#[tokio::test]
#[ignore]
async fn curseforge_author() {
    if !fl_core::curseforge::available() {
        return;
    }
    let c = core();
    let cr = people::credits(&c, "cf:238222").await.unwrap();
    assert_eq!(cr["members"][0]["name"], "mezz");
    let p = people::profile(&c, "cf", "17072262").await.unwrap();
    assert_eq!(p["creator"]["name"], "mezz");
    assert!(p["projects"].as_array().unwrap().iter().any(|w| w["project_id"] == "cf:238222"));
}