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
- [ ] Install the Ely.by plugin from `docs/addons/yggdrasil-provider` (zip with `addon.json` at the top): the consent box warns that it is unverified and wants a third-party sign-in.
- [ ] Sign in with a real Ely.by account (also one with two-factor): the account appears; start the game with it; skin and cape show up in game.
- [ ] Next day (or after changing the system clock) start again: no new sign-in is asked, the token was refreshed by the launcher.

## Other systems
- [ ] Linux or macOS with a keyring: `cargo test -p fl-core --test keychain_live -- --ignored --test-threads=1` passes, and `accounts.json` holds no tokens.
- [ ] Windows without a Mojang Java runtime for the chosen version: the launcher installs one and the game starts.
