#!/usr/bin/env python3
"""Lists words in the iOS app's code that the string catalog doesn't translate.

    python3 apps/i18n/check_swift.py
"""
import json, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.join(HERE, "..", "ios", "Marrow")
catalog = json.load(open(os.path.join(APP, "Localizable.xcstrings"), encoding="utf-8"))["strings"]

# Calls whose first string literal is looked up in the catalog.
CALLS = r'(?:Text|Button|Label|TextField|SecureField|Toggle|Picker|Section|Link|String\(localized:|SectionTitle\(text:|navigationTitle|alert|confirmationDialog|chip|tool|row|option|source|sectionHead|title:|text:|how:)'
lit = re.compile(CALLS + r'\s*\(?\s*"((?:[^"\\]|\\.)*)"')
missing = set()
for root, _, files in os.walk(APP):
    for f in files:
        if not f.endswith(".swift"):
            continue
        src = open(os.path.join(root, f), encoding="utf-8").read()
        src = re.sub(r'verbatim:\s*"(?:[^"\\]|\\.)*"', "", src)
        for m in lit.finditer(src):
            s = m.group(1)
            if not re.search(r"[A-Za-z]", s) or s.startswith("-") or "/" in s and " " not in s:
                continue
            # Interpolations become %@ (or %lld for numbers; both are tried).
            keys = {re.sub(r"\\\((.*?)\)", "%@", s), re.sub(r"\\\((.*?)\)", "%lld", s)}
            keys |= {re.sub(r"\\\((Int\(.*?\)|\d+|n|count|.*count.*)\)", "%lld", s)}
            if not any(k in catalog for k in keys):
                missing.add(s)
for s in sorted(missing):
    print(s)
print(f"{len(missing)} missing", file=sys.stderr)
