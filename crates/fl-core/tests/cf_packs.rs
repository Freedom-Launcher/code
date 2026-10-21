//! Live CurseForge modpack plans: `FREEDOM_CF_API_KEY=… cargo test --test cf_packs -- --ignored --nocapture`.
use std::time::Instant;

use fl_core::{catalog, curseforge, modrinth, Core};

fn core() -> std::sync::Arc<Core> {
    let dir = std::env::temp_dir().join(format!("fl-cfp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    Core::new(dir, Instant::now(), false).unwrap()
}

#[tokio::test]
#[ignore]
async fn plans() {
    let c = core();
    // RLCraft, ATM9, Pixelmon, SkyFactory 4, Fabulously Optimized, BMC1, SevTech, ATM10, Prominence II
    for id in ["cf:285109", "cf:715572", "cf:389615", "cf:296062", "cf:396246", "cf:452013", "cf:268208", "cf:925200", "cf:466901"] {
        let vs = match catalog::project_versions(&c, id).await {
            Ok(v) => v,
            Err(e) => { println!("{id}: versions FAIL {e}"); continue; }
        };
        let Some(v) = modrinth::pick_pack(&vs, &["neoforge", "forge", "fabric", "quilt"]) else { println!("{id}: no version"); continue };
        let Some(f) = v.files.iter().find(|f| f.filename.ends_with(".zip")) else { println!("{id}: no zip"); continue };
        let t = Instant::now();
        let bytes = match c.http().get(&f.url).send().await.and_then(|r| r.error_for_status()) {
            Ok(r) => r.bytes().await.unwrap(),
            Err(e) => { println!("{id}: download FAIL {e} url={}", f.url); continue; }
        };
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        if !curseforge::is_pack(&mut z) { println!("{id}: {} not a CF pack", f.filename); continue; }
        let m = match curseforge::read_manifest(&mut z) { Ok(m) => m, Err(e) => { println!("{id}: manifest FAIL {e}"); continue } };
        let nfiles = m.files.len();
        match curseforge::pack_plan(&c, m).await {
            Ok(p) => println!("{id} {}: mc={} loader={}-{} files={} tasks={} blocked={} {:?} ({:.0}s)",
                p.name, p.mc, p.loader, p.loader_version, nfiles, p.tasks.len(), p.blocked.len(),
                p.blocked_files.iter().take(3).map(|b| format!("{} -> {} [{}]", b.title, b.url, b.rel)).collect::<Vec<_>>(), t.elapsed().as_secs_f32()),
            Err(e) => println!("{id} {}: PLAN FAIL {e}", f.filename),
        }
    }
}