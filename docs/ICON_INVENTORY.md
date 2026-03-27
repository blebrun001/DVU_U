# Icon Inventory (Local Cleanup Notes)

Date: 2026-03-27

## Method

- Computed checksums for icon files under `src-tauri/icons/`.
- Identified byte-identical duplicates.
- Removed only duplicates that were clearly redundant naming variants.

## Duplicates removed

- `src-tauri/icons/ios/AppIcon-20x20@2x-1.png` (duplicate of `AppIcon-20x20@2x.png`)
- `src-tauri/icons/ios/AppIcon-29x29@2x-1.png` (duplicate of `AppIcon-29x29@2x.png`)
- `src-tauri/icons/ios/AppIcon-40x40@2x-1.png` (duplicate of `AppIcon-40x40@2x.png`)

## Kept intentionally

- Bundle-referenced icons in `src-tauri/tauri.conf.json` (`icons/icon.png`, `icons/icon.ico`)
- Other platform icon sizes and variants required by packaging targets
