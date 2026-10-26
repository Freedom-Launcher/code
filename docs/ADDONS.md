# Addons («Плагины»)

In the UI, addons are called **«Плагины»**: they extend the launcher itself.
Game content (mods, modpacks, resource packs, shaders, data packs) is separate and lives under «Моды».
The code name stays `addon` (`addon.json`, `addons.*` actions, `addon:` protocol).

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
(Settings → Плагины → Открыть), press «Обновить», then «Разрешить и включить».

## Installing and publishing

A plugin can live in any repository. Its author publishes a `.zip` of the
plugin folder (for example as a GitHub release asset); players install it with
Settings → Плагины → «Установить плагин» → «Выбрать файл…» (action
`addons.install { path }`). Nothing in the launcher has to change for a new
plugin.

- `addon.json` sits at the top of the archive or inside one top folder.
- Only plugin files are accepted: `html js css json png jpg jpeg webp svg woff2
  woff md txt`, names `A-Z a-z 0-9 - _ .`, no symlinks, at most 1024 files and
  64 MB.
- A plugin with the same id is replaced only if the new one loads; otherwise
  the installed copy stays. Approved permissions are kept, new ones are asked
  about again.
- `signature.json` travels inside the archive, so a signed plugin keeps its
  badge.

Reference plugin in its own repository: `Freedom-Launcher/plugin-elyby`.

## Built-in plugins

Plugins in the repository folder `addons/` ship inside the launcher
(`crates/fl-core/src/bundled.rs`, list `BUNDLED`). On start they are copied into
`<data>/addons/` when missing or older. They are signed «Официальный», but are
never switched on by themselves: the user approves them like any other plugin.
A built-in plugin the user deleted comes back only with a newer version, and a
foreign folder with the same id is left alone. After changing any of its files,
bump `version` and sign the folder again (see «Signing and badges») before
building: the files are embedded at compile time.

A built-in plugin that stops shipping moves to `RETIRED` together with the
`sig` of every `signature.json` it shipped with. On start its folder, its
storage and its approval are removed, but only while the folder is still the
exact signed copy the launcher installed; a copy the user changed is kept.

No plugins are built in. Retired: `play-stats` (shipped in 0.3.14 only; the same
statistics already live in Settings → «Статистика») and `server-manager`
(«Управление серверами», 0.3.15–0.3.17, removed together with the `servers.*`
actions; the user's `servers.json`, server folders and backups are left on disk).

## addon.json

| Field | Meaning |
|-------|---------|
| `id` | `a-z0-9-`, same as the folder; built-in namespaces (`app`, `accounts`, `game`, …) are reserved |
| `name` | 1–60 characters |
| `version`, `author`, `description` | shown in settings |
| `entry` | HTML file inside the folder, default `index.html` |
| `permissions` | what the addon needs, see below |
| `slots` | where it appears: `rail.end`, `topbar.end`, `instance.end`, `accounts.end`, `discover.end`, `settings.end`, `menu.ctx.instance`, `menu.instance` |
| `account_provider` | `{ "label": "…" }` — the service name on the accounts screen; needs `accounts_provide`, see «Account providers» |
| `hosts` | up to 16 host names (`api.example.org`, no scheme, port or path) the addon may reach with `fl.fetch`; needs `network` |
| `pages` | up to 4 own pages `[{ "id": "grid", "title": "Сетка", "entry": "grid.html" }]`, each a tab in the rail; `entry` defaults to `<id>.html`; needs `ui_pages` |
| `launch_overlay` | HTML file shown full-window while the game starts; needs `ui_overlay` |

## Permissions

| Tier | Permissions |
|------|-------------|
| Интерфейс | `instances_read`, `accounts_read`, `ui_pages`, `ui_overlay`, `storage` |
| Функция | `instances_manage`, `instance_files`, `accounts_manage`, `network`, `game_launch` |
| Расширенный | `accounts_provide`, `process` — «Официальный» and «Проверено» packages only |

Nothing runs until the user approves. Approval is per permission: the
consent dialog lists every requested permission in plain words, highlights
the risky ones (anything that changes data, the network, the game) and warns
when the package is unsigned. The user ticks what to allow; unticked
permissions are remembered as declined and not asked about again, and the
addon simply gets `permission denied` for those calls. Settings → Плагины →
«Права…» changes the choice later. A new version that asks for more
permissions stays off until the user decides on the new ones.

Whatever is granted, an addon never gets the computer's files, processes or
other programs: it is a sandboxed page that reaches the launcher only through
the actions below. Launcher
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
- `fl.action(id, cb)`, `fl.lang` — see «Actions and panels».
- `fl.on`, `fl.context`, `fl.overlay.done()` — see «Pages, launch overlay and events».
- `fl.storage.get/set/delete/keys` — see «Storage».
- `fl.files.list/read/write/delete` — see «Instance files».
- `fl.resize()` — fit the frame to the content (also automatic).
- `fl.fetch(url, {method, headers, body})` → `{status, ok, headers, body, json()}`,
  see Network below.
- Theme tokens (`--bg`, `--text`, `--accent`, `--font-ui`, …) are set on the
  addon's `<html>`, and updated when the user switches themes.

## Actions and panels

Plugins build into the launcher's own screens instead of opening tabs of their
own. Both need `ui_pages` («Свой интерфейс»).

- **Actions** (`actions`, up to 12): `{ "id", "title", "icon", "places" }`.
  The launcher draws each action in its own style — an icon button, a menu
  item, a button on a card or a command — in every listed place:
  `instance.toolbar` (instance page header), `logs.toolbar` (the instance's
  logs), `crash` (the crash card), `menu.instance` («⋯» on the instance page),
  `menu.ctx.instance` (right click on an instance), `palette` (command bar, for
  the open instance). `icon` is a launcher icon name: `share`, `note`,
  `folder`, `terminal` or `cube` (the default).
  The code lives in the plugin's `entry` page, which the launcher loads hidden
  in the background: `fl.action(id, async ({ instance, place }) => result)`.
  While it runs the button shows progress. The result may be
  `{ toast?, copy?, open? }`: `copy` goes to the clipboard, `open` (https, a
  host from `hosts`) becomes an «Открыть» button on the toast. A thrown error is
  shown as an error toast; after 90 s the launcher stops waiting.
- **Panels** (`panels`, up to 4): `{ "id", "title"?, "entry"?, "place" }` — a
  small HTML block rendered inline in a launcher screen and sized to its
  content (`entry` defaults to `<id>.html`). `instance.overview` sits on the
  instance page above its tabs; `fl.context.instance` is the open instance and
  `fl.on('context', cb)` fires when the user opens another one.
- `fl.lang` is the launcher language (`ru`, `en`, …) for the plugin's own
  strings.

`pages` are only for whole new screens; small features belong in actions and
panels. The official plugins use no pages: «Поделиться логом» is an action in
the logs, on the crash card, in both instance menus and in the command bar;
«Заметки к сборкам» is a panel on the instance page.

## Pages, launch overlay and events

- **Pages** (`ui_pages`): each entry of `pages` becomes a tab in the rail
  with the first letter of its title. The page fills the main area and keeps
  its state while the launcher runs. Good for whole screens: a hero grid,
  statistics, a mod browser.
- **Launch overlay** (`ui_overlay`): when the user presses «Играть», the
  `launch_overlay` page covers the whole window on top of everything. It is
  click-through (the launcher stays usable), Esc removes it, and it is removed
  6 s after the game started, at once if the launch failed, and after 20 s at
  most. Call `fl.overlay.done()` when the animation is over.
- **Events** (`instances_read` or `ui_overlay`): `fl.on(name, cb)` with
  `game.launching`, `game.launched`, `game.failed`, `game.exited` (data
  `{ instance }`), or `*` for all; returns an unsubscribe function.
- `fl.context` tells the frame where it is:
  `{ kind: 'slot' | 'page' | 'panel' | 'background' | 'overlay', slot?, page?, panel?, instance?, lang }`.

Page and overlay frames may use any CSS animations, canvas and SVG: they are
ordinary pages, only without network and without the launcher's own DOM.

## Storage

`storage` gives the addon its own key/value store (any JSON values, keys up
to 128 characters, 1 MB in total), kept in
`<data>/addon-data/<id>/storage.json`. An addon sees only its own keys.

```js
await fl.storage.set('layout', { columns: 5 });
const layout = await fl.storage.get('layout');
```

## Instance files

`instance_files` («Изменять файлы сборок», tier «Функция») lets the addon
read and change text files inside instance game folders: configs,
`options.txt`, world data. The core enforces:

- relative paths only, inside the instance's own folder: no `..`, no absolute
  paths, no links (a symlink anywhere on the path is refused);
- no writing or deleting code: `.jar`, `.class`, `.zip`, `.exe`, `.dll`,
  `.so`, `.bat`, `.cmd`, `.ps1`, `.sh`, `.js`, `.py` and similar, and nothing
  in `mods/`, `coremods/`, `kubejs/`, `natives/`. Mods are installed only
  through the launcher's own mod actions (`instances_manage`);
- no launcher bookkeeping files (`freedom-*`);
- UTF-8 text, at most 2 MB per file, at most 1000 entries per listing.

| Action | Args | Result |
|--------|------|--------|
| `files.list` | `{ instance, path? }` | `[{ name, dir, size }]` |
| `files.read` | `{ instance, path }` | `{ text }` |
| `files.write` | `{ instance, path, text }` | `{ ok }` |
| `files.delete` | `{ instance, path }` | `{ ok }` |

```js
const opts = await fl.files.read(instanceId, 'options.txt');
await fl.files.write(instanceId, 'config/myaddon.json', JSON.stringify(cfg));
```

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

## Account providers

An account provider adds sign-in through another service. It is an addon with
the `accounts_provide` permission (tier «Расширенный», so only signed
«Проверено» / «Официальный» packages) and an `account_provider` block:

```json
{ "id": "example-auth", "name": "Example", "permissions": ["accounts_provide", "network"],
  "hosts": ["auth.example.org"], "account_provider": { "label": "Example" } }
```

The addon signs the player in with its own UI and `fl.fetch`, then hands the
result to the core:

| Action | Args | Result |
|--------|------|--------|
| `accounts.provider.save` | `{ name, uuid, token, expires_at }` | the provider's own accounts |
| `accounts.provider.list` | — | the provider's own accounts |
| `accounts.provider.remove` | `{ id }` | the provider's own accounts |

Rules the core enforces:

- Service accounts are available only after a Microsoft account with
  Minecraft: Java Edition is signed in, the same rule as local profiles.
- A provider sees and removes only accounts it saved; tokens never come back
  to any addon or to the UI. At most 16 accounts per provider.
- `name` is 3–16 characters `A-Z a-z 0-9 _`, `uuid` is the profile UUID,
  `token` is up to 8192 printable ASCII characters, `expires_at` is a Unix time
  in the future.
- At launch the core checks again: the addon is enabled, the grant is
  approved, the signature is intact and the token has not expired. If any of
  that fails, the account cannot start the game until the addon signs in again.

```js
await fl.call('accounts.provider.save', { name, uuid, token, expires_at });
```

### In-game sign-in, skins and capes (`yggdrasil`)

A provider whose service speaks the Yggdrasil protocol sets
`account_provider.yggdrasil` to its authlib-injector API root. The URL must be
https and its host must be listed in `hosts`:

```json
"account_provider": { "label": "Ely.by", "yggdrasil": "https://authserver.ely.by/api/authlib-injector" }
```

When the game starts with such an account, the core adds
[authlib-injector](https://github.com/yushijinhun/authlib-injector) with that
root (`-javaagent:<jar>=<root>`), so joining servers, skins and capes go
through the service. The agent is not shipped with the launcher (AGPL-3.0): the
official release jar is downloaded once into `<data>/agents/` and must match
the SHA-256 pinned in `crates/fl-core/src/agent.rs`. Microsoft accounts and
local profiles never load it.

Safe settings (`--safe`, Shift at start) and «Отключить все плагины» run the
launcher with no addons at all.

## Official catalog

The «Плагины» page opens on the **Каталог** tab. It reads `plugins/catalog.json` from the
`main` branch of `Freedom-Launcher/releases` (core action `addons.catalog`, cached for 10 minutes,
last copy used offline). Each entry has `id, name, author, description, version, hue, permissions,
url, sha256, size, asset, issue`. `addons.install_from {id}` downloads the listed `.zip` (only from
`raw.githubusercontent.com/Freedom-Launcher/` or `github.com/Freedom-Launcher/`, ≤ 64 MB), checks
its SHA-256 and installs it like a `.zip` picked by hand; the plugin still asks for its permissions.

- **Likes**: reactions 👍 / ❤ on the plugin's issue (label `plugin`) in `Freedom-Launcher/releases`;
  the heart on a tile opens that issue.
- **Downloads**: `download_count` of the asset with the entry's `asset` name in the release tagged
  `plugins` of the same repository, when it exists (the zip URL then switches to that asset).

Sources of official plugins live in `plugins/<id>/` of this repository.
`python3 scripts/plugins-pack.py <seed file> <out dir>` signs them as «Официальный», zips them and
writes `catalog.json`; push the output to `plugins/` of `Freedom-Launcher/releases`.
