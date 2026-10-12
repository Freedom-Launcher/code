# Architecture

## Processes and layers

```
┌──────────── WebView (ui/ or a theme) ────────────┐
│  markup with data-action / data-state            │
│  built-in JS (app.js)  ·  addon UI slots (later) │
└───────────────┬──────────────────────────────────┘
                │ one IPC command: action(name, args)
┌───────────────▼──────────────────────────────────┐
│ app/ (Tauri shell): window, tray, single instance│
├──────────────────────────────────────────────────┤
│ fl-core                                          │
│  ActionRegistry → Caller::check → handler        │
│  accounts · msauth · versions · instances ·      │
│  settings · store (atomic JSON) · (later) launch,│
│  downloads, addon host, theme engine, updater    │
└──────────────────────────────────────────────────┘
```

## Action bus

- An action has a name (`namespace.verb`), a required `Permission`, and a `bindable` flag.
- `Core::dispatch(caller, name, args)` is the only entry point. Unknown names and missing permissions are errors.
- Callers:
  - `BuiltinUi` — the built-in screens, full access (they are trusted code shipped with the launcher).
  - `Theme` — only `bindable` actions, only from a user click on a `data-action` element.
  - `Addon` — only actions whose permission the user granted at install time.
- Addons register actions only in their own namespace `<addon id>.*`.
- The UI marks the element running an action with `data-state` = `loading` / `success` / `error`, so themes style states in CSS alone.

## Permissions and trust

| Tier | Example permissions | Who may request |
|------|---------------------|-----------------|
| Тема (Theme) | none, visuals | everyone |
| Интерфейс (Interface) | `accounts_read`, `instances_read` | everyone |
| Функция (Function) | `accounts_manage`, `instances_manage`, `network`, `game_launch` | everyone, with user consent |
| Расширенный (Extended) | `accounts_provide`, `process` | Official and Verified packages only |

Badges: **Официальный** (built by the project), **Проверено** (reviewed and signed with the project key), **Сообщество** (anyone).

## Addon runtime (milestone M3)

- Each addon runs in its **own isolated WebView/iframe** with a strict CSP and no Tauri globals; it talks to the core only via a `postMessage` bridge mapped to `dispatch(Caller::Addon{...})`.
- Package = signed archive: `manifest.json` (id, version, permissions, UI slots), files, Ed25519 signature. Verified badge = project-key signature over the archive hash.
- Network access goes through a core-side `network` action with a per-addon host allowlist from the manifest.
- Account providers implement a fixed contract (`login`, `refresh`, `profile`) and never see other providers' secrets.
- Crash protection: two main-UI crashes in a row → next start offers safe settings.

## Themes (milestone M2)

- Theme = folder/archive with `theme.json` (tokens), CSS, optional HTML templates per screen. No JS (enforced by CSP and by the loader rejecting script tags/handlers).
- Templates call actions with `data-action`; state shown via `data-state`.

## Accounts

- Microsoft: OAuth device code → Xbox Live → XSTS → Minecraft services. Tokens stay in the core; the UI sees only public fields.
- Local profiles are created only when a Microsoft account with Minecraft: Java Edition is present.
- Third-party services: account-provider addons (Extended tier).
- Secrets currently in `accounts.json` with 0600; moving to the OS keychain is planned (M1).

## Startup performance

- One small release binary (LTO, `panic=abort`, stripped), no frontend framework, no bundler.
- HTTP client and TLS created lazily, after first paint.
- First paint uses only local files; the version list loads afterwards with a 1-hour cache and offline fallback.
- `app.ready` records process-start → first-paint time; CI will fail if it regresses past the budget.

## Safe settings

`--safe` (later also Shift at launch, tray item, automatic offer after crashes) starts with only the
`app.*`, `settings.get` and `safe.*` actions registered: restore default theme, disable addons, reset settings.