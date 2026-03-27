#!/usr/bin/env bash

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must be run on macOS."
  exit 1
fi

PRODUCT_NAME="$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).productName")"
VERSION="$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json','utf8')).version")"
ARCH="$(uname -m)"

APP_SRC="src-tauri/target/release/bundle/macos/${PRODUCT_NAME}.app"
if [[ ! -d "${APP_SRC}" ]]; then
  echo "ERROR: App bundle not found at ${APP_SRC}"
  echo "Run: npm run tauri:build -- --bundles app"
  exit 1
fi

STAGE_DIR="$(mktemp -d /tmp/dvu-stage.XXXXXX)"
APP_STAGE="${STAGE_DIR}/${PRODUCT_NAME}.app"
DMG_DIR="src-tauri/target/release/bundle/dmg"
DMG_PATH="${DMG_DIR}/${PRODUCT_NAME}_${VERSION}_${ARCH}.dmg"

cleanup() {
  rm -rf "${STAGE_DIR}"
}
trap cleanup EXIT

echo "==> Preparing app bundle for unsigned distribution"
ditto --noqtn --norsrc "${APP_SRC}" "${APP_STAGE}"
xattr -cr "${APP_STAGE}"

echo "==> Applying clean ad-hoc signature"
codesign --force --deep --sign - "${APP_STAGE}"
codesign --verify --deep --strict --verbose=2 "${APP_STAGE}"

mkdir -p "${DMG_DIR}"
rm -f "${DMG_PATH}"
ln -s /Applications "${STAGE_DIR}/Applications"

echo "==> Creating DMG"
hdiutil create -volname "${PRODUCT_NAME}" -srcfolder "${STAGE_DIR}" -ov -format UDZO "${DMG_PATH}" >/dev/null
hdiutil verify "${DMG_PATH}" >/dev/null

echo "==> DMG ready: ${DMG_PATH}"
