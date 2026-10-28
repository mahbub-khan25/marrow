#!/usr/bin/env python3
"""Starts Marrow on an emulator (or a connected phone) and walks through it:
makes a vault, opens Today, Notes, a note, Tasks and Search, saving a
screenshot of each. Fails if the app stops or Android reports a crash.

    python3 apps/android/test/smoke.py app-debug.apk screens/
"""
import os
import re
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

APP = "app.marrow.notes"


def adb(*args, check=False):
    r = subprocess.run(["adb", *args], capture_output=True, text=True)
    if check and r.returncode:
        sys.exit(f"adb {' '.join(args)} failed: {r.stderr}")
    return r.stdout


def shot(out, name):
    with open(os.path.join(out, f"{name}.png"), "wb") as f:
        f.write(subprocess.run(["adb", "exec-out", "screencap", "-p"], capture_output=True).stdout)


def find(text):
    adb("shell", "uiautomator", "dump", "/sdcard/ui.xml")
    xml = adb("shell", "cat", "/sdcard/ui.xml")
    try:
        root = ET.fromstring(xml[xml.find("<"):])
    except ET.ParseError:
        return None
    for n in root.iter("node"):
        if n.get("text") == text or n.get("content-desc") == text:
            x1, y1, x2, y2 = map(int, re.findall(r"\d+", n.get("bounds")))
            return (x1 + x2) // 2, (y1 + y2) // 2
    return None


def dismiss_system_dialogs():
    """A slow emulator sometimes shows "… isn't responding" for its own apps."""
    for button in ("Wait", "Close app"):
        at = find(button)
        if at and find("Wait"):
            adb("shell", "input", "tap", str(at[0]), str(at[1]))
            time.sleep(2)
            return


def tap(text, wait=3):
    for _ in range(10):
        dismiss_system_dialogs()
        at = find(text)
        if at:
            adb("shell", "input", "tap", str(at[0]), str(at[1]))
            time.sleep(wait)
            return
        time.sleep(1)
    sys.exit(f"Couldn't find “{text}” on screen")


def alive():
    if not adb("shell", "pidof", APP).strip():
        print(adb("logcat", "-d", "-s", "AndroidRuntime:E", "*:F")[-4000:])
        sys.exit("Marrow stopped")


def main():
    apk, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    adb("install", "-r", apk, check=True)
    adb("logcat", "-c")
    adb("shell", "am", "start", "-n", f"{APP}/app.marrow.MainActivity", check=True)
    time.sleep(8)
    dismiss_system_dialogs()
    alive()
    shot(out, "1-welcome")
    tap("Start writing", 6)
    alive()
    shot(out, "2-today")
    tap("Notes")
    shot(out, "3-notes")
    tap("Welcome", 4)
    alive()
    shot(out, "4-note")
    adb("shell", "input", "keyevent", "KEYCODE_BACK")
    time.sleep(2)
    tap("Tasks")
    shot(out, "5-tasks")
    tap("Search")
    adb("shell", "input", "text", "Ideas")
    time.sleep(3)
    shot(out, "6-search")
    alive()
    crashes = adb("logcat", "-d", "-s", "AndroidRuntime:E")
    if "FATAL EXCEPTION" in crashes:
        print(crashes[-4000:])
        sys.exit("Android reported a crash")
    print("Marrow ran through every screen.")


if __name__ == "__main__":
    main()
