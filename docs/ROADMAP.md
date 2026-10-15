# Roadmap

- **M0 — skeleton (done):** workspace, action bus with permission checks, Microsoft sign-in, local profiles, version list with cache, instances, safe settings, CI.
- **M1 — play:** download client/libraries/assets with hash checks and parallelism, Java runtime management, game launch and logs, design D. Mod loaders: Fabric (done), Forge, NeoForge. Entitlements check, token refresh and OS keychain storage, single instance + tray, Shift/tray safe settings. Instance basics: RAM, Java picker, rename/duplicate/delete/open folder, resolution, playtime, per-field overrides.
- **M2 — themes:** theme loader, tokens, screen templates, theme picker. _Done: see docs/THEMES.md._
- **M3 — addons:** isolated addon host, manifest and permission prompt, signing and badges, UI slots, account-provider contract, addon catalog.
  - Done: isolated addon host, manifest checks, permission approval, UI slots, SDK, `docs/ADDONS.md`. Next: network action, signing (Verified badge), account-provider contract, catalog.
- **M4 — content:** mod browsing via Modrinth — search, install with required dependencies (SHA1-checked), enable/disable, remove, real icons (done); Quilt; modpack import (.mrpack).
- **M5 — release:** Tauri NSIS installer (per-user, no admin, WebView2 bootstrapper, uninstaller), `tauri-plugin-updater` on GitHub Releases with signed manifests, code signing, first-run guide inside the launcher, startup budget enforced in CI.