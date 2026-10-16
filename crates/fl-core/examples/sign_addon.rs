//! Signs a package folder with a project key, writing `signature.json`:
//! `cargo run -p fl-core --example sign_addon -- <folder> <seed file> [verified|official] [key id]`
//! The seed file holds the 32-byte Ed25519 private key as hex; keep it out of the repo.

use std::path::PathBuf;

use fl_core::{signing, Trust};

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 2 {
        eprintln!("usage: sign_addon <folder> <seed file> [verified|official] [key id]");
        std::process::exit(2);
    }
    let dir = PathBuf::from(&a[0]);
    let id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .expect("folder name")
        .to_owned();
    let seed = std::fs::read_to_string(&a[1]).expect("seed file");
    let seed = signing::unhex::<32>(&seed).expect("seed: 64 hex characters");
    let badge = match a.get(2).map(String::as_str).unwrap_or("verified") {
        "official" => Trust::Official,
        "verified" => Trust::Verified,
        b => panic!("unknown badge {b}"),
    };
    let key = a.get(3).map(String::as_str).unwrap_or(signing::KEYS[0].0);
    let s = signing::sign(&dir, &id, badge, key, &seed).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        dir.join(signing::FILE),
        serde_json::to_string_pretty(&s).unwrap(),
    )
    .unwrap();
    println!("{id}: {:?}", signing::check(&dir, &id));
}
