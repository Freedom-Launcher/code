//! Building the game command line and starting the process.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use super::{
    install::{library_plan, Layout},
    version::{Env, VersionJson},
};
use crate::CoreError;

/// Who is playing. `access_token` is never logged.
#[derive(Debug, Clone)]
pub struct LaunchAuth {
    pub name: String,
    /// UUID without dashes.
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
    /// `msa` for Microsoft accounts, `legacy` for local profiles.
    pub user_type: String,
}

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub java: PathBuf,
    pub game_dir: PathBuf,
    pub memory_mb: u32,
    pub extra_jvm_args: Vec<String>,
    pub client_id: String,
    pub resolution: Option<(u32, u32)>,
}

/// Default heap from total RAM: under 8 GB → 2 GB, 24 GB or more → 6 GB,
/// otherwise 4 GB. Unknown RAM → 4 GB.
pub fn default_memory_mb(total_ram_mb: Option<u64>) -> u32 {
    match total_ram_mb {
        Some(r) if r < 8 * 1024 => 2048,
        Some(r) if r >= 24 * 1024 => 6144,
        _ => 4096,
    }
}

/// Total physical memory in MB, when the OS tells us.
pub fn total_ram_mb() -> Option<u64> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut s: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        s.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: `s` is a properly sized, initialised MEMORYSTATUSEX.
        if unsafe { GlobalMemoryStatusEx(&mut s) } != 0 {
            return Some(s.ullTotalPhys / (1024 * 1024));
        }
        None
    }
    #[cfg(not(windows))]
    {
        let info = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = info
            .lines()
            .find(|l| l.starts_with("MemTotal:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        Some(kb / 1024)
    }
}

fn fill(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = s.to_owned();
    for (k, v) in vars {
        out = out.replace(&format!("${{{k}}}"), v);
    }
    out
}

/// Full argument list after `java` (JVM args, main class, game args).
pub fn build_args(
    v: &VersionJson,
    env: &Env,
    layout: &Layout,
    auth: &LaunchAuth,
    opts: &LaunchOptions,
) -> Vec<String> {
    let sep = if env.os == "windows" { ";" } else { ":" };
    let mut classpath: Vec<PathBuf> = library_plan(v, env, &layout.libraries()).classpath;
    classpath.push(layout.client_jar(v.jar_id()));
    let classpath = classpath
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    let assets_root = layout.assets();
    let game_assets = if v.assets_id() == "legacy" || v.assets_id() == "pre-1.6" {
        opts.game_dir.join("resources")
    } else {
        assets_root.join("virtual").join(v.assets_id())
    };
    let p = |x: &Path| x.display().to_string();
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", auth.name.clone()),
        ("auth_uuid", auth.uuid.clone()),
        ("auth_access_token", auth.access_token.clone()),
        (
            "auth_session",
            format!("token:{}:{}", auth.access_token, auth.uuid),
        ),
        ("auth_xuid", auth.xuid.clone()),
        ("clientid", opts.client_id.clone()),
        ("user_type", auth.user_type.clone()),
        ("user_properties", "{}".into()),
        ("version_name", v.id.clone()),
        (
            "version_type",
            if v.kind.is_empty() {
                "release".into()
            } else {
                v.kind.clone()
            },
        ),
        ("game_directory", p(&opts.game_dir)),
        ("assets_root", p(&assets_root)),
        ("game_assets", p(&game_assets)),
        ("assets_index_name", v.assets_id().to_owned()),
        ("natives_directory", p(&layout.natives_dir(&v.id))),
        ("library_directory", p(&layout.libraries())),
        ("classpath", classpath),
        ("classpath_separator", sep.into()),
        ("resolution_width", opts.resolution.map_or(String::new(), |r| r.0.to_string())),
        ("resolution_height", opts.resolution.map_or(String::new(), |r| r.1.to_string())),
        ("launcher_name", "freedom-launcher".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
    ]);

    let mut args = vec![format!("-Xmx{}M", opts.memory_mb), "-Xms512M".to_owned()];
    match &v.arguments {
        Some(a) if !a.jvm.is_empty() => {
            args.extend(
                a.jvm
                    .iter()
                    .flat_map(|x| x.values(env))
                    .map(|s| fill(s, &vars)),
            );
        }
        _ => {
            if env.os == "windows" {
                args.push(
                    "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"
                        .into(),
                );
            }
            args.push(fill("-Djava.library.path=${natives_directory}", &vars));
            args.push("-cp".into());
            args.push(vars["classpath"].clone());
        }
    }
    if let Some(l) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let path = p(&layout.log_config(&l.file.id));
        args.push(l.argument.replace("${path}", &path));
    }
    args.extend(opts.extra_jvm_args.iter().cloned());
    args.push(v.main_class.clone());

    match (&v.minecraft_arguments, &v.arguments) {
        (_, Some(a)) if !a.game.is_empty() => {
            args.extend(
                a.game
                    .iter()
                    .flat_map(|x| x.values(env))
                    .map(|s| fill(s, &vars)),
            );
        }
        (Some(legacy), _) => args.extend(legacy.split_whitespace().map(|s| fill(s, &vars))),
        _ => {}
    }
    // Old versions have no resolution rule; the game still reads the flags.
    if v.arguments.as_ref().map_or(true, |a| a.game.is_empty()) {
        if let Some((w, h)) = opts.resolution {
            args.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
        }
    }
    args
}

/// Starts the game. Output goes to `<game_dir>/logs/launcher-output.log`.
pub fn spawn(java: &Path, args: &[String], game_dir: &Path) -> Result<Child, CoreError> {
    std::fs::create_dir_all(game_dir.join("logs"))?;
    let log = std::fs::File::create(game_dir.join("logs").join("launcher-output.log"))?;
    let err = log.try_clone()?;
    let mut cmd = Command::new(java);
    cmd.args(args)
        .current_dir(game_dir)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
        .map_err(|e| CoreError::msg(format!("could not start Java ({}): {e}", java.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn win() -> Env {
        Env {
            os: "windows",
            arch: "x86_64",
            os_version: "10.0".into(),
            features: BTreeMap::new(),
        }
    }

    fn auth() -> LaunchAuth {
        LaunchAuth {
            name: "Steve".into(),
            uuid: "abc".into(),
            access_token: "tok".into(),
            xuid: "1".into(),
            user_type: "msa".into(),
        }
    }

    fn opts() -> LaunchOptions {
        LaunchOptions {
            java: "/j/javaw.exe".into(),
            game_dir: "/g".into(),
            memory_mb: 4096,
            resolution: None,
            extra_jvm_args: vec![],
            client_id: String::new(),
        }
    }

    #[test]
    fn heap_heuristic() {
        assert_eq!(default_memory_mb(Some(6 * 1024)), 2048);
        assert_eq!(default_memory_mb(Some(16 * 1024)), 4096);
        assert_eq!(default_memory_mb(Some(32 * 1024)), 6144);
        assert_eq!(default_memory_mb(None), 4096);
    }

    #[test]
    fn legacy_arguments() {
        let v: VersionJson = serde_json::from_value(json!({
            "id": "1.8.9", "mainClass": "net.minecraft.client.main.Main", "assets": "1.8",
            "minecraftArguments": "--username ${auth_player_name} --accessToken ${auth_access_token} --version ${version_name}"
        }))
        .unwrap();
        let layout = Layout {
            root: "/d/game".into(),
        };
        let a = build_args(&v, &win(), &layout, &auth(), &opts());
        assert_eq!(a[0], "-Xmx4096M");
        assert!(a
            .iter()
            .any(|x| x == "-Djava.library.path=/d/game/versions/1.8.9/natives"));
        let main = a
            .iter()
            .position(|x| x == "net.minecraft.client.main.Main")
            .unwrap();
        assert_eq!(
            a[main + 1..],
            [
                "--username",
                "Steve",
                "--accessToken",
                "tok",
                "--version",
                "1.8.9"
            ]
        );
    }

    #[test]
    fn modern_arguments_with_logging() {
        let v: VersionJson = serde_json::from_value(json!({
            "id": "1.21", "mainClass": "M", "type": "release",
            "assetIndex": {"id": "17", "sha1": "a", "url": "https://e"},
            "arguments": {
                "game": ["--assetIndex", "${assets_index_name}", "--uuid", "${auth_uuid}"],
                "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
            },
            "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}",
                "type": "log4j2-xml", "file": {"id": "client-1.12.xml", "sha1": "b", "url": "https://e"}}}
        }))
        .unwrap();
        let layout = Layout {
            root: "/d/game".into(),
        };
        let a = build_args(&v, &win(), &layout, &auth(), &opts());
        let cp = a.iter().position(|x| x == "-cp").unwrap();
        assert_eq!(a[cp + 1], "/d/game/versions/1.21/1.21.jar");
        assert!(a.contains(
            &"-Dlog4j.configurationFile=/d/game/assets/log_configs/client-1.12.xml".to_owned()
        ));
        assert!(a.ends_with(&[
            "M".into(),
            "--assetIndex".into(),
            "17".into(),
            "--uuid".into(),
            "abc".into()
        ]));
    }
}
