# Release checklist (by hand, on Windows)

Automated tests cover the logic; these are the things only a real window, a real keyring and a real
service can show. Tick them before publishing a release (`scripts/release-feed.sh`, `scripts/release-push.sh`).

## Update
- [ ] Install the previous release, then update from inside the launcher (Settings → Лаунчер → Обновления). Accounts, instances and settings are still there afterwards.
- [ ] A clean install on a machine without the launcher starts and shows the first-run screen.

## Play
- [ ] Sign in with Microsoft, start vanilla and one Fabric instance, quit, start again (token refresh).
- [ ] Mods: install one from Modrinth into an instance and start it.

## Themes and confirmations
- [ ] Install a `.fltheme` by dropping it on the window: a native box asks whether to switch it on; «Отмена» keeps the old theme.
- [ ] A theme with a `data-action="game.launch"` button: clicking it shows the native box with three buttons (Разрешить / Всегда для этой темы / Отмена); each one does what it says.
- [ ] After «Всегда для этой темы»: Settings → Вид shows «Доверять этой теме» on; edit one file of the theme and the box comes back.
- [ ] Settings → Плагины → «Подтверждения»: turning it off asks in a native box; with it off, themes launch the game without asking.
- [ ] Safe mode (hold Shift at start, or the tray entry): the button «Включить подтверждения для тем» works and there is no way to turn them off there.
- [ ] A theme with a `shell` template and a rebuilt `view.settings`: everything is reachable; Ctrl+Alt+Shift+T restores the default theme from anywhere.

## Plugins and other sign-ins
- [ ] Install the Ely.by plugin from `docs/addons/elyby` (zip with `addon.json` at the top): the consent box warns that it is unverified and wants a third-party sign-in.
- [ ] Sign in with a real Ely.by account (also one with two-factor): the account appears; start the game with it; skin and cape show up in game.
- [ ] Next day (or after changing the system clock) start again: no new sign-in is asked, the token was refreshed by the launcher.

- [ ] Ely.by plugin installed but not switched on: the accounts page says so («плагин входа установлен, но выключен») and the button leads to switching it on.
- [ ] The sign-in page of the plugin looks right in the light and the dark theme, the 2FA field is hidden until the service asks, and the «Войти» button and Enter both work.

## Plugin sources, updates, developer mode
- [ ] Плагины → Каталог → «Добавить источник» with a real repository (`владелец/репозиторий` with a `freedom-plugins.json`): its plugins appear marked «Сообщество»; the chips filter by source; removing the source removes them.
- [ ] Install a plugin from that source, then publish a higher version there: the banner «Доступны обновления» and «Обновить до vX» appear, the update installs, a new permission waits for approval.
- [ ] Settings → Плагины → «Режим разработчика»: the «Разработка» tab appears; «Создать плагин…» makes a folder named like its id; edit a file and the plugin reloads within a second; a deliberate error shows in the log.
- [ ] A linked folder with a name different from its id shows the error and installs nothing.

## Doctor
- [ ] A build with a broken or duplicate mod shows the small «Доктор · N» button (not a red banner); a click opens the details and «Исправить всё».
- [ ] Settings → Параметры сборок → «Доктор сборок» off: no button, no menu entries, no check before launch; on again: they return.

## Other systems
- [ ] Linux or macOS with a keyring: `cargo test -p fl-core --test keychain_live -- --ignored --test-threads=1` passes, and `accounts.json` holds no tokens.
- [ ] Windows without a Mojang Java runtime for the chosen version: the launcher installs one and the game starts.
