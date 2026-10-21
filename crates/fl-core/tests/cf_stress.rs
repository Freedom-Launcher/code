//! Broad live CurseForge sweep: `FREEDOM_CF_API_KEY=… cargo test --test cf_stress -- --ignored --nocapture`.
use std::time::Instant;

use fl_core::{catalog, curseforge, modrinth, Core};
use modrinth::Kind;

fn core() -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join(format!("fl-cfs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    Core::new(dir, Instant::now(), false).unwrap()
}

async fn top(c: &Core, kind: Kind, mc: &str, loaders: &[&str], n: u32) -> Vec<modrinth::Hit> {
    let q = modrinth::Query {
        text: "",
        kind,
        mc,
        loaders,
        categories: &[],
        versions: &[],
        env: "",
        open_source: false,
        exclude: &[],
        sort: "downloads",
        offset: 0,
        limit: n,
    };
    match curseforge::search(c, &q).await {
        Ok(p) => p.hits,
        Err(e) => {
            println!("SEARCH FAIL {kind:?} {mc} {loaders:?}: {e}");
            vec![]
        }
    }
}

#[tokio::test]
#[ignore]
async fn sweep() {
    let c = core();
    let combos: &[(Kind, &str, &[&str])] = &[
        (Kind::Mod, "1.20.1", &["forge"]),
        (Kind::Mod, "1.21.1", &["neoforge"]),
        (Kind::Mod, "1.21.1", &["fabric"]),
        (Kind::ResourcePack, "1.21.1", &["minecraft"]),
        (Kind::Shader, "1.21.1", &["iris", "optifine"]),
    ];
    let mut fails = 0;
    let mut total = 0;
    for (kind, mc, loaders) in combos {
        let hits = top(&c, *kind, mc, loaders, 15).await;
        println!("== {kind:?} {mc} {loaders:?}: {} hits", hits.len());
        for h in hits {
            total += 1;
            let gd = std::env::temp_dir().join(format!("fl-cfs-gd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&gd).unwrap();
            let t = fl_core::mods::Target { game_dir: &gd, kind: *kind, mc, loaders };
            match fl_core::mods::install(&c, t, &h.project_id, None).await {
                Ok(a) => println!("  ok   {} ({}) +{}", h.title, h.project_id, a.len()),
                Err(e) => {
                    fails += 1;
                    println!("  FAIL {} ({}): {e}", h.title, h.project_id);
                }
            }
            let _ = std::fs::remove_dir_all(&gd);
        }
    }
    // Modpacks: version choice + pack file presence (no full download).
    let packs = top(&c, Kind::Modpack, "", &[], 15).await;
    println!("== modpacks: {} hits", packs.len());
    for h in packs {
        total += 1;
        match catalog::project_versions(&c, &h.project_id).await {
            Ok(vs) => {
                let order = ["neoforge", "forge", "fabric", "quilt"];
                match modrinth::pick_pack(&vs, &order) {
                    Some(v) => {
                        let f = v.files.iter().find(|f| f.filename.ends_with(".zip") && curseforge::download_ok(&f.url));
                        match f {
                            Some(f) => println!("  ok   {} {} loaders={:?} {} {}MB", h.title, v.id, v.loaders, f.filename, f.size / 1_000_000),
                            None => {
                                fails += 1;
                                println!("  FAIL {} {}: no pack zip; loaders={:?} files={:?} versions={}", h.title, v.id, v.loaders, v.files.iter().map(|f| &f.filename).collect::<Vec<_>>(), vs.len());
                            }
                        }
                    }
                    None => {
                        fails += 1;
                        println!("  FAIL {}: no version among {}", h.title, vs.len());
                    }
                }
            }
            Err(e) => {
                fails += 1;
                println!("  FAIL {}: versions: {e}", h.title);
            }
        }
    }
    println!("TOTAL {total} FAILS {fails}");
}