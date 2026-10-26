#!/usr/bin/env python3
"""Packs the official plugins for the in-launcher catalog.

    python3 scripts/plugins-pack.py <seed file> <out dir>

For every folder in `plugins/`: copies it, signs it as «Официальный» with the
project key (`sign_addon` example), zips it deterministically to `<id>-<version>.zip`
and writes `catalog.json`. Push the output to `plugins/` of Freedom-Launcher/releases
(main branch). Likes: one issue per plugin labelled `plugin` (ISSUES below).
"""
import hashlib, json, pathlib, shutil, subprocess, sys, zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
RAW = "https://raw.githubusercontent.com/Freedom-Launcher/releases/main/plugins/"
ISSUES = {"mclogs": 1, "notes": 2}
HUES = {"mclogs": 18, "notes": 45}


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    seed, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    work = out / ".work"
    shutil.rmtree(work, ignore_errors=True)
    plugins = []
    for src in sorted(p for p in (ROOT / "plugins").iterdir() if p.is_dir()):
        m = json.loads((src / "addon.json").read_text(encoding="utf-8"))
        pid, ver = m["id"], m["version"]
        assert pid == src.name, f"{src.name}: id {pid}"
        d = work / pid
        shutil.copytree(src, d)
        r = subprocess.run(["cargo", "run", "-q", "-p", "fl-core", "--example", "sign_addon", "--",
                            str(d), str(seed), "official"], cwd=ROOT, capture_output=True, text=True)
        if r.returncode != 0 or not (d / "signature.json").is_file():
            sys.exit(f"{pid}: signing failed (exit {r.returncode})")
        name = f"{pid}-{ver}.zip"
        z = out / name
        with zipfile.ZipFile(z, "w", zipfile.ZIP_DEFLATED) as zf:
            for f in sorted(p for p in d.rglob("*") if p.is_file()):
                info = zipfile.ZipInfo(f.relative_to(d).as_posix(), (2026, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o644 << 16
                zf.writestr(info, f.read_bytes())
        b = z.read_bytes()
        plugins.append({
            "id": pid, "name": m["name"], "author": m.get("author", ""),
            "description": m.get("description", ""), "version": ver,
            "hue": HUES.get(pid), "permissions": m.get("permissions", []),
            "url": RAW + name, "sha256": hashlib.sha256(b).hexdigest(), "size": len(b),
            "asset": name, "issue": ISSUES.get(pid, 0),
        })
        print(f"{name}: {len(b)} bytes")
    shutil.rmtree(work, ignore_errors=True)
    (out / "catalog.json").write_text(json.dumps({"plugins": plugins}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"catalog.json: {len(plugins)} plugins")


main()