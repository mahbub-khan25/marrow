#!/usr/bin/env bash
# Builds Marrow's engine for iPhone and iPad (and the Simulator), writes its
# Swift bindings into Generated/, and the app's words into the string
# catalog. Needs a Mac with Xcode and Rust. Then:  cd apps/ios && xcodegen
#   ./build-rust.sh        # devices and the Simulator
#   ./build-rust.sh sim    # only the Simulator on this Mac (faster)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$HERE/../.."
export IPHONEOS_DEPLOYMENT_TARGET=17.0
cd "$ROOT"
host=$(uname -m)
if [ "${1:-}" = "sim" ]; then
  [ "$host" = "arm64" ] && sims=(aarch64-apple-ios-sim) || sims=(x86_64-apple-ios)
  devices=()
else
  sims=(aarch64-apple-ios-sim x86_64-apple-ios)
  devices=(aarch64-apple-ios)
fi
rustup target add "${sims[@]}" "${devices[@]}" >/dev/null
for t in "${sims[@]}" "${devices[@]}"; do
  cargo build --release -p marrow-ffi --target "$t"
done
mkdir -p "$HERE/rust/iphoneos" "$HERE/rust/iphonesimulator"
if [ ${#devices[@]} -gt 0 ]; then
  cp target/aarch64-apple-ios/release/libmarrow_ffi.a "$HERE/rust/iphoneos/"
fi
libs=()
for t in "${sims[@]}"; do libs+=("target/$t/release/libmarrow_ffi.a"); done
lipo -create "${libs[@]}" -output "$HERE/rust/iphonesimulator/libmarrow_ffi.a"
# Bindings, from the engine built for this Mac.
cargo build -p marrow-ffi
rm -rf "$HERE/Generated" && mkdir -p "$HERE/Generated"
cargo run -q -p marrow-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/debug/libmarrow_ffi.dylib --language swift --out-dir "$HERE/Generated" --no-format
mv "$HERE/Generated/MarrowCoreFFI.modulemap" "$HERE/Generated/module.modulemap"
# The version, from Cargo.toml.
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
echo "MARKETING_VERSION = $version" > "$HERE/Generated/Version.xcconfig"
python3 "$HERE/../i18n/strings.py"
ls -la "$HERE/rust"/*/ "$HERE/Generated"
