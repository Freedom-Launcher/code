//! Manual smoke test: `cargo run -p fl-core --example install_smoke -- 1.21.1 /tmp/fl-smoke`
//! Installs a vanilla version plus its Java runtime and prints the result.

use std::{sync::Arc, time::Instant};

use fl_core::{
    game::{
        download::Progress,
        install, java,
        launch::{self, LaunchAuth, LaunchOptions},
        version::Env,
    },
    Core,
};

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let id = a.next().unwrap_or_else(|| "1.21.1".into());
    let dir = a.next().unwrap_or_else(|| "/tmp/fl-smoke".into());
    let core = Core::new(dir.into(), Instant::now(), false).expect("core");
    let p = Arc::new(Progress::default());
    let base = install::load_version(&core, &install::Layout::new(&core), &id)
        .await
        .expect("version json");
    let t = Instant::now();
    let v = install::install(&core, base.clone(), Arc::clone(&p))
        .await
        .expect("install");
    println!(
        "game {} installed in {:?}: {}",
        v.id,
        t.elapsed(),
        p.snapshot()
    );
    let t = Instant::now();
    let exe = java::ensure(&core, v.java_version.as_ref(), Arc::clone(&p))
        .await
        .expect("java");
    println!("java at {} in {:?}", exe.display(), t.elapsed());
    let t = Instant::now();
    install::install(&core, base, Arc::new(Progress::default()))
        .await
        .expect("reinstall");
    println!("re-install (full verify) took {:?}", t.elapsed());

    // Start the game in official demo mode for ~25 s to check the command line.
    let layout = install::Layout::new(&core);
    let mut env = Env::current();
    env.features.insert("is_demo_user".into(), true);
    let auth = LaunchAuth {
        name: "Demo".into(),
        uuid: "00000000000000000000000000000000".into(),
        access_token: "0".into(),
        xuid: String::new(),
        user_type: "msa".into(),
    };
    let opts = LaunchOptions {
        java: exe.clone(),
        game_dir: core.data_dir.join("instances").join("smoke"),
        memory_mb: launch::default_memory_mb(launch::total_ram_mb()),
        extra_jvm_args: vec![],
        client_id: String::new(),
    };
    let args = launch::build_args(&v, &env, &layout, &auth, &opts);
    let mut child = launch::spawn(&exe, &args, &opts.game_dir).expect("spawn");
    std::thread::sleep(std::time::Duration::from_secs(25));
    println!(
        "game still running after 25 s: {}",
        child.try_wait().unwrap().is_none()
    );
    let _ = child.kill();
}
