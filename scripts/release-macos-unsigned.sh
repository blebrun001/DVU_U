#!/usr/bin/env bash

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must be run on macOS."
  exit 1
fi

echo "==> Installing dependencies"
npm ci

echo "==> Running frontend tests"
npm test

echo "==> Running Rust backend tests"
cargo test --manifest-path src-tauri/Cargo.toml

echo "==> Building unsigned macOS DMG"
npm run tauri:build

DMG_PATH="$(find src-tauri/target/release/bundle/dmg -maxdepth 1 -type f -name '*.dmg' | head -n 1 || true)"

if [[ -z "${DMG_PATH}" ]]; then
  echo "ERROR: No DMG artifact found in src-tauri/target/release/bundle/dmg"
  exit 1
fi

echo "==> DMG ready: ${DMG_PATH}"
