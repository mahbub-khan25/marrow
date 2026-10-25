#!/usr/bin/env bash
# Refreshes Marrow's copy of the Ekushe Next typing engine (Bangla typing).
# Needs the website repository next to this one (../ekushenext-website); run its
# engine/build.sh first if the engine itself changed.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
WEB="${EKUSHE_WEBSITE:-$HERE/../ekushenext-website}"
MAIN="${EKUSHE_REPO:-$HERE/../ekushenext}"
cp "$WEB/site/engine/ekushe.wasm" "$WEB/site/engine/ekushe-web.dict.z" "$HERE/ui/public/ekushe/"
{ printf '%s\n' "// From Ekushe Next (https://github.com/mahbub-khan25/ekushenext-website, site/engine/ekushe.js)," "// MPL-2.0. The engine itself is ui/public/ekushe/ekushe.wasm: ekushe-core built for the web."; cat "$WEB/site/engine/ekushe.js"; } > "$HERE/ui/src/vendor/ekushe.js"
if [ -d "$MAIN" ]; then
  cp "$MAIN/LICENSE" "$HERE/ui/public/ekushe/LICENSE-MPL-2.0.txt"
  cp "$MAIN/data/dictionary/LICENSE.md" "$HERE/ui/public/ekushe/DICTIONARY-LICENSE.md"
  cp "$MAIN/data/dictionary/CC-BY-SA-4.0.txt" "$HERE/ui/public/ekushe/CC-BY-SA-4.0.txt"
fi
echo "Updated. Run: npm test"
