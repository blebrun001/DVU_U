# Project Analysis

## 1) Goal

`Dataverse Uploader Universal (DVU_U)` is a desktop application focused on large Dataverse transfers.
The project prioritizes operational reliability over basic upload capability:
- local batch preparation through source scanning,
- pre-transfer analysis to prevent duplicates/conflicts,
- robust execution with retry/pause/resume/cancel,
- recovery after application interruption,
- traceability through history and exportable final reports.

## 2) High-Level Architecture

Two-layer architecture:
- Frontend: React + TypeScript (Vite), focused on UX and UI state.
- Native backend: Rust (Tauri v2), handling all critical logic.

Backend layout:
- `commands`: Tauri IPC API consumed by frontend.
- `services`: business services (scanner, analyzer, transfer engine, Dataverse client, storage, reporting, secrets).
- `domain`: shared models, errors, and state machine.

Frontend layout:
- `features`: domain UI modules (`DestinationForm`, `SourceManager`, `TransferPanel`, `HistoryPanel`).
- `store`: global orchestration (Zustand) and backend synchronization.
- `lib/api.ts`: IPC contract (Tauri command invocations).

## 3) End-to-End Business Flow

1. **Destination configuration**
- User enters server URL + dataset PID + API token.
- Connectivity/permission checks are performed by `test_destination`.
- Token is stored in OS keychain (`keyring`), never in SQLite.

2. **Source definition**
- Files/folders are added (dialog + drag & drop).
- Paths are normalized/canonicalized.
- Duplicate source paths are ignored using canonical path de-duplication.

3. **Scan**
- Recursive or flat traversal depending on source settings.
- Symlinks are ignored.
- Produces `ScanSummary` plus a `ScannedItem` list.

4. **Pre-transfer analysis**
- Fetches remote Dataverse file inventory.
- Decision rules:
  - same path + same size => `skip_existing`,
  - same path + different size => `conflict`,
  - same name + same size (different folder) => heuristic + SHA-256 escalation when remote checksum is available,
  - no match => `ready`.
- Persists a `TransferPlan`.

5. **Transfer**
- A single worker processes actionable candidates (`ready/retrying`).
- Uses direct upload when available, with automatic fallback to classic upload.
- Bounded retry policy (`MAX_ATTEMPTS = 4`) with backoff.
- Emits `transfer:snapshot` events for live UI updates.

6. **Completion and reporting**
- Final states include `completed`, `completed_with_errors`, `failed`, or `cancelled`.
- Generates a `FinalReport`.
- Exports JSON/CSV to the app data `reports` directory.

## 4) State Model

Session states:
`draft -> scanning -> analyzing -> ready -> uploading -> paused/cancelling -> completed|completed_with_errors|failed|interrupted`

Item states:
`pending_scan -> ready|ignored|error -> uploading -> uploaded|retrying|error|cancelled`

Transitions are explicitly validated in `domain/state_machine.rs`, preventing illegal state changes in backend logic.

## 5) Persistence, Security, and Resilience

Local persistence (`SessionStore`):
- SQLite `state.sqlite` in the app data directory.
- Main tables: `kv`, `sources`, `batch_items`, `history`.
- `session_id` rotates when runtime artifacts are cleared.

Security:
- API token stored in OS keychain (`SecretsService`), keyed by `(server_url, dataset_pid)`.
- Minimal Tauri capabilities (`core:default`, `dialog:default`).

Resilience:
- On startup, an unfinished `uploading/cancelling` session is converted to `interrupted`.
- Snapshot is persisted and can be manually restored through `restore_last_interrupted`.

## 6) Frontend / Backend Contract

Exposed IPC commands include:
- bootstrap/config: `load_bootstrap_state`, `save_destination`, `test_destination`
- sources/scan/analysis: `add_sources`, `remove_source`, `scan_sources`, `analyze_batch`
- transfer control: `start_transfer`, `pause_transfer`, `resume_transfer`, `cancel_transfer`
- state reads: `get_transfer_snapshot`, `get_analysis_summary`, `get_final_report`, `list_history`
- recovery/export: `restore_last_interrupted`, `export_report`

DTOs are conceptually mirrored between `src-tauri/src/domain/models.rs` and `src/lib/types.ts`.

## 7) Quality and Existing Tests

Frontend (Vitest):
- formatting helper unit tests,
- component tests for key controls and state-dependent behavior.

Backend (`cargo test`):
- Dataverse service tests (validation, file listing, direct->classic fallback),
- persistence test (session rotation),
- JSON/CSV report export test,
- state-machine tests.

## 8) Strengths

- Clear separation between UI and critical logic.
- Explicit, tested state machine.
- Strong operational reliability (retry, pause/resume, interruption safety).
- Structured local persistence with historical reporting.

## 9) Current Risks / Limitations

- Local `rg --files` output includes `node_modules` (review noise and overhead).
- Frontend snapshot polling (every 1.5s) could be optimized to reduce IPC traffic outside active transfers.
- Analysis table UI is capped at 250 rows (readability gain, but limited navigation for large batches).

## 10) Improvement Opportunities

1. Add end-to-end integration tests for full scenarios (scan -> analysis -> transfer simulation).
2. Introduce controlled upload concurrency (configurable parallelism), if compatible with target Dataverse instances.
3. Add guided automatic recovery at bootstrap when an `interrupted` session is detected.
4. Expose richer operational metrics (per-file timing, retry distribution, failure taxonomy).
