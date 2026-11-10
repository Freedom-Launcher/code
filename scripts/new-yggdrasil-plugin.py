#!/usr/bin/env python3
"""Makes a sign-in plugin for a Yggdrasil / authlib-injector service from the template in
docs/addons/yggdrasil-provider. You give it the service's name and API root; it writes a
ready folder (addon.json + config.js + the page) that you can zip and publish.

  python3 scripts/new-yggdrasil-plugin.py --name LittleSkin --root https://littleskin.cn/api/yggdrasil
  python3 scripts/new-yggdrasil-plugin.py --id elyby --name Ely.by --two-factor \\
      --root https://authserver.ely.by/api/authlib-injector --author "Your Name" --out ./my-plugin
"""
import argparse, json, re, shutil, sys
from pathlib import Path
from urllib.parse import urlparse

TEMPLATE = Path(__file__).resolve().parent.parent / "docs" / "addons" / "yggdrasil-provider"
RESERVED = {"app", "accounts", "game", "instances", "mods", "settings", "themes", "addons", "safe", "net", "files"}


def die(msg):
    print("error:", msg, file=sys.stderr)
    sys.exit(2)


def main():
    a = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument("--name", required=True, help="service name shown to the player, e.g. LittleSkin")
    a.add_argument("--root", required=True, help="authlib-injector API root (https), e.g. https://littleskin.cn/api/yggdrasil")
    a.add_argument("--id", help="plugin id a-z0-9- (default: made from the name)")
    a.add_argument("--author", default="", help="author shown in settings")
    a.add_argument("--version", default="0.1.0")
    a.add_argument("--auth-api", help="base URL of the sign-in API if it is not <root>/authserver (Ely.by: https://authserver.ely.by/auth)")
    a.add_argument("--two-factor", action="store_true", help="the service takes 'password:code' (Ely.by does)")
    a.add_argument("--out", help="output folder (default: ./<id>)")
    o = a.parse_args()

    pid = o.id or re.sub(r"[^a-z0-9]+", "-", o.name.lower()).strip("-")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,31}", pid or ""):
        die(f"id {pid!r} must be 1-32 characters a-z 0-9 -")
    if pid in RESERVED:
        die(f"id {pid!r} is reserved by the launcher")
    if not 1 <= len(o.name) <= 60:
        die("name must be 1-60 characters")
    u = urlparse(o.root)
    if u.scheme != "https" or not u.hostname or u.username or u.password or u.port not in (None, 443):
        die("root must be an https URL on the default port without credentials")
    if re.fullmatch(r"[0-9.]+|\[.*\]", u.hostname):
        die("root must use a host name, not an IP address")
    root = o.root.rstrip("/")
    auth = (o.auth_api or f"{root}/authserver").rstrip("/")
    ua = urlparse(auth)
    if ua.scheme != "https" or ua.hostname != u.hostname or ua.username or ua.password or ua.port not in (None, 443):
        die("--auth-api must be an https URL on the same host as --root")

    out = Path(o.out or pid)
    if out.exists():
        die(f"{out} already exists")
    shutil.copytree(TEMPLATE, out)

    m = json.loads((out / "addon.json").read_text(encoding="utf-8"))
    m.update(id=pid, name=o.name, version=o.version, hosts=[u.hostname],
             description=f"Вход через {o.name}",
             account_provider={"label": o.name, "yggdrasil": root, "auth_api": auth})
    if o.author:
        m["author"] = o.author
    (out / "addon.json").write_text(json.dumps(m, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    cfg = (out / "config.js").read_text(encoding="utf-8")
    # json.dumps gives a valid JS string for any name (quotes, backslashes); lambdas keep re from reading escapes.
    cfg = re.sub(r"label: '[^']*'", lambda _: "label: " + json.dumps(o.name, ensure_ascii=False), cfg, count=1)
    cfg = re.sub(r"apiRoot: '[^']*'", lambda _: "apiRoot: " + json.dumps(root), cfg, count=1)
    cfg = re.sub(r"authApi: '[^']*'", lambda _: "authApi: " + json.dumps(auth), cfg, count=1)
    cfg = re.sub(r"twoFactor: ('colon'|null)", lambda _: "twoFactor: " + ("'colon'" if o.two_factor else "null"), cfg, count=1)
    (out / "config.js").write_text(cfg, encoding="utf-8")

    print(f"Plugin '{pid}' written to {out}/")
    print("Next: zip the folder (addon.json at its top) and install it: Settings -> Plugins -> Install plugin.")


if __name__ == "__main__":
    main()
