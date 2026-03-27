# Packaging Checklist (macOS DMG + Windows NSIS, unsigned)

## Pre-build

1. Install Node.js LTS, Rust toolchain, and Tauri OS dependencies.
2. Ensure code is formatted and tests pass:
   - `npm test`
   - `cd src-tauri && cargo test`
3. Verify app permissions/capabilities are minimal and reviewed.

## Build artifacts

1. Build unsigned macOS DMG:
   - `npm run release:macos:unsigned`
2. Build unsigned Windows NSIS installer (run on Windows):
   - `npm run release:windows:unsigned`
3. Verify generated installer:
   - macOS: `src-tauri/target/release/bundle/dmg/*.dmg`
   - Windows: `src-tauri/target/release/bundle/nsis/*.exe`

## Installation notes (Gatekeeper)

1. First launch may be blocked because the app is unsigned.
2. Use Right-click on the app, then `Open`, then confirm.
3. Alternative path:
   - `System Settings > Privacy & Security > Open Anyway`
4. If `Open Anyway` does not appear, remove quarantine from the installed app:
   - `xattr -dr com.apple.quarantine "/Applications/Dataverse Uploader Universal (DVU_U).app"`
5. This package is for internal/testing distribution only and is not a publicly trusted macOS build.

## Installation notes (Windows SmartScreen)

1. First launch may show a SmartScreen warning because the installer is unsigned.
2. Click `More info` then `Run anyway` to continue internal/testing installs.
3. This package is for internal/testing distribution only and is not a publicly trusted Windows build.

## Release validation

1. Fresh install and first-run checks.
2. Destination connection test and dataset validation.
3. Large transfer smoke test (classic and direct upload fallback path).
4. Interruption recovery test (close app mid-upload, reopen, resume).
5. Final report export validation (JSON + CSV).
