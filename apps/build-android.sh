#!/usr/bin/env bash
# Builds Marrow's engine for Android phones and tablets, and writes its Kotlin
# bindings into the app. Needs Rust, an Android NDK (ANDROID_NDK_HOME) and
# cargo-ndk (`cargo install cargo-ndk`).
#   apps/build-android.sh            # every phone type
#   apps/build-android.sh arm64-v8a  # only one (faster while working on it)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/.."
APP="$HERE/android/app"
ABIS=("$@")
[ ${#ABIS[@]} -eq 0 ] && ABIS=(arm64-v8a armeabi-v7a x86_64)
declare -A TRIPLE=([arm64-v8a]=aarch64-linux-android [armeabi-v7a]=armv7-linux-androideabi [x86_64]=x86_64-linux-android)
targets=()
for a in "${ABIS[@]}"; do targets+=("${TRIPLE[$a]}"); done
rustup target add "${targets[@]}" >/dev/null
args=()
for a in "${ABIS[@]}"; do args+=(-t "$a"); done
cd "$ROOT"
cargo ndk "${args[@]}" -P 26 -o "$APP/src/main/jniLibs" build --release -p marrow-ffi
# The bindings come from the engine built for this computer (also used by
# the screenshot tests, which run on this computer).
cargo build -p marrow-ffi
lib=target/debug/libmarrow_ffi.so
[ -f "$lib" ] || lib=target/debug/libmarrow_ffi.dylib
rm -rf "$APP/src/generated/kotlin"
cargo run -q -p marrow-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library "$lib" --language kotlin --out-dir "$APP/src/generated/kotlin" --no-format
# cargo-ndk copies every library it finds; only Marrow's engine is needed.
find "$APP/src/main/jniLibs" -name 'libredb*' -delete
python3 "$HERE/i18n/strings.py"
ls -la "$APP/src/main/jniLibs"/*/
