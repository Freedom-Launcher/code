#!/usr/bin/env python3
"""Adds (or updates) your plugin in the catalog file of your repository, so players can add the
repository once and get your plugin in their launcher.

  1. Zip the plugin folder (addon.json at the top or inside one top folder) and publish the zip,
     for example as a GitHub release asset.
  2. Run:  python3 scripts/plugin-catalog-entry.py my-plugin.zip \\
             --url https://github.com/you/repo/releases/download/v1.0.0/my-plugin.zip
  3. Commit the written freedom-plugins.json to the root of the repository.

Players then type `you/repo` under «Плагины → Каталог → Добавить источник».
The id, name, version, author, description and permissions come from the plugin's addon.json; the
SHA-256 and size are computed from the zip, so the launcher can check the download.
"""
import argparse, hashlib, json, re, sys, zipfile
from pathlib import Path
from urllib.parse import urlparse

SOURCE_FILE = "freedom-plugins.json"


def die(msg):
    print("error:", msg, file=sys.stderr)
    sys.exit(2)


def manifest(z):
    names = z.namelist()
    if "addon.json" in names:
        path = "addon.json"
    else:
        tops = {n.split("/")[0] for n in names}
        path = next((f"{t}/addon.json" for t in tops if f"{t}/addon.json" in names and len(tops) == 1), None)
        if not path:
            die("addon.json not found at the top of the archive or inside one top folder")
    try:
        return json.loads(z.read(path).decode("utf-8"))
    except Exception as e:  # noqa: BLE001
        die(f"addon.json is not valid JSON: {e}")


def main():
    a = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument("zip", help="the plugin archive")
    a.add_argument("--url", required=True, help="https URL players download this exact zip from")
    a.add_argument("--catalog", default=SOURCE_FILE, help=f"catalog file to create or update (default: ./{SOURCE_FILE})")
    a.add_argument("--name", help="name of the whole catalog (shown next to the plugins); set once")
    o = a.parse_args()

    u = urlparse(o.url)
    if u.scheme != "https" or not u.hostname or u.username or u.password or u.port not in (None, 443):
        die("--url must be an https URL on the default port without credentials")
    if re.fullmatch(r"[0-9.]+|\[.*\]", u.hostname):
        die("--url must use a host name, not an IP address")

    data = Path(o.zip).read_bytes()
    with zipfile.ZipFile(o.zip) as z:
        m = manifest(z)
    pid = m.get("id", "")
    if not re.fullmatch(r"[a-z0-9-]{1,64}", pid):
        die(f"addon.json id {pid!r} must be 1-64 characters a-z 0-9 -")
    entry = {
        "id": pid,
        "name": m.get("name", pid),
        "version": str(m.get("version", "0.0.0")),
        "author": m.get("author", ""),
        "description": m.get("description", ""),
        "permissions": list(m.get("permissions", [])),
        "url": o.url,
        "sha256": hashlib.sha256(data).hexdigest(),
        "size": len(data),
    }

    cat = Path(o.catalog)
    feed = json.loads(cat.read_text(encoding="utf-8")) if cat.exists() else {"plugins": []}
    if o.name:
        feed["name"] = o.name
    feed.setdefault("plugins", [])
    feed["plugins"] = [p for p in feed["plugins"] if p.get("id") != pid] + [entry]
    cat.write_text(json.dumps(feed, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"{pid} {entry['version']}: {entry['sha256'][:12]}… ({entry['size']} bytes) written to {cat}")
    print("permissions players will see:", ", ".join(entry["permissions"]) or "none")


if __name__ == "__main__":
    main()
