//! Installs a loader instance for real (fabric / forge / neoforge).
//! `cargo run -p fl-core --example loader_smoke -- forge 1.21.4 /tmp/fl-loader`

use std::{sync::Arc, time::Instant};

use fl_core::{
    game::{download::Progress, install, loader, version::Env},
    instances::Instance,
    Core,
};

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let kind = a.next().unwrap_or_else(|| "fabric".into());
    let game = a.next().unwrap_or_else(|| "1.21.4".into());
    let dir = a.next().unwrap_or_else(|| "/tmp/fl-loader".into());
    let core = Core::new(dir.into(), Instant::now(), false).expect("core");
    let layout = install::Layout::new(&core);
    let lv = loader::latest(&core, &kind, &game).await.expect("loader list");
    let inst = Instance {
        id: "smoke".into(),
        name: "smoke".into(),
        version: game,
        loader: kind.clone(),
        loader_version: lv,
        created_at: 0,
        last_played: 0,
        playtime_secs: 0,
        settings: Default::default(),
    };
    let t = Instant::now();
    let v = loader::resolve(&core, &layout, &inst).await.expect("resolve");
    let p = Arc::new(Progress::default());
    let v = install::install(&core, v, Arc::clone(&p)).await.expect("install");
    if kind != "fabric" {
        fl_core::game::forge::post_install(&core, &layout, &inst, &v, p)
            .await
            .expect("processors");
    }
    println!("{} installed in {:?}", inst.version_id(), t.elapsed());
    println!("main class: {}  jar: {}", v.main_class, v.jar_id());
    let plan = install::library_plan(&v, &Env::current(), &layout.libraries());
    let missing: Vec<_> = plan.classpath.iter().filter(|p| !p.exists()).collect();
    println!("classpath: {} jars, missing: {:?}", plan.classpath.len(), missing);
    println!("local resolve ok: {}", loader::resolve_local(&layout, &inst).is_some());
}