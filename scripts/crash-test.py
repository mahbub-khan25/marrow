"""Kills a process at random moments while it saves a note, many times,
and checks the note afterwards. A crash-safe save always leaves one whole
version; a plain overwrite sometimes leaves a cut-off or empty file.

    cargo build --release --example crashtest
    python scripts/crash-test.py [runs]
"""
import os, random, subprocess, sys, tempfile, time

exe = os.path.join("target", "release", "examples", "crashtest" + (".exe" if os.name == "nt" else ""))
runs = int(sys.argv[1]) if len(sys.argv) > 1 else 200
SIZE = 4 << 20

def check(mode):
    bad = 0
    with tempfile.TemporaryDirectory() as d:
        f = os.path.join(d, "Note.md")
        with open(f, "w") as h:
            h.write("a" * SIZE)
        for _ in range(runs):
            p = subprocess.Popen([exe, f, mode])
            time.sleep(random.uniform(0.02, 0.25))
            p.kill()
            p.wait()
            data = open(f, "rb").read()
            whole = len(data) == SIZE and (data == b"a" * SIZE or data == b"b" * SIZE)
            bad += not whole
    return bad

for mode in ("plain", "atomic"):
    print(f"{mode:>6}: {check(mode)} damaged notes out of {runs} crashes")
