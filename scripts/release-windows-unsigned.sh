#!/usr/bin/env bash

set -euo pipefail

DVU_TARGET_DIR="${DVU_TARGET_DIR:-/tmp/dvu_u-target}"
export DVU_TARGET_DIR
export CARGO_TARGET_DIR="${DVU_TARGET_DIR}"

if [[ "${OS:-}" != "Windows_NT" ]]; then
  echo "This script must be run on Windows."
  exit 1
fi

echo "==> Installing dependencies"
npm ci

echo "==> Running frontend tests"
npm test

echo "==> Running Rust backend tests"
cargo test --manifest-path src-tauri/Cargo.toml

echo "==> Building Windows NSIS installer"
npm run tauri:build -- --bundles nsis

echo "==> Validating unsigned Windows installer"
npm run package:windows:unsigned
