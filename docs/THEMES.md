# Themes

A theme changes how Freedom Launcher looks, up to the structure of screens. It contains **no code**.

Install: Settings → Launcher → «Папка тем» → «Открыть», put the theme folder there, press «Обновить».
A broken theme never breaks the launcher: it is shown as unavailable, or the default theme is used
with a message. Safe settings (`--safe`, Shift at start, tray) never load themes.

## Layout

```
themes/<id>/
  theme.json      tokens, CSS list, templates
  *.css           optional
  *.html          optional screen templates
  img/, fonts/    optional assets (png jpg webp gif svg woff2 woff ttf)
```

`theme.json`:

| field | meaning |
|---|---|
| `id` | folder name; `a-z 0-9 -`, up to 40 chars; `default` and `light` are reserved |
| `name` | shown in the picker, 1–60 chars |
| `version`, `author` | optional |
| `base` | `dark` or `light`: the built-in palette the theme starts from |
| `tokens` | CSS variables from `ui/tokens.css`, e.g. `"--accent": "#ffb86b"` |
| `css` | up to 8 CSS files, applied after the built-in CSS |
| `templates` | screen → HTML file; screens: `view.instance`, `view.empty`, `view.create`, `view.accounts`, `view.discover` |

Settings are not themeable, so you can always switch back.

## Templates

- Built-in pieces keep their behaviour: `<div data-use="instance.play"></div>` puts the built-in element
  with that `data-id` here. Run the launcher with `?dev` to list every `data-id`.
- `data-view="settings"` navigates; `data-action="<action>"` (+ `data-args='{"…"}'`) calls a
  **bindable** action with theme rights (see «What a theme may call» below). While it runs the element gets `data-state` =
  `loading` / `success` / `error`, so states are styled in CSS only.
- `href` / `src` may point only to theme files or `#…`.

## What a theme may call

A theme is markup and CSS from anyone, and its CSS can make any element cover the window, so a
click on a theme element is not proof that the user meant it. A theme can therefore bind only
harmless actions: lists and info (`app.info`, `versions.list`, `accounts.list`, `instances.list`,
`accounts.providers.list`), opening folders (`instances.folder`, `logs.folder`, `screenshots.folder`,
`worlds.folder`, `worlds.backups_folder`), `accounts.select`, `accounts.microsoft.begin`, `instances.create`,
`game.stop`, `game.launch`, `accounts.microsoft.authorize` and the `safe.*` recovery actions.

- `game.launch` and `accounts.microsoft.authorize` ask the user in a **native dialog every time**
  («Тема … просит действие»). The dialog names the theme by its id and the instance by its name; a theme
  cannot restyle or cover it.
- Everything that changes or deletes data or installs content (`accounts.remove`, `instances.update`,
  `instances.duplicate`, `game.install`, all `mods.*`, `doctor.fix`, …) is **not** bindable. To show
  those controls, embed the built-in piece with `data-use` — it keeps its own behaviour and confirmations.
- The exact list is locked by the test `bindable_actions_are_exactly_the_safe_set`.
- A `.fltheme` installed from a file or dropped on the window is **not switched on by itself**: the
  launcher asks first (native dialog). Safe settings (`--safe`, Shift at start, tray) always bring back
  the default theme.

## Sharing a theme as one file

A `.fltheme` file is a zip of the theme folder with `theme.json` at the top (a single top folder is also accepted). Drop it onto the launcher window or use Settings → Вид → «Тема из файла» → «Установить…» (then confirm switching it on); «Поделиться» saves the current custom theme as `<id>.fltheme`. Installing runs the same checks as loading, and a broken update never replaces a working theme with the same id. Allowed files: json, html, css, images, fonts, md, txt; up to 512 files and 32 MB.

## What is rejected

- Tags: `script iframe frame object embed link meta base style form portal foreignObject animate set math`.
- Attributes: `on*`, `style`, `srcdoc`, `srcset`, `formaction`, `xlink:href`, `javascript:` / `vbscript:` URLs.
- CSS: `@import`, backslash escapes, `expression(`, `image-set(`, `url()` to anything but theme files or `data:image/…`.
- Token values: `url(`, `;`, braces, `<`, `>`, `\`, `@`, longer than 200 chars.

Everything is checked by the core when a theme is loaded and again when a file is served; the CSP
blocks script and network as a second line of defence.

See `docs/themes/example`.
