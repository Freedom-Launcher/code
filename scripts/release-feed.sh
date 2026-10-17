#!/bin/sh
# Builds latest.json for the in-app updater from a finished Windows build.
# Usage: TAURI_SIGNING_PRIVATE_KEY=<path> scripts/release-feed.sh <version> [notes]
# Upload the setup .exe and latest.json to a release of Freedom-Launcher/releases.
set -eu
VER="$1"; NOTES="${2:-}"
DIR="target/x86_64-pc-windows-msvc/release/bundle/nsis"
EXE="Freedom Launcher_${VER}_x64-setup.exe"
SIG="$DIR/$EXE.sig"
[ -f "$SIG" ] || { echo "нет $SIG — соберите с TAURI_SIGNING_PRIVATE_KEY" >&2; exit 1; }
URL="https://github.com/Freedom-Launcher/releases/releases/download/v${VER}/$(echo "$EXE" | sed 's/ /-/g')"
python3 - "$VER" "$NOTES" "$URL" "$SIG" > "$DIR/latest.json" <<'PY'
import json, sys, datetime
ver, notes, url, sig = sys.argv[1:]
print(json.dumps({
    "version": ver,
    "notes": notes,
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {"windows-x86_64": {"signature": open(sig).read().strip(), "url": url}},
}, ensure_ascii=False, indent=2))
PY
echo "$DIR/latest.json"