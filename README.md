# Dataverse Uploader Universal (DVU_U)

<img src="src-tauri/icons/512.png" alt="Dataverse Uploader Universal (DVU_U) icon" width="140" />

A Tauri desktop application for transferring large file batches to Dataverse, with:
- pre-transfer duplicate/conflict analysis,
- interruption recovery,
- automatic retries,
- final in-app reporting.

## Choose Your Setup

You can use DVU_U in two different ways:

1. Use prebuilt releases (recommended for most users)
2. Run/build in developer mode (from source)

## Option 1: Use Prebuilt Releases (No Dev Setup)

Prebuilt desktop installers are published for both macOS and Windows.

- GitHub Releases: <https://github.com/blebrun001/DVU_U/releases>
- macOS artifact: unsigned `.dmg`
- Windows artifact: unsigned NSIS `.exe`

Important:
- Current release artifacts are unsigned and intended for internal/testing use.
- macOS may show Gatekeeper warnings on first launch.
- Windows may show SmartScreen warnings on first launch.

## Option 2: Developer Mode (Run/Build From Source)

### Development Prerequisites

- Node.js LTS + npm
- Rust toolchain
- Tauri prerequisites for your OS

### Quick Start (Developer Mode)

```bash
npm install
npm run tauri:dev
```

### Useful Scripts (Developer Mode)

- `npm run dev`: Vite frontend only
- `npm run tauri:dev`: full desktop app (frontend + Rust backend)
- `npm run build`: frontend build
- `npm run tauri:build`: desktop bundle build
- `npm run clean:build`: remove Rust/Tauri build artifacts
- `npm run package:macos:unsigned`: package unsigned macOS DMG from an existing app bundle
- `npm run release:macos:unsigned`: macOS unsigned DMG release flow (tests + build + artifact checks)
- `npm run package:windows:unsigned`: validate Windows unsigned NSIS artifact exists
- `npm run release:windows:unsigned`: Windows unsigned NSIS release flow (tests + build + artifact checks)
- `npm test`: frontend tests (Vitest)
- `cd src-tauri && cargo test`: Rust backend tests

By default, Rust/Tauri build artifacts are written to `/tmp/dvu_u-target` to keep repository size small.
You can override this path by exporting `DVU_TARGET_DIR` before running scripts.

## User Workflow

1. Configure Dataverse destination (`server URL`, `dataset PID`, `API token`)
2. Add sources (files/folders)
3. Run source scan
4. Run pre-transfer analysis (local/remote comparison)
5. Start transfer (pause/resume/cancel supported)
6. Review history and final report

## Repository Structure

- `src/`: React frontend (UI, Zustand store, IPC API)
- `src-tauri/src/commands/`: Tauri commands exposed to frontend
- `src-tauri/src/services/`: business logic (scan, analysis, transfer, Dataverse, persistence)
- `src-tauri/src/domain/`: models, errors, state machine
- `docs/`: project documentation

## Documentation

- Detailed project analysis: [`docs/PROJECT_ANALYSIS.md`](docs/PROJECT_ANALYSIS.md)
- Architecture overview: [`ARCHITECTURE.md`](ARCHITECTURE.md)
- Packaging/release checklist: [`docs/PACKAGING_CHECKLIST.md`](docs/PACKAGING_CHECKLIST.md)
