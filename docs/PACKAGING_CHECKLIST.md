# Packaging Checklist (macOS + Windows)

## Pre-build

1. Install Node.js LTS, Rust toolchain, and Tauri OS dependencies.
2. Ensure code is formatted and tests pass:
   - `npm test`
   - `cd src-tauri && cargo test`
3. Verify app permissions/capabilities are minimal and reviewed.

## Build artifacts

1. Build app bundles:
   - `npm run tauri:build`
2. Verify generated installers:
   - macOS: `.dmg` / `.app`
   - Windows: `.msi` / `.exe`

## Signing

1. macOS: sign app and installer with Developer ID certificates.
2. Windows: sign installer/executables with Authenticode certificate.
3. Re-verify signatures post-build.

## Release validation

1. Fresh install and first-run checks.
2. Destination connection test and dataset validation.
3. Large transfer smoke test (classic and direct upload fallback path).
4. Interruption recovery test (close app mid-upload, reopen, resume).
5. Final report export validation (JSON + CSV).
