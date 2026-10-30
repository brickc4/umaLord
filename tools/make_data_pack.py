"""Build the weekly data pack the app downloads (app/src/datapack.rs).

Run from the repo root after tools/update_knowledge.py and the tests:

    python -I -X utf8 tools/make_data_pack.py

Writes dist/data-pack.zip (knowledge/data, with sim/, and the uma and card art) and
dist/data-pack.json, the manifest: the schema, when it was built, the zip's SHA-256 and each
file's path, size and SHA-256. The app compares each file's hash with its own copy and takes only
the ones that differ. .github/workflows/data-pack.yml runs this weekly and publishes both as the
"data-pack" release.
"""

import datetime as dt
import hashlib
import json
import os
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIST = os.path.join(ROOT, "dist")
# The app refuses a pack whose schema it doesn't know (datapack::SCHEMA).
SCHEMA = 1
PARTS = [("knowledge/data", (".json",)), ("app/assets/umas", (".webp",)), ("app/assets/cards", (".webp",))]


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 16), b""):
            h.update(block)
    return h.hexdigest()


def main() -> None:
    files = []
    for base, exts in PARTS:
        for dirpath, _, names in os.walk(os.path.join(ROOT, base)):
            for n in sorted(names):
                if n.endswith(exts):
                    full = os.path.join(dirpath, n)
                    rel = os.path.relpath(full, ROOT).replace(os.sep, "/")
                    files.append({"path": rel, "size": os.path.getsize(full), "sha256": sha256(full)})
    files.sort(key=lambda f: f["path"])
    os.makedirs(DIST, exist_ok=True)
    zip_path = os.path.join(DIST, "data-pack.zip")
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
        for f in files:
            z.write(os.path.join(ROOT, f["path"]), f["path"])
    meta = json.load(open(os.path.join(ROOT, "knowledge/data/meta.json"), encoding="utf-8"))
    manifest = {
        "schema": SCHEMA,
        "built": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "counts": meta.get("counts", {}),
        "zip_sha256": sha256(zip_path),
        "files": files,
    }
    with open(os.path.join(DIST, "data-pack.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    print(f"data pack: {len(files)} files, {os.path.getsize(zip_path) // 1024} KiB, counts {manifest['counts']}")


if __name__ == "__main__":
    main()
