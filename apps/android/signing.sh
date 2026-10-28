#!/usr/bin/env bash
# In CI: turns the MARROW_KEYSTORE_BASE64 secret (the release key file, as
# text) into a file, and points the build at it. Without the secret, release
# builds are signed with a debug key instead: they install, but an update
# signed with the real key later needs the test version uninstalled first.
set -euo pipefail
if [ -z "${KEYSTORE_BASE64:-}" ]; then
  echo "No release key set up: the APK is signed with a debug key (for trying it out)."
  exit 0
fi
file="$RUNNER_TEMP/marrow-upload.jks"
echo "$KEYSTORE_BASE64" | base64 -d > "$file"
echo "MARROW_KEYSTORE=$file" >> "$GITHUB_ENV"
echo "Signing with the release key."
