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
  **bindable** action with theme rights. While it runs the element gets `data-state` =
  `loading` / `success` / `error`, so states are styled in CSS only.
- `href` / `src` may point only to theme files or `#…`.

## Sharing a theme as one file

A `.fltheme` file is a zip of the theme folder with `theme.json` at the top (a single top folder is also accepted). Drop it onto the launcher window or use Settings → Вид → «Тема из файла» → «Установить…»; «Поделиться» saves the current custom theme as `<id>.fltheme`. Installing runs the same checks as loading, and a broken update never replaces a working theme with the same id. Allowed files: json, html, css, images, fonts, md, txt; up to 512 files and 32 MB.

## What is rejected

- Tags: `script iframe frame object embed link meta base style form portal foreignObject animate set math`.
- Attributes: `on*`, `style`, `srcdoc`, `srcset`, `formaction`, `xlink:href`, `javascript:` / `vbscript:` URLs.
- CSS: `@import`, backslash escapes, `expression(`, `image-set(`, `url()` to anything but theme files or `data:image/…`.
- Token values: `url(`, `;`, braces, `<`, `>`, `\`, `@`, longer than 200 chars.

Everything is checked by the core when a theme is loaded and again when a file is served; the CSP
blocks script and network as a second line of defence.

See `docs/themes/example`.
