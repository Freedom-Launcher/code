# Addons

An addon adds features to Freedom Launcher. It is HTML/CSS/JS that runs in an
**isolated, sandboxed iframe**: no Tauri, no network, no access to the launcher
page. Its only way out is `fl.call(action, args)`, and the core checks every call
against the permissions the user approved for this addon.

Themes change how the launcher looks and contain no code — see [THEMES.md](THEMES.md).

## Layout

```
<data>/addons/<id>/
  addon.json      manifest
  index.html      entry (any name, set "entry")
  main.js, *.css, images, fonts
```

Example: [`docs/addons/example`](addons/example). Copy it into the addons folder
(Settings → Дополнения → Открыть), press «Обновить», then «Разрешить и включить».

## addon.json

| Field | Meaning |
|-------|---------|
| `id` | `a-z0-9-`, same as the folder; built-in namespaces (`app`, `accounts`, `game`, …) are reserved |
| `name` | 1–60 characters |
| `version`, `author`, `description` | shown in settings |
| `entry` | HTML file inside the folder, default `index.html` |
| `permissions` | what the addon needs, see below |
| `slots` | where it appears: `rail.end`, `topbar.end`, `instance.end`, `accounts.end`, `discover.end`, `settings.end`, `menu.ctx.instance`, `menu.instance` |
| `hosts` | up to 16 host names (`api.example.org`, no scheme, port or path) the addon may reach with `fl.fetch`; needs `network` |

## Permissions

| Tier | Permissions |
|------|-------------|
| Интерфейс | `instances_read`, `accounts_read` |
| Функция | `instances_manage`, `accounts_manage`, `network`, `game_launch` |
| Расширенный | `accounts_provide`, `process` — «Официальный» and «Проверено» packages only |

Nothing runs until the user approves. A new version that asks for more
permissions stays off until the user approves again. Launcher
self-management (themes, addons, safe settings) is never available to addons.

## Signing and badges

A package earns «Проверено» or «Официальный» only through `signature.json` in
its folder: an Ed25519 signature by a project key over the badge, the addon id
and the SHA-256 of every other file. Without it the package is «Сообщество».
Any changed, added or removed file breaks the signature: the launcher shows
«Подпись нарушена» and treats the package as «Сообщество», so «Расширенный»
permissions stop loading.

```json
{ "v": 1, "key": "fl-2026", "badge": "verified", "sig": "<128 hex>" }
```

Project maintainers sign a reviewed folder with
`cargo run -p fl-core --example sign_addon -- <folder> <seed file> [verified|official]`.
The private key never goes into the repository; public keys live in
`crates/fl-core/src/signing.rs` (`KEYS`). Folders with symlinks, more than 1024
files or more than 64 MB cannot be signed.

## SDK

Load `<script src="fl-sdk.js"></script>` (served by the launcher, not shipped).
Inline scripts are blocked by the CSP: put code in a `.js` file.

- `fl.call(name, args)` → Promise with the action result.
- `fl.toast(text)` — short notification in the launcher.
- `fl.resize()` — fit the frame to the content (also automatic).
- `fl.fetch(url, {method, headers, body})` → `{status, ok, headers, body, json()}`,
  see Network below.
- Theme tokens (`--bg`, `--text`, `--accent`, `--font-ui`, …) are set on the
  addon's `<html>`, and updated when the user switches themes.

## Network

The iframe itself has no network. `fl.fetch` (the `net.fetch` action) asks the
core to make the request, and the core allows it only when:

- the addon has the approved `network` permission;
- the URL is `https://`, on the default port, without a login or password;
- the host is exactly one of the manifest's `hosts` (no wildcards, no IP literals);
- the host resolves only to public addresses — never loopback, LAN, link-local
  or other private ranges.

Redirects are not followed: a `3xx` comes back as is. Methods: `GET`, `HEAD`,
`POST`, `PUT`, `PATCH`, `DELETE`. The launcher sets `User-Agent`, `Host`,
`Cookie`, `Origin`, `Referer` and transport headers; an addon may not. No
cookies are kept and `Set-Cookie` is not returned. Limits: request body 1 MB,
response 4 MB, 30 s. The body is text; send JSON as a string or an object.

```js
const r = await fl.fetch('https://api.modrinth.com/v2/tag/loader');
if (r.ok) console.log(r.json());
```

Safe settings (`--safe`, Shift at start) and «Отключить все дополнения» run the
launcher with no addons at all.
