# Releasing

The updater reads `https://github.com/Freedom-Launcher/releases/releases/latest/download/latest.json`. A release is a
signed Windows installer plus that file, published as Release `v<version>` in `Freedom-Launcher/releases`.
The installer must be signed with the Tauri updater private key (its public key is in `app/tauri.conf.json`);
a build signed with any other key is refused by every installed launcher, so the key never changes.

## Before

1. `main` is green (CI) and the version in `app/tauri.conf.json` is the one to release.
2. Go through `docs/RELEASE_CHECKLIST.md` on Windows.

## From GitHub (no Windows machine needed)

Once: add three repository secrets (Settings → Secrets and variables → Actions):
`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (if the key has one) and `RELEASES_TOKEN`
(a fine-grained token on `Freedom-Launcher/releases` with Contents: Read and write).

Then: Actions → **release** → Run workflow → check the version and the notes → Run. It tests, builds, signs and pushes
the build to the `staging` branch of `releases`, whose `publish` workflow creates the Release. Delete the token and the key
secret when you stop using this.

## By hand (what the workflow does)

```
$env:TAURI_SIGNING_PRIVATE_KEY = "path\to\key"
cd app; cargo tauri build --target x86_64-pc-windows-msvc --bundles nsis; cd ..
sh scripts/release-push.sh 0.3.50 "What is new"
```

`release-feed.sh` makes `latest.json` from the finished build; `release-push.sh` calls it and pushes everything to `staging`.
