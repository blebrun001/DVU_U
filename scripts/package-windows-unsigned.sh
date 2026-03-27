#!/usr/bin/env bash

set -euo pipefail

if [[ "${OS:-}" != "Windows_NT" ]]; then
  echo "This script must be run on Windows."
  exit 1
fi

EXE_DIR="src-tauri/target/release/bundle/nsis"
EXE_PATH="$(find "${EXE_DIR}" -maxdepth 1 -type f -name '*.exe' | sort | tail -n 1 || true)"

if [[ -z "${EXE_PATH}" ]]; then
  echo "ERROR: No NSIS .exe artifact found in ${EXE_DIR}"
  echo "Run: npm run tauri:build -- --bundles nsis"
  exit 1
fi

echo "==> NSIS installer ready: ${EXE_PATH}"
