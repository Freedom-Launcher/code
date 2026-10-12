# Freedom Launcher

A fast, minimal, deeply customizable Minecraft: Java Edition launcher.
Rust core, Tauri 2 shell, plain HTML/CSS/JS interface. Windows, macOS and Linux.

> Status: **0.1 — minimal working skeleton.** Microsoft sign-in, local profiles, version list, instances, safe settings.
> Game launch, themes and addons are the next milestones (see `docs/ROADMAP.md`).

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

The GitHub Actions workflow lives in `docs/ci.workflow.yml`. To enable it, copy it to `.github/workflows/ci.yml`
(GitHub only accepts workflow files from accounts with the `workflows` permission).
