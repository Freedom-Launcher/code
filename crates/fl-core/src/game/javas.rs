//! Java installations on this computer: discovery, probing and the
//! recommended major version per Minecraft release (Э5).
//!
//! Probing prefers the JDK/JRE `release` file (no process); only when it is
//! missing is `java -version` run.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Serialize;

use super::{java, version::Env};
use crate::Core;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JavaInstall {
    pub path: String,
    pub major: u32,
    pub version: String,
    pub vendor: String,
    pub arch: String,
    /// Installed by the launcher under `<data_dir>/java`.
    pub managed: bool,
}

/// `java` binary name used to launch the game.
fn exe_name() -> &'static str {
    if cfg!(windows) {
        "javaw.exe"
    } else {
        "java"
    }
}

/// `1.8.0_382` → 8, `17.0.8` → 17, `21` → 21.
pub fn major_of(version: &str) -> Option<u32> {
    let mut it = version.trim().split(['.', '_', '-', '+']);
    let first: u32 = it.next()?.parse().ok()?;
    if first == 1 {
        it.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches('"').to_owned()
}

/// Reads `JAVA_VERSION`, `IMPLEMENTOR`, `OS_ARCH` from a `release` file.
pub fn parse_release(text: &str) -> Option<(String, String, String)> {
    let get = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('=').map(unquote))
            .unwrap_or_default()
    };
    let v = get("JAVA_VERSION");
    (!v.is_empty()).then(|| (v, get("IMPLEMENTOR"), get("OS_ARCH")))
}

/// Version from `java -version` output: `openjdk version "17.0.8" …`.
pub fn parse_version_output(out: &str) -> Option<String> {
    let line = out.lines().find(|l| l.contains("version \""))?;
    let start = line.find('"')? + 1;
    let end = start + line[start..].find('"')?;
    Some(line[start..end].to_owned())
}

/// Vendor guess from `java -version` output when there is no release file.
fn vendor_from_output(out: &str) -> String {
    let l = out.to_lowercase();
    for (k, v) in [
        ("temurin", "Eclipse Adoptium"),
        ("zulu", "Azul Systems"),
        ("corretto", "Amazon"),
        ("microsoft", "Microsoft"),
        ("graalvm", "GraalVM"),
        ("liberica", "BellSoft"),
        ("openj9", "IBM Semeru"),
    ] {
        if l.contains(k) {
            return v.into();
        }
    }
    if l.contains("java(tm)") {
        "Oracle".into()
    } else {
        String::new()
    }
}

/// Java home for an executable: `<home>/bin/java`.
fn home_of(exe: &Path) -> Option<&Path> {
    exe.parent()?.parent()
}

fn run_version(exe: &Path) -> Option<String> {
    // `javaw` has no console output on Windows; probe its `java.exe` sibling.
    let probe = exe.with_file_name(if cfg!(windows) { "java.exe" } else { "java" });
    let probe = if probe.is_file() {
        probe
    } else {
        exe.to_path_buf()
    };
    let mut cmd = Command::new(probe);
    cmd.arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    Some(String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Describes the Java at `exe`, or `None` if it is not a working Java.
pub fn probe(exe: &Path, managed_root: &Path) -> Option<JavaInstall> {
    if !exe.is_file() {
        return None;
    }
    let release = home_of(exe).and_then(|h| fs::read_to_string(h.join("release")).ok());
    let (version, vendor, arch) = match release.as_deref().and_then(parse_release) {
        Some(r) => r,
        None => {
            let out = run_version(exe)?;
            let arch = if out.contains("64-Bit") { "x86_64" } else { "" };
            (
                parse_version_output(&out)?,
                vendor_from_output(&out),
                arch.into(),
            )
        }
    };
    Some(JavaInstall {
        path: exe.to_string_lossy().into_owned(),
        major: major_of(&version)?,
        version,
        vendor,
        arch,
        managed: exe.starts_with(managed_root),
    })
}

/// Sub-directories of `dir` (missing dir → none).
fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}

/// Java homes in the usual places for this OS.
fn system_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if let Some(h) = env("JAVA_HOME") {
        homes.push(h);
    }
    if cfg!(windows) {
        for pf in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .into_iter()
            .filter_map(env)
        {
            for vendor in [
                "Java",
                "Eclipse Adoptium",
                "Eclipse Foundation",
                "AdoptOpenJDK",
                "Zulu",
                "Microsoft",
                "Amazon Corretto",
                "BellSoft",
                "Semeru",
                "OpenJDK",
            ] {
                homes.extend(subdirs(&pf.join(vendor)));
            }
        }
        // Official launcher runtimes: runtime/<component>/<platform>/<component>.
        let mut rt: Vec<PathBuf> = env("APPDATA")
            .map(|a| a.join(".minecraft/runtime"))
            .into_iter()
            .collect();
        rt.extend(env("LOCALAPPDATA").map(|l| {
            l.join("Packages/Microsoft.4297127D64EC6_8wekyb3d8bbwe/LocalCache/Local/runtime")
        }));
        for root in rt {
            for comp in subdirs(&root) {
                for plat in subdirs(&comp) {
                    homes.extend(subdirs(&plat));
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        let mut roots = vec![PathBuf::from("/Library/Java/JavaVirtualMachines")];
        roots.extend(env("HOME").map(|h| h.join("Library/Java/JavaVirtualMachines")));
        for root in roots {
            homes.extend(subdirs(&root).into_iter().map(|d| d.join("Contents/Home")));
        }
    } else {
        for root in [
            "/usr/lib/jvm",
            "/usr/lib64/jvm",
            "/usr/java",
            "/opt/java",
            "/opt/jdk",
        ] {
            homes.extend(subdirs(Path::new(root)));
        }
        if let Some(h) = env("HOME") {
            homes.extend(subdirs(&h.join(".sdkman/candidates/java")));
            homes.extend(subdirs(&h.join(".jdks")));
        }
    }
    homes
}

/// Every working Java found on this computer, newest major first.
pub fn list(core: &Core) -> Vec<JavaInstall> {
    let managed_root = core.data_dir.join("java");
    let env = Env::current();
    let mut exes: Vec<PathBuf> = subdirs(&managed_root)
        .iter()
        .map(|d| java::java_exe(d, &env))
        .collect();
    exes.extend(
        system_homes()
            .iter()
            .map(|h| h.join("bin").join(exe_name())),
    );
    if let Some(path) = std::env::var_os("PATH") {
        exes.extend(std::env::split_paths(&path).map(|d| d.join(exe_name())));
    }
    let mut seen = BTreeSet::new();
    let mut out: Vec<JavaInstall> = exes
        .into_iter()
        .filter_map(|e| {
            let real = fs::canonicalize(&e).ok()?;
            seen.insert(real).then_some(e)
        })
        .filter_map(|e| probe(&e, &managed_root))
        .collect();
    out.sort_by(|a, b| b.major.cmp(&a.major).then(b.managed.cmp(&a.managed)));
    out
}

/// Java major a Minecraft release needs (Mojang's `javaVersion`, by release).
pub fn required_major(mc: &str) -> u32 {
    let mut it = mc.split(['.', '-', ' ']);
    let (Some(a), Some(b)) = (it.next(), it.next()) else {
        return 21;
    };
    let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) else {
        return 21;
    };
    if a > 1 {
        return 25; // year-based releases (26.x)
    }
    let patch: u32 = it.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    match b {
        0..=16 => 8,
        17 => 16,
        18..=19 => 17,
        20 if patch < 5 => 17,
        _ => 21,
    }
}

/// Mojang runtime component for a Java major (managed install).
pub fn component_for_major(major: u32) -> &'static str {
    match major {
        0..=8 => "jre-legacy",
        9..=16 => "java-runtime-alpha",
        17..=20 => "java-runtime-gamma",
        21..=24 => "java-runtime-delta",
        _ => "java-runtime-epsilon",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn majors() {
        assert_eq!(major_of("1.8.0_382"), Some(8));
        assert_eq!(major_of("17.0.8"), Some(17));
        assert_eq!(major_of("21"), Some(21));
        assert_eq!(major_of("21.0.2+13-LTS"), Some(21));
        assert_eq!(major_of("x"), None);
    }

    #[test]
    fn release_and_output() {
        let r = "IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"17.0.8\"\nOS_ARCH=\"amd64\"\n";
        assert_eq!(
            parse_release(r),
            Some(("17.0.8".into(), "Eclipse Adoptium".into(), "amd64".into()))
        );
        assert_eq!(parse_release("FOO=1"), None);
        let o = "openjdk version \"21.0.2\" 2024-01-16 LTS\nOpenJDK Runtime Environment Temurin-21.0.2+13 (build 21.0.2+13-LTS)\nOpenJDK 64-Bit Server VM";
        assert_eq!(parse_version_output(o).as_deref(), Some("21.0.2"));
        assert_eq!(vendor_from_output(o), "Eclipse Adoptium");
    }

    #[test]
    fn required() {
        assert_eq!(required_major("1.12.2"), 8);
        assert_eq!(required_major("1.16.5"), 8);
        assert_eq!(required_major("1.17.1"), 16);
        assert_eq!(required_major("1.18.2"), 17);
        assert_eq!(required_major("1.20.4"), 17);
        assert_eq!(required_major("1.20.5"), 21);
        assert_eq!(required_major("1.21.4"), 21);
        assert_eq!(required_major("26.1"), 25);
        assert_eq!(required_major("24w14a"), 21);
    }
}
