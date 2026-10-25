//! Exposes the launcher version (the one in `app/tauri.conf.json`) as `FL_VERSION`,
//! so logs, diagnostics and the User-Agent show the real release, not the crate version.
fn main() {
    let conf = "../../app/tauri.conf.json";
    println!("cargo:rerun-if-changed={conf}");
    let v = std::fs::read_to_string(conf)
        .ok()
        .and_then(|s| {
            let i = s.find("\"version\"")?;
            let rest = &s[i + 9..];
            let a = rest.find('"')? + 1;
            let b = rest[a..].find('"')?;
            Some(rest[a..a + b].to_owned())
        })
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=FL_VERSION={v}");
}
