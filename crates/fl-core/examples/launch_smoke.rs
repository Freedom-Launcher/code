//! Installs and starts a loader instance in demo mode, then prints the tail
//! of the game log. `cargo run -p fl-core --example launch_smoke -- forge 1.21.4 /tmp/fl-ls`

use std::{sync::Arc, time::Instant};

use fl_core::{
    game::{
        download::Progress,
        install, java,
        launch::{self, LaunchAuth, LaunchOptions},
        loader,
        version::Env,
    },
    instances::Instance,
    Core,
};

#[tokio::main]
async fn main() {
    let mut a = std::env::args().skip(1);
    let kind = a.next().unwrap_or_else(|| "fabric".into());
    let game = a.next().unwrap_or_else(|| "1.21.4".into());
    let dir = a.next().unwrap_or_else(|| "/tmp/fl-ls".into());
    let secs: u64 = a.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let core = Core::new(dir.into(), Instant::now(), false).expect("core");
    let layout = install::Layout::new(&core);
    let lv = if kind == "vanilla" {
        String::new()
    } else {
        loader::latest(&core, &kind, &game)
            .await
            .expect("loader list")
    };
    let inst = Instance {
        id: "smoke".into(),
        name: "smoke".into(),
        dir: String::new(),
        version: game,
        loader: kind.clone(),
        loader_version: lv,
        created_at: 0,
        last_played: 0,
        playtime_secs: 0,
        settings: Default::default(),
    };
    let v = loader::resolve(&core, &layout, &inst)
        .await
        .expect("resolve");
    let p = Arc::new(Progress::default());
    let v = install::install(&core, v, Arc::clone(&p))
        .await
        .expect("install");
    if kind == "forge" || kind == "neoforge" {
        fl_core::game::forge::post_install(&core, &layout, &inst, &v, Arc::clone(&p))
            .await
            .expect("processors");
    }
    let exe = java::ensure(&core, v.java_version.as_ref(), p)
        .await
        .expect("java");
    println!("{} ready, java {}", v.id, exe.display());

    let mut env = Env::current();
    env.features.insert("is_demo_user".into(), true);
    let auth = LaunchAuth {
        name: "Demo".into(),
        uuid: "00000000000000000000000000000000".into(),
        access_token: "0".into(),
        xuid: String::new(),
        user_type: "msa".into(),
    };
    let game_dir = core
        .data_dir
        .join("instances")
        .join(format!("smoke-{kind}"));
    let opts = LaunchOptions {
        java: exe.clone(),
        game_dir: game_dir.clone(),
        memory_mb: 2048,
        extra_jvm_args: vec![],
        client_id: String::new(),
        resolution: None,
    };
    let args = launch::build_args(&v, &env, &layout, &auth, &opts);
    std::fs::write(game_dir.join("args.txt"), args.join("\n")).ok();
    let mut child = launch::spawn(&exe, &args, &game_dir).expect("spawn");
    let t = Instant::now();
    let code = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if t.elapsed().as_secs() >= secs {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    };
    match code {
        Some(s) => println!("EXITED after {:?}: {s}", t.elapsed()),
        None => {
            println!("STILL RUNNING after {secs} s");
            let _ = child.kill();
        }
    }
    println!(
        "log: {}",
        game_dir.join("logs/launcher-output.log").display()
    );
}
