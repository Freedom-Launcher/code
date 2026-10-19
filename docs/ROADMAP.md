# Roadmap

- **M0 — skeleton (done):** workspace, action bus with permission checks, Microsoft sign-in, local profiles, version list with cache, instances, safe settings, CI.
- **M1 — play:** download client/libraries/assets with hash checks and parallelism, Java runtime management, game launch and logs, design D. Mod loaders: Fabric (done), Forge, NeoForge. Entitlements check, token refresh and OS keychain storage, single instance + tray, Shift/tray safe settings. Instance basics: RAM, Java picker, rename/duplicate/delete/open folder, resolution, playtime, per-field overrides.
- **M2 — themes:** theme loader, tokens, screen templates, theme picker. _Done: see docs/THEMES.md._
- **M3 — addons:** isolated addon host, manifest and permission prompt, signing and badges, UI slots, account-provider contract, addon catalog.
  - Done: isolated addon host, manifest checks, permission approval, UI slots, SDK, `docs/ADDONS.md`; network action `net.fetch` / `fl.fetch` (https only, manifest hosts, public addresses only, no redirects, size limits); Ed25519 package signatures (`signature.json`, «Проверено» / «Официальный», «Подпись нарушена» on any changed file); account-provider contract (`accounts.provider.*`, signed `accounts_provide` addons, only after a Microsoft account with the game, re-checked at launch). Next: catalog.
- **M4 — content:** mod browsing via Modrinth — search, install with required dependencies (SHA1-checked), enable/disable, remove, real icons (done); mod updates — per mod and «update all», disabled state kept (done); modpack import (.mrpack) — path/host/hash checks, overrides, progress (done); Quilt — loader via meta.quiltmc.org, Fabric mods accepted, Quilt .mrpack import (done).
- **Interface editor (done):** `Ctrl Shift E` / Ctrl K «Настроить интерфейс» / Settings → Лаунчер → «Интерфейс». General tab: radii, sizes, layout sides. Element tab: per-`data-id` radius, size, hide, order — and drag blocks to reorder (animated, undo). Edits live in `settings.ui` (numbers and known words only, no CSS). «Сохранить как тему» (`themes.save {name}`) turns the edits into a user theme: themes carry `ui`, layered under the user's own edits.
- **Modrinth App parity:** Э1 content types, Э2 project page, Э3 modpack update + `.mrpack` export (done). Э4 instance page — Worlds tab (done: icon, last played, size, open folder, delete via `worlds.folder` / `worlds.delete`); Screenshots tab (done: day groups, viewer, delete, `instances.screenshots`, `shot:` protocol) and Logs tab (done: live follow, levels, search, copy via `instances.log`); next: files, servers. Shared motion layer `ui/motion.js` / `ui/motion.css`. Gap list vs other launchers: artifact `launcher-gap-audit`. Э5 Java and instance settings (done: sectioned dialog Общее / Java и память / Окно / Запуск; Java detection `java.list`, `java.test`, managed download `java.install`; memory slider from `system.memory`; fullscreen, env vars, hooks pre-launch / wrapper / post-exit; launcher-wide defaults in `settings.game`, per-instance overrides block by block; program-running fields only via Internal `instances.configure`). Э6 library: search, sort, group-by (group / loader / version) and «Группа…» (done, `Instance.group`); import from other launchers (done in 0.3.3); next: downloads panel.
- **M5 — release:** Tauri NSIS installer (per-user, no admin, WebView2 bootstrapper, uninstaller), `tauri-plugin-updater` on GitHub Releases with signed manifests, code signing, first-run guide inside the launcher, startup budget enforced in CI.
  - Done: NSIS per-user installer config (ru/en, WebView2 bootstrapper); first-run guide (sign in → look → first instance, `settings.onboarded`). Look settings moved into the core (`settings.set`). Waiting on a public repo: code signing (SignPath) and the updater endpoint on GitHub Releases.

## Grouped settings, statistics, drag and drop (0.3.5)

- Settings rebuilt after Modrinth App's settings modal (`ui/settings.js`, `ui/settings.css`): a side nav grouped into Оформление (Внешний вид, Обои и стекло, Интерфейс), Игра (Параметры сборок, Статистика) and Система (Плагины, Клавиши, О лаунчере), with a sliding marker; each topic is a card with a label. Cards whose rows are all hidden disappear.
- Settings search: `/` focuses it, results filter rows across all sections, Enter jumps to the first match. Every single setting is also in Ctrl K (only when typing); picking one opens its section, scrolls to the row and highlights it.
- «Статистика»: total time in game, instance count, launched this week, most played instances with bars (from `playtime_secs` / `last_played`).
- «Клавиши»: every shortcut in one list. «О лаунчере»: version, link to all releases, updates.
- Drag and drop from the file manager (Tauri `onDragDropEvent`, `ui/drop.js`): a `.mrpack` anywhere imports a modpack; other files on an instance page go into its open content section (`drop_content`), with an overlay that says what will happen.
- Old pane names keep working: `launcher` opens «Внешний вид», `addons` stays «Плагины».

## Content tab like Modrinth App (0.3.4)

- Instance «Контент» tab rebuilt after Modrinth App's content page (`ui/content.js`, `ui/content-ops.js`, `ui/content.css`): search over title, file, author and version; sort A–Я / Я–А / newest / oldest / author / size and by the version column, remembered per instance; quick pills Все / Обновления / Включённые / Выключенные.
- «Фильтр»: author (with avatars, searchable), environment (client only / server only / both), open source (OSI licenses), update available, not from Modrinth, frozen. Options that would match everything are hidden, as in Modrinth.
- Rows: icon and title open the project page; author avatar and name (or «Загружен вручную»); version and file name with middle truncation; one icon slot — green update, «Заменить версию» or lock; toggle; delete with confirmation (Shift deletes at once); ⋯ menu — open page, switch version, show file, copy link, freeze / unfreeze.
- Multi-select (Shift for ranges) with a floating bar: update, enable, disable, copy names / file names / links / Markdown, delete. Deleting or disabling something other content needs warns with the list.
- «Заменить версию» dialog: searchable versions with release / beta / alpha, «Текущая», incompatible versions on demand, changelog beside the list; update, roll back or install anyway.
- Core: list entries carry author, license, client/server side, dependencies, size, date added (`mods.meta` fills them and identifies files by hash); `mods.versions`, `mods.switch`, `mods.lock` (frozen files skip updates), `mods.add_files`; app commands `reveal_content`, `pick_content`. Upload files and refresh in the toolbar.

## Import from other launchers, layout, scoped catalog (0.3.3)

- Import from other launchers: Prism Launcher / MultiMC, CurseForge, ATLauncher, GDLauncher, Modrinth App, the official launcher and any game folder (`foreign.rs`, Internal `foreign.scan` / `foreign.import`). Name, version and loader come from each launcher's metadata, otherwise from `logs/latest.log` and the mods. Only game content is copied (mods, configs, packs, worlds, options, servers). Dialog grouped by launcher, «Выбрать папку…», progress; also in Ctrl K.
- Catalog scoped to an instance: «Установка в …» header with back, loader and version locked, only the kinds the instance supports, path and rail follow the instance.
- «Панели: Авто / Отдельно / Вплотную» (`look.panels`), independent of the wallpaper; glass in the light theme with legible text.
- Instance page: cover view switch (large / slim / none) next to «Сменить обложку». New instance: own icon and cover picked up front, compact cover row.
- Project pages: images from any host render as images (core-fetched `rimg:` protocol — https, public hosts, type-sniffed, cached).
- Interface editor: hidden elements listed first in «Общее» and shown ghosted while editing; one shared card size; no reordering of sorted lists; borders drawn without shifting the layout; the panel no longer jumps sides; a user can show what a theme hides (`ui.els.hide = false`).
- Fixes: `.mrpack` import read error (native dialog, the core reads the file); home cards collapsing under «По группам» without groups.

## Own pictures, element styles, colours in themes (0.3.2)

- Own instance icon and cover from the user's photos: crop dialog (drag the frame, wheel to zoom around the cursor, corner handles, arrows / ± keys, rule-of-thirds grid while moving, live previews, drop another file). Cropped in the UI (icon 256 PNG, cover ≤1600×900 WebP), stored by the core in `art/<instance>.<icon|cover>.<ext>` (type from bytes, 8 MB max), served via the `art:` protocol; kept on duplicate, removed on delete. Icon is used in the rail, home, breadcrumbs and Ctrl K; «Как обложка» falls back to the cover.
- Interface editor: a reset button on every slider; per-element background colour (shared colour picker), opacity, backdrop blur, brightness, contrast, saturation, border width and colour, shadow depth. All values are clamped numbers or `#hex` in `settings.ui.els` — still no raw CSS.
- «Сохранить как тему» can bake the user's base colours (with derived shades) into the theme's tokens (`themes.save {name, tokens}`), so a palette can be shared as a theme file; the overrides are then cleared.

## Base colors (0.3.1)

- Settings → Лаунчер → «Цвета»: accent, window background, panels, cards, fields, glass tint, three text levels, lines, error and warning colors — separately for the dark and light base, laid over the active theme.
- Own color picker (saturation/brightness, hue, opacity for lines, HEX, eyedropper, presets, was/now), ready-made sets, per-color and full reset, contrast warning.
- Hover/press/soft shades, text on accent, focus ring and glow are derived automatically. Stored in `settings.colors` (hex only, validated by the core); safe mode ignores them and «Вернуть стандартную тему» clears them.

## Navigation, catalog pages, instance settings (0.3.0)

- Title bar: back / forward history and breadcrumbs (`ui/nav.js`), Alt+←/→, mouse buttons 4/5.
- Catalog: numbered pages, 10 / 20 / 50 per page; `look.browse` = `pages` | `scroll`, `look.per_page`.
- Settings → Игра is inline (`ui/game-settings.js`): Java per major (`settings.javas`), memory, JVM args, env, window, hooks, instances folder.
- Instance settings: Общее (name, group, cover) / Установка (`loaders.versions`, `instances.set_game`, repair, `instances.unlink`) / Java и память / Окно / Хуки запуска (`ui/inst-install.js`).
- Remaining Modrinth gaps: `/workspace/research/matrix.md` (artifact `modrinth-port-plan`).

## Instances folder and in-app updates (0.2.0)

- Custom instances root: `settings.instances_dir`, `instances.root`, Internal `instances.move_root` (moves folders with progress and rollback, refuses while a game runs). UI: Settings → Игра → «Папка сборок».
- In-app updater: `tauri-plugin-updater`, minisign key kept outside the repo, feed `latest.json` from the public `Freedom-Launcher/releases` repo (pending creation), `scripts/release-feed.sh`. Build with `TAURI_SIGNING_PRIVATE_KEY`. UI: Settings → Лаунчер → «Обновления».
