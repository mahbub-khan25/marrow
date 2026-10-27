# Marrow for iPhone, iPad and Android

Two native apps that share Marrow's engine with the desktop app:

- `android/` — Kotlin and Jetpack Compose (Material 3), Android 8 and newer.
- `ios/` — Swift and SwiftUI, iOS and iPadOS 17 and newer.
- `../crates/marrow-ffi/` — the engine for phones: one `Marrow` object that
  both apps call through bindings made by UniFFI. Files, search, daily notes,
  tasks, locks, sync (WebDAV or a folder, end-to-end encrypted), version
  history, the graph, canvases and Ask your notes all live here, so the apps
  only draw screens.
- `i18n/strings.py` — every word in both apps, in English and Bangla. Edit it,
  not the generated `strings.xml` or `Localizable.xcstrings`.
- `demo-vault/` — sample notes used for the screenshot tests.

## Android

Needs Rust, JDK 17 or newer, the Android SDK, an NDK (`ANDROID_NDK_HOME`) and
`cargo install cargo-ndk`.

```sh
apps/build-android.sh              # engine for phones, Kotlin bindings, strings
cd apps/android
./gradlew assembleDebug            # app/build/outputs/apk/debug/app-debug.apk
./gradlew testDebugUnitTest        # screenshots of every screen
```

The screenshots run on this computer (Robolectric and Roborazzi) and land in
`app/build/outputs/roborazzi/`. `test/smoke.py` installs the app on an
emulator or phone and taps through it.

## iPhone and iPad

Needs a Mac with Xcode 16, Rust and `brew install xcodegen`.

```sh
apps/ios/build-rust.sh             # engine, Swift bindings, string catalog
cd apps/ios && xcodegen            # makes Marrow.xcodeproj
open Marrow.xcodeproj
```

The UI test `ScreenshotTests` opens each screen with the demo vault; run it
with `DEMO_VAULT` and `SHOTS_DIR` set to save the pictures.

## Bindings

Both build scripts run the bindings generator. To run it alone:

```sh
cargo build -p marrow-ffi
cargo run -p marrow-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library target/debug/libmarrow_ffi.so --language kotlin --out-dir OUT
```

Use `--language swift` (and the `.dylib` on a Mac) for iOS.
