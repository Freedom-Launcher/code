# Freedom Launcher

A fast, minimal, deeply customizable Minecraft: Java Edition launcher.
Rust core, Tauri 2 shell, plain HTML/CSS/JS interface. Windows, macOS and Linux.

> Status: **0.3 — playable.** Microsoft sign-in, local profiles, versions and instances, game launch (Fabric, Quilt, Forge, NeoForge), mods and modpacks from Modrinth and CurseForge, skins, import from other launchers, themes and sandboxed addons. See `docs/ROADMAP.md` for what is done and what is next.

## Principles

- **The clean launcher is small and fast.** Target: window painted in under 1 second, no visible jank.
- **Everything is an action.** Each capability is a named action (`accounts.list`, `instances.create`, ...).
  The built-in screens, themes and addons all call the same actions through the same permission checks.
- **Themes change looks, addons change behaviour.** Themes carry no code. Addons get only the permissions the user granted.
- **There is always a way back.** Safe settings start without any theme or addon and can undo what broke the UI.

## Accounts

- **Microsoft account** — the primary way to sign in.
- **Local profiles** — become available after signing in with a Microsoft account that owns Minecraft: Java Edition.
- **Third-party services** — added through account-provider addons from the addon catalog.

## Layout

```
crates/fl-core   core library: action bus, permissions, accounts, versions, instances
app/             Tauri 2 desktop shell (one IPC command: `action`)
ui/              built-in interface (no framework, no build step)
docs/            architecture, roadmap
```

## Build

Prerequisites: Rust (stable) and the [Tauri 2 system dependencies](https://tauri.app/start/prerequisites/).

```sh
cargo test -p fl-core                     # core tests
cargo run -p freedom-launcher             # debug run
cargo run -p freedom-launcher -- --safe   # safe settings
cargo build -p freedom-launcher --release
```

Microsoft sign-in needs the Azure application (client) ID in `FREEDOM_MSA_CLIENT_ID`
(at build time or at runtime). Set `FREEDOM_LOG_STARTUP=1` to print startup time.
## CI

`.github/workflows/ci.yml` runs on every push to `main` and every pull request: `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test` for `fl-core`, the UI mount test (`scripts/ui-mount-test.js`, jsdom) and a release build of the app on Linux, Windows and macOS.
Locally: `cargo fmt --all --check && cargo clippy -p fl-core -- -D warnings && cargo test -p fl-core`.
