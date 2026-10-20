//! Live CurseForge checks; run only with a key: `FREEDOM_CF_API_KEY=… cargo test --test curseforge_live -- --ignored`.
use std::time::Instant;

use fl_core::{catalog, curseforge, modrinth, Core};

fn core() -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join(format!("fl-cf-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    Core::new(dir, Instant::now(), false).unwrap()
}

#[tokio::test]
#[ignore]
async fn search_page_versions_install() {
    let c = core();
    let q = modrinth::Query {
        text: "jei",
        kind: modrinth::Kind::Mod,
        mc: "1.20.1",
        loaders: &["forge"],
        categories: &[],
        versions: &[],
        env: "",
        open_source: false,
        exclude: &[],
        sort: "relevance",
        offset: 0,
        limit: 5,
    };
    let page = curseforge::search(&c, &q).await.unwrap();
    assert!(page.total_hits > 0);
    let jei = page.hits.iter().find(|h| h.slug == "jei").expect("jei in results");
    assert_eq!(jei.project_id, "cf:238222");
    let p = catalog::project_page(&c, "cf:238222").await.unwrap();
    assert_eq!(p.slug, "jei");
    assert!(!p.body.is_empty());
    let vs = catalog::versions(&c, "cf:238222", "1.20.1", &["forge"]).await.unwrap();
    let v = modrinth::pick(&vs, "1.20.1", &["forge"]).expect("a forge 1.20.1 build");
    let f = v.primary_file().unwrap();
    assert_eq!(f.hashes.sha1.len(), 40);
    let one = catalog::version(&c, "cf:238222", &v.id).await.unwrap();
    assert_eq!(one.id, v.id);
    let cats = curseforge::categories(&c, modrinth::Kind::Mod).await.unwrap();
    assert!(cats.len() > 10);

    // A real install into a temp game dir.
    let gd = std::env::temp_dir().join(format!("fl-cf-gd-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&gd).unwrap();
    let t = fl_core::mods::Target { game_dir: &gd, kind: modrinth::Kind::Mod, mc: "1.20.1", loaders: &["forge"] };
    let added = fl_core::mods::install(&c, t, "cf:238222", None).await.unwrap();
    assert!(!added.is_empty());
    let list = fl_core::mods::list(&gd, modrinth::Kind::Mod).unwrap();
    assert!(list.iter().any(|m| m.source.as_ref().is_some_and(|s| s.project_id == "cf:238222")));
    let ups = fl_core::mods::updates(&c, t).await.unwrap();
    assert!(ups.is_empty(), "fresh install has no update: {ups:?}");
    let meta = fl_core::mods::refresh_meta(&c, &gd, modrinth::Kind::Mod).await.unwrap();
    assert!(meta.iter().any(|m| m.source.as_ref().is_some_and(|s| s.slug == "jei")));
}

#[tokio::test]
#[ignore]
async fn modpack_plan() {
    let c = core();
    let m: curseforge::Manifest = serde_json::from_value(serde_json::json!({
        "minecraft": { "version": "1.20.1", "modLoaders": [{ "id": "forge-47.2.0", "primary": true }] },
        "manifestType": "minecraftModpack", "name": "Test",
        "files": [{ "projectID": 238222, "fileID": 9268985, "required": true }]
    })).unwrap();
    let plan = curseforge::pack_plan(&c, m).await.unwrap();
    assert_eq!(plan.loader, "forge");
    assert_eq!(plan.tasks.len() + plan.blocked.len(), 1);
    if let Some(t) = plan.tasks.first() {
        assert!(t.path.starts_with("mods"));
        assert_eq!(plan.index[0].1.project_id, "cf:238222");
    }
}