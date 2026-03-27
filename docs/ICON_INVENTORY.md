# Icon Inventory

Date: 2026-03-27

## Current source set (`src-tauri/icons/`)

- `16.png`
- `32.png`
- `64.png`
- `128.png`
- `256.png`
- `512.png`
- `1024.png`

## Derived compatibility files

- `icon.png` (copied from `512.png`)
- `icon.ico` (generated from `256.png`)

## Project references updated

- Tauri bundle config uses `icons/icon.png` and `icons/icon.ico` from the generated compatibility files.
- Frontend favicon uses `public/favicon.png` (copied from `src-tauri/icons/64.png`).
- README displays `src-tauri/icons/512.png`.
