# Dataverse Heavy Uploader

Tauri desktop application for transferring large file batches to Dataverse, with:
- pre-transfer duplicate/conflict analysis,
- interruption recovery,
- automatic retries,
- exportable final reporting (JSON/CSV).

## Quick Start

Prerequisites:
- Node.js LTS + npm
- Rust toolchain
- Tauri prerequisites for your OS

Install and run:

```bash
npm install
npm run tauri:dev
```

## Useful Scripts

- `npm run dev`: Vite frontend only
- `npm run tauri:dev`: full desktop app (frontend + Rust backend)
- `npm run build`: frontend build
- `npm run tauri:build`: desktop installer build
- `npm run clean:build`: remove Rust/Tauri build artifacts
- `npm run release:macos:unsigned`: macOS unsigned DMG release flow (tests + build + DMG check)
- `npm run release:windows:unsigned`: Windows unsigned NSIS release flow (tests + build + `.exe` check)
- `npm run package:windows:unsigned`: validate Windows unsigned NSIS artifact exists
- `npm test`: frontend tests (Vitest)
- `cd src-tauri && cargo test`: Rust backend tests

By default, Rust/Tauri build artifacts are now written to `/tmp/dvu_u-target` to keep repository size small.
You can override this path by exporting `DVU_TARGET_DIR` before running scripts.

## User Workflow

1. Configure Dataverse destination (`server URL`, `dataset PID`, `API token`)
2. Add sources (files/folders)
3. Run source scan
4. Run pre-transfer analysis (local/remote comparison)
5. Start transfer (pause/resume/cancel supported)
6. Review history and export final report

## Repository Structure

- `src/`: React frontend (UI, Zustand store, IPC API)
- `src-tauri/src/commands/`: Tauri commands exposed to frontend
- `src-tauri/src/services/`: business logic (scan, analysis, transfer, Dataverse, persistence, reporting)
- `src-tauri/src/domain/`: models, errors, state machine
- `docs/`: project documentation

## Documentation

- Detailed project analysis: [`docs/PROJECT_ANALYSIS.md`](docs/PROJECT_ANALYSIS.md)
- Architecture overview: [`ARCHITECTURE.md`](ARCHITECTURE.md)
- Packaging checklist: [`docs/PACKAGING_CHECKLIST.md`](docs/PACKAGING_CHECKLIST.md)
