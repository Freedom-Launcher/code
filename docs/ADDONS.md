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
| `hosts` | hosts the addon will reach through the core's network action (next M3 step); needs `network` |

## Permissions

| Tier | Permissions |
|------|-------------|
| Интерфейс | `instances_read`, `accounts_read` |
| Функция | `instances_manage`, `accounts_manage`, `network`, `game_launch` |
| Расширенный | `accounts_provide`, `process` — «Официальный» and «Проверено» packages only |

Nothing runs until the user approves. A new version that asks for more
permissions stays off until the user approves again. Launcher
self-management (themes, addons, safe settings) is never available to addons.

## SDK

Load `<script src="fl-sdk.js"></script>` (served by the launcher, not shipped).
Inline scripts are blocked by the CSP: put code in a `.js` file.

- `fl.call(name, args)` → Promise with the action result.
- `fl.toast(text)` — short notification in the launcher.
- `fl.resize()` — fit the frame to the content (also automatic).
- Theme tokens (`--bg`, `--text`, `--accent`, `--font-ui`, …) are set on the
  addon's `<html>`, and updated when the user switches themes.

Safe settings (`--safe`, Shift at start) and «Отключить все дополнения» run the
launcher with no addons at all.
