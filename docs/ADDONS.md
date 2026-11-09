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
plugin folder (for example as a GitHub release asset). Players get it in one of
two ways: add the repository once as a **source** and install from the catalog
(see «Plugin sources»), or pick the file with Settings → Плагины → «Установить
плагин» → «Выбрать файл…» (action `addons.install { path }`). Nothing in the
launcher has to change for a new plugin, and nobody has to approve it first.

- `addon.json` sits at the top of the archive or inside one top folder.
- Only plugin files are accepted: `html js css json png jpg jpeg webp svg woff2
  woff md txt`, names `A-Z a-z 0-9 - _ .`, no symlinks, at most 1024 files and
  64 MB.
- A plugin with the same id is replaced only if the new one loads; otherwise
  the installed copy stays. Approved permissions are kept, new ones are asked
  about again.
- `signature.json` travels inside the archive, so a signed plugin keeps its
  badge.

Ready-made sign-in plugin: [`docs/addons/yggdrasil-provider`](addons/yggdrasil-provider) (Ely.by). To make one
for another Yggdrasil / authlib-injector service, give the generator its name and API root:

```
python3 scripts/new-yggdrasil-plugin.py --name LittleSkin --root https://littleskin.cn/api/yggdrasil
```

It writes a folder with `addon.json`, `config.js` and the page; zip it (with `addon.json` at the top) and
publish it anywhere. `--two-factor` is for services that take `password:code` (Ely.by does). Checked by
`scripts/yggdrasil-plugin-test.js` (the page, against a fake launcher) and the core test
`the_yggdrasil_template_is_a_valid_provider`.

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
| Расширенный | `accounts_provide` (sign-in through another service) |

Any package may ask for any of these, signed or not. The badge is **trust, not
permission**: «Официальный» / «Проверено» mean the launcher team looked at the
code, so the consent dialog stays calm; an unsigned «Сообщество» package gets a
warning banner (and a stronger one when it asks for `accounts_provide`) and the
risky rows stay highlighted. Three rights exist for nobody, whatever the badge:
`internal` (launcher self-management), `process` (raw processes and filesystem:
no action needs it) and the retired `servers`.

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
«Подпись нарушена» and treats the package as «Сообщество», so the consent
dialog warns about it and an account provider stops working.

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
the `accounts_provide` permission (tier «Расширенный»; any package may ask, the
user decides in the consent dialog) and an `account_provider` block:

```json
{ "id": "example-auth", "name": "Example", "permissions": ["accounts_provide", "network"],
  "hosts": ["auth.example.org"], "account_provider": { "label": "Example" } }
```

The addon signs the player in with its own UI and `fl.fetch`, then hands the
result to the core:

| Action | Args | Result |
|--------|------|--------|
| `accounts.provider.save` | `{ name, uuid, token, expires_at, client_token? }` | the provider's own accounts |
| `accounts.provider.list` | — | the provider's own accounts |
| `accounts.provider.remove` | `{ id }` | the provider's own accounts |

Rules the core enforces:

- Microsoft is the built-in default sign-in; a provider is an alternative, not
  an add-on to it. Signing in through a provider needs **no** Microsoft account
  (local profiles still do). `accounts.provider.*` results carry only `accounts`.
- A provider sees and removes only accounts it saved; tokens never come back
  to any addon or to the UI. At most 16 accounts per provider.
- `name` is 3–16 characters `A-Z a-z 0-9 _`, `uuid` is the profile UUID,
  `token` is up to 8192 printable ASCII characters, `expires_at` is a Unix time
  in the future.
- At launch the core checks again: the addon is enabled, the grant is
  approved, the signature (if it has one) still matches the files and the token
  has not expired. A package that was signed and then changed is not a provider:
  providers handle credentials. If any of
  that fails, the account cannot start the game until the addon signs in again.

```js
await fl.call('accounts.provider.save', { name, uuid, token, expires_at, client_token });
```

**Keeping the session alive.** Pass the service's `clientToken` as `client_token` (Yggdrasil services
such as Ely.by give one at sign-in) and the core keeps the token fresh itself: before each launch it asks
`<yggdrasil>/authserver/validate`, swaps a rejected token through `/authserver/refresh`, and stores the new
one. The player is sent back to your sign-in only when the service refuses the refresh. Without `client_token`
the saved `expires_at` decides, as before. Requests go only to the `yggdrasil` API root from your manifest
(its host must be in `hosts`), https only; neither token ever reaches your page. If the service cannot be
reached, the saved token is used while `expires_at` is still in the future.

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

## Developer mode

Settings → Плагины → «Режим разработчика» (off by default) adds a **Разработка** tab to the plugins screen:

- **Подключить папку…** links a plugin folder. The launcher installs a copy and keeps it in step: within a second of a
  file change the copy is refreshed, the plugin list is rebuilt and the plugin's frames are reloaded. No zipping, no
  reinstalling. **Создать плагин…** makes a new folder from the example plugin and links it.
- The copy goes through exactly the rules of a normal install (allowed file types, size limits, valid manifest, reserved
  ids and rights). `.git`, `node_modules`, `target` and dotfiles are not shipped. A problem is shown next to the folder
  and in the log; the last good copy keeps running, and fixing the files is enough.
- A linked plugin is an ordinary «Сообщество» plugin: you still switch it on and approve each permission. A link
  cannot replace a signed plugin or one installed from a catalog source, and is never updated from a catalog.
- The **log** shows what your plugin prints (`console.log/info/warn/error`), uncaught errors and unhandled promise
  rejections (the SDK forwards them, at most 20 lines a second), and every refresh of the copy.

Actions (all `Internal`, refuse while the mode is off): `addons.dev_state` (also picks up changes), `addons.dev_link`,
`addons.dev_unlink`, `addons.dev_new`, `addons.dev_folder`.

## Plugin sources

Anyone can publish plugins without us: put a `freedom-plugins.json` at the root of a repository (or
host the same JSON at any https URL):

```json
{ "name": "My plugins",
  "plugins": [ { "id": "my-plugin", "name": "My plugin", "version": "1.0.0", "author": "me",
    "description": "What it does", "permissions": ["network"],
    "url": "https://github.com/me/repo/releases/download/v1.0.0/my-plugin.zip",
    "sha256": "<sha-256 of the zip>", "size": 12345 } ] }
```

`python3 scripts/plugin-catalog-entry.py my-plugin.zip --url <download URL>` writes or updates the entry
for you (id, version, permissions from `addon.json`; SHA-256 and size from the zip).

**Player.** Плагины → Каталог → «Добавить источник», then type `owner/repository`, a GitHub repository
address (`/tree/<branch>` is honoured) or the https link of the JSON. It is added once and stays: its
plugins appear in the same catalog, labelled «Сообщество» with the source's name, and install in one click
(`addons.source_add`, `addons.source_remove`, `addons.sources`, `addons.install_from { id, source }`). Up to
16 sources. The official catalog is always there, listed first, and cannot be removed. The chips above the list filter by
source (a click shows only that source, a second click shows everything).

**What the launcher checks for a plugin from a source** (nobody reviews it by hand):

- The list and the zip are fetched over https only, from public addresses only (also after a redirect,
  at most three, which GitHub downloads need); a list is at most 1 MB, a zip 64 MB, a source lists at most 200 plugins.
- The zip's SHA-256 equals the one in the list. The archive holds the plugin the entry names (same `id`).
- **The archive asks for no permission the entry did not list**: what the player read in the catalog is a
  ceiling. An archive that wants more is refused before anything is installed.
- The usual archive rules apply (allowed file types, safe names, size limits, valid manifest, reserved
  ids and rights stay reserved).
- A feed cannot call itself official or invent downloads and likes: those are overwritten.
- A plugin installed from one source is not replaced by another source, and a signed plugin
  («Проверено» / «Официальный») is not replaced by any foreign source. Remove it first.
- Nothing is switched on by the install: the player still sees the permission dialog (with the stronger
  warning for sign-in providers), and the plugin stays «Сообщество» unless it carries a valid project signature.

**Updates.** The launcher checks quietly (cached for ten minutes) whether a plugin has a newer version in the
source **it came from** (`addons.updates`): a banner «Доступны обновления» with «Обновить всё» appears on the installed
tab, each plugin gets «Обновить до vX». A higher version of the same id in another source is never offered, a
hand-installed plugin and a developer link are never touched, and an update goes through the same checks as an install.
Versions compare numerically (`1.10.0` > `1.9.0`), a pre-release is older than its release (`1.0.0-beta` < `1.0.0`), and
an unreadable version never triggers an update. If the new version asks for a **new permission** (the entry must list it),
what the player approved stays approved and only the new one waits for a decision; the plugin is not switched on or
given more by the update.

What it does **not** do: read the plugin's code. «Проверено» still means a project signature, nothing else.
A source is as trustworthy as its author, so the catalog says «Плагины из чужих источников мы не проверяем».
