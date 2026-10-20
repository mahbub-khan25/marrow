# Releasing Marrow

## Where things stand (0.9.0)

Every blocker from the readiness reviews is fixed and covered by tests:

| Area | Status | How it's checked |
|---|---|---|
| Crash-safe saves | Done | `crashtest` kills Marrow 200× mid-save (Windows: 0 damaged; plain overwrite: 35) |
| Unsaved text on crash | Draft at least every 5 s while typing, offered back | `npm test` (docs lifecycle) |
| Failed saves | Block close / switch / lock / trash; ask first | `npm test` |
| Interrupted renames | Finished on next open | `cargo test` (rename journal) |
| Locked folders | Sealed on create, import, move; fail closed | `cargo test -p marrow-app` |
| Canvas conflicts | Both versions kept, edits during the conflict too | `npm test` (canvas) |
| Portable rebuild | Never deletes Notes/Data | script review |
| Freezing | Commands run in the background; very large renames still make other actions wait | manual check below |

Speed on a generated vault (Linux, release build; Windows is usually a bit slower):

| | 10,000 notes (22 MB) | 50,000 notes (108 MB) |
|---|---|---|
| Open, first time | 0.6 s | 3.7 s |
| Open again | 0.35 s | 1.6 s |
| First search / next search | 0.3 s / 17 ms | 1.6 s / 77 ms |
| Database folder | 2,000 rows: 39 ms | 10,000 rows: 0.2 s |
| Save one note | 7 ms | 35 ms |

Run it yourself: `cargo run --release --example bench -p marrow-core -- 10000`.

### Known limits (fine for 1.0, worth doing later)
- A database view draws every row; above a few thousand rows scrolling gets slow.
- Search scans note text (fast enough to ~50,000 notes; an index would help beyond).
- Renaming a note that thousands of notes link to takes a while (about 2 ms per note).
- No automatic updates yet (see below).

## Automated checks

- **CI** (`.github/workflows/ci.yml`) runs on every push: type check, window
  tests, engine and app tests, lint, the crash test and a build, on Windows,
  macOS and Linux.
- **Release** (`.github/workflows/release.yml`): push a tag such as `v0.9.0`
  and GitHub builds the Windows installers (.exe, .msi) and a macOS app (.dmg,
  Apple Silicon and Intel) and attaches them to a draft release.
- **On your PC**: double-click `scripts\windows-check.cmd`. It runs every
  check, ends with a clear PASS or FAIL (details in `windows-check.log`),
  and starts Marrow only if everything passed.

## Linux packages

The release builds three Linux files, and the "Linux packages" workflow
installs them on fresh Fedora and Ubuntu machines to prove they work:

| File | For | Notes |
|---|---|---|
| `Marrow-<version>-1.x86_64.rpm` | Fedora 40 and newer, openSUSE Tumbleweed | `sudo dnf install ./Marrow-*.rpm` pulls in what it needs |
| `Marrow_<version>_amd64.deb` | Ubuntu 22.04+, Debian 12+, Linux Mint 21+ | `sudo apt install ./Marrow_*.deb` |
| `Marrow_<version>_amd64.AppImage` | Most other desktops | Needs a system as new as Ubuntu 22.04 |

**Red Hat Enterprise Linux (and Rocky/Alma):** Marrow needs the WebKitGTK 4.1
web engine. RHEL 9 only ships the older 4.0, and RHEL 10's standard
repositories ship no WebKitGTK at all, so the .rpm can't install there; the
AppImage doesn't run on RHEL 9 either (it needs a newer C library). The way
to support Red Hat is a **Flatpak** (Flathub), which brings its own web engine
and runs on RHEL 9 and 10 as well as every other distribution.

## Steps only you can do

These need accounts, money or decisions, so they're left to you.

1. **Put the code on GitHub** (private is fine) so the workflows run.
2. **Windows signing.** Without it, Windows SmartScreen warns "unknown
   publisher" on first run. Cheapest good option: Azure Trusted Signing (about
   US$10/month, needs ID verification). Then add a `signCommand` under
   `bundle > windows` in `src-tauri/tauri.conf.json`. An OV certificate from a
   certificate authority works too.
3. **macOS signing and notarization.** Needs the Apple Developer Program
   (US$99/year). Add the `APPLE_*` secrets listed in `release.yml` to the
   GitHub repository; the release workflow then signs and notarizes the app.
4. **Automatic updates** (optional for a beta). Needs a signing key pair
   (`npx tauri signer generate`) and somewhere to host `latest.json` (GitHub
   Releases works). Ask Claude to wire up `tauri-plugin-updater` once you have
   the key.
5. **A page for people**: a download page, a privacy statement (Marrow sends
   nothing anywhere unless you connect an AI; the log stays on the computer),
   and an email address for bug reports.
6. **Version**: `0.9.0` is the beta. After the beta, set `1.0.0` in
   `Cargo.toml` (workspace), `package.json` and `src-tauri/tauri.conf.json`.

## Manual check before each release (15 minutes)

Use a fresh vault on a clean Windows PC (or a new Windows user account):

1. Install from the `.exe`. Create a vault. Make three notes that link to each other.
2. Type in a note, then end Marrow from Task Manager straight away. Reopen:
   the text you typed is offered back.
3. Make a note read-only in Explorer (Properties → Read-only) and type in it:
   an error says why; closing Marrow asks before leaving the text behind.
4. Rename a note that others link to: links update. Undo with another rename.
5. Lock a folder. Create a database row in it, move a note into it, import a
   Markdown file into it: open each `.md` in Notepad, it's unreadable.
6. Edit a canvas in Marrow and, at the same time, the same `.canvas` file in
   Notepad: both versions survive.
7. Search, the graph, a database board and slides all open without the window
   freezing.
8. Settings → About → Show the log: the log opens and contains no note text.
9. Uninstall: the vault folder and its notes are untouched.

Then start a small private beta (5–10 people), fix what they hit, and release
1.0 after a couple of quiet weeks.
