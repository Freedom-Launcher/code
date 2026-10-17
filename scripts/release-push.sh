#!/bin/sh
# Publishes a finished signed Windows build as Release v<version> in Freedom-Launcher/releases.
# Usage: scripts/release-push.sh <version> [notes]
# Commits the installer, .sig and latest.json on a `staging` branch cut from releases/main;
# .github/workflows/publish.yml there (template: scripts/release-publish.yml) creates the
# Release, uploads the files and deletes `staging`. The in-app updater reads
# https://github.com/Freedom-Launcher/releases/releases/latest/download/latest.json
set -eu
VER="$1"; NOTES="${2:-}"
DIR="target/x86_64-pc-windows-msvc/release/bundle/nsis"
sh scripts/release-feed.sh "$VER" "$NOTES" >/dev/null
REMOTE="${RELEASES_REMOTE:-https://github.com/Freedom-Launcher/releases.git}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
git -C "$W" init -q
git -C "$W" fetch -q --depth 1 "$REMOTE" main
git -C "$W" checkout -q -b staging FETCH_HEAD
[ -f "$W/.github/workflows/publish.yml" ] || { echo "в releases нет .github/workflows/publish.yml (шаблон: scripts/release-publish.yml)" >&2; exit 1; }
A="Freedom-Launcher_${VER}_x64-setup.exe"
cp "$DIR/Freedom Launcher_${VER}_x64-setup.exe" "$W/$A"
cp "$DIR/Freedom Launcher_${VER}_x64-setup.exe.sig" "$W/$A.sig"
cp "$DIR/latest.json" "$W/latest.json"
git -C "$W" add "$A" "$A.sig" latest.json
git -C "$W" -c user.name="${GIT_NAME:-Freedom Launcher}" -c user.email="${GIT_EMAIL:-release@freedom-launcher.local}" commit -q -m "v$VER"
git -C "$W" -c http.postBuffer=524288000 push -q -f "$REMOTE" staging
echo "v$VER → staging; https://github.com/Freedom-Launcher/releases/actions"