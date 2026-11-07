//! `secrets_leave_the_file_and_come_back` needs a desktop session with an unlocked default
//! keyring (GNOME Keyring, KWallet, KeePassXC), so it is `#[ignore]`d. Run it by hand on such a
//! machine: `cargo test -p fl-core --test keychain_live -- --ignored --test-threads=1`.
//! A bare daemon without a default collection answers `NoStorageAccess`; the launcher then
//! treats the keychain as unavailable and keeps secrets in the file, which the second test
//! (and every other test of accounts) covers.
#![cfg(target_os = "linux")]

use fl_core::accounts::{self, Account, AccountKind, AccountStore, Secrets};

#[test]
#[ignore]
fn secrets_leave_the_file_and_come_back() {
    assert!(fl_core::keychain::available(), "no Secret Service on this session bus");
    let dir = std::env::temp_dir().join(format!("fl-kc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("accounts.json");
    let id = format!("kc-test-{}", std::process::id());
    let mut s = AccountStore::default();
    s.upsert(Account {
        id: id.clone(),
        kind: AccountKind::Microsoft,
        name: "Steve".into(),
        uuid: "0f1e2d3c-4b5a-4978-8a69-5a4b3c2d1e0f".into(),
        owns_game: true,
        secrets: Some(Secrets {
            refresh_token: "REFRESH-SECRET-VALUE".into(),
            mc_access_token: "ACCESS-SECRET-VALUE".into(),
            mc_expires_at: 4_000_000_000,
        }),
    });
    accounts::save_store(&path, &s).unwrap();

    // Nothing secret is left on disk...
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert!(!on_disk.contains("SECRET-VALUE"), "secrets leaked into accounts.json: {on_disk}");
    // ...and they come back from the keychain.
    let back = accounts::load_store(&path).unwrap();
    let sec = back.accounts[0].secrets.as_ref().expect("secrets read back");
    assert_eq!(sec.refresh_token, "REFRESH-SECRET-VALUE");
    assert_eq!(sec.mc_access_token, "ACCESS-SECRET-VALUE");
    assert_eq!(sec.mc_expires_at, 4_000_000_000);

    // Removing the account removes its secrets too.
    let mut s = back;
    s.remove(&id);
    accounts::save_store(&path, &s).unwrap();
    assert!(fl_core::keychain::get(&id).is_none(), "secrets must be deleted with the account");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without a keychain (this is what a bare session looks like) the file keeps working.
#[test]
fn without_a_keychain_the_file_keeps_the_secrets() {
    std::env::set_var("FL_NO_KEYCHAIN", "1");
    assert!(!fl_core::keychain::available());
    let dir = std::env::temp_dir().join(format!("fl-kc2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("accounts.json");
    let mut s = AccountStore::default();
    s.upsert(Account {
        id: "a".into(),
        kind: AccountKind::Microsoft,
        name: "Steve".into(),
        uuid: "0f1e2d3c-4b5a-4978-8a69-5a4b3c2d1e0f".into(),
        owns_game: true,
        secrets: Some(Secrets { refresh_token: "R".into(), mc_access_token: "A".into(), mc_expires_at: 4_000_000_000 }),
    });
    accounts::save_store(&path, &s).unwrap();
    let back = accounts::load_store(&path).unwrap();
    assert_eq!(back.accounts[0].secrets.as_ref().unwrap().mc_access_token, "A");
    let _ = std::fs::remove_dir_all(&dir);
}
