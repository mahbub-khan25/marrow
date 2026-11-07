#!/usr/bin/env python3
"""Makes the store packages of Marrow Clipper.

    python3 clipper/pack.py 1.8.0 out/

writes out/Marrow_Clipper_1.8.0_chrome.zip (Chrome, Edge, Brave, Opera) and
out/Marrow_Clipper_1.8.0_firefox.zip. They differ only in the manifest:
Chrome wants a service worker and no Firefox settings; Firefox wants
background scripts.
"""
import json
import pathlib
import sys
import zipfile

here = pathlib.Path(__file__).resolve().parent
version, out = sys.argv[1], pathlib.Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
base = json.loads((here / "manifest.json").read_text(encoding="utf-8"))
base["version"] = version

def files():
    for f in sorted(here.rglob("*")):
        rel = f.relative_to(here)
        if f.is_file() and rel.parts[0] != "store" and f.suffix not in (".md", ".py"):
            yield f, rel.as_posix()

for store in ("chrome", "firefox"):
    m = json.loads(json.dumps(base))
    if store == "chrome":
        m["background"] = {"service_worker": "background.js", "type": "module"}
        m.pop("browser_specific_settings", None)
    else:
        m["background"] = {"scripts": ["background.js"], "type": "module"}
    path = out / f"Marrow_Clipper_{version}_{store}.zip"
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("manifest.json", json.dumps(m, indent=2, ensure_ascii=False) + "\n")
        for f, rel in files():
            if rel != "manifest.json":
                z.write(f, rel)
    print(path)
