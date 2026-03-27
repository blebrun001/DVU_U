# Architecture Overview

## Separation of responsibilities

- `src/`: frontend workflow, forms, status tables, and progress UI.
- `src-tauri/src/commands`: IPC entry points.
- `src-tauri/src/services`: native business services.
- `src-tauri/src/domain`: shared domain models, errors, state transitions.

## Backend services

- `session_store`: SQLite persistence (session, sources, scanned items, analysis, snapshots, reports, history).
- `secrets`: API token storage via OS keychain.
- `scanner`: recursive file/folder scan, symlink ignore policy, duplicate-path handling.
- `analyzer`: local/remote comparison and transfer decisioning.
  - Uses path+size fast matching and targeted SHA-256 escalation for ambiguous `name+size` cases when remote checksum is available.
- `dataverse_client`: Dataverse API integration (validation, remote inventory, classic upload, direct upload + fallback).
- `transfer_engine`: single active transfer orchestration with retries, pause/resume/cancel, snapshot emission.
- `reporting`: final report export (JSON/CSV).

## Persistence policy

- Non-sensitive state in SQLite (`state.sqlite`) under app data directory.
- A new `session_id` is generated when runtime artifacts are cleared and when analysis is applied, so each transfer run can be tracked independently in history.
- Secrets (API token) in keychain (`keyring` crate).
- Structured logs in app data `logs/`.

## Transfer state model

Session states:
`draft -> scanning -> analyzing -> ready -> uploading -> paused/cancelling -> completed|completed_with_errors|failed|interrupted`

Item states:
`pending_scan -> ready|ignored|error -> uploading -> uploaded|retrying|error|cancelled`

## Frontend-backend contract

- IPC commands are typed and return explicit domain DTOs.
- Transfer progress is streamed via `transfer:snapshot` events.
- Frontend remains UI-focused; critical upload logic stays in Rust backend.
