#!/usr/bin/env bash

set -euo pipefail

DATAVERSE_UPLOADER_TARGET_DIR="${DATAVERSE_UPLOADER_TARGET_DIR:-${DVU_TARGET_DIR:-/tmp/dataverse_uploader-target}}"
export DATAVERSE_UPLOADER_TARGET_DIR
export DVU_TARGET_DIR="${DVU_TARGET_DIR:-${DATAVERSE_UPLOADER_TARGET_DIR}}"
export CARGO_TARGET_DIR="${DATAVERSE_UPLOADER_TARGET_DIR}"

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

echo "==> Building macOS app bundle"
npm run tauri:build -- --bundles app

echo "==> Packaging unsigned macOS DMG"
npm run package:macos:unsigned

DMG_PATH="$(find "${DATAVERSE_UPLOADER_TARGET_DIR}/release/bundle/dmg" -maxdepth 1 -type f -name '*.dmg' | sort | tail -n 1 || true)"

if [[ -z "${DMG_PATH}" ]]; then
  echo "ERROR: No DMG artifact found in ${DATAVERSE_UPLOADER_TARGET_DIR}/release/bundle/dmg"
  exit 1
fi

echo "==> DMG ready: ${DMG_PATH}"
