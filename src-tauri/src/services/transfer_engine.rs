use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::Utc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::domain::errors::{internal, AppError, AppResult};
use crate::domain::models::{
    FileTransferProgress, FinalReport, FinalReportEntry, ItemState, OperationResult, SessionState,
    ScannedItem, TransferSnapshot,
};
use crate::services::dataverse_client::{DataverseClient, ProgressFn};
use crate::services::retry::{is_retryable, next_backoff};
use crate::services::secrets::SecretsService;
use crate::services::session_store::SessionStore;

const MAX_ATTEMPTS: u32 = 4;

#[derive(Default)]
struct TransferControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
}

pub struct TransferEngine {
    app_handle: AppHandle,
    store: Arc<SessionStore>,
    secrets: Arc<SecretsService>,
    dataverse: Arc<DataverseClient>,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    control: Arc<Mutex<Option<Arc<TransferControl>>>>,
}

impl TransferEngine {
    pub fn new(
        app_handle: AppHandle,
        store: Arc<SessionStore>,
        secrets: Arc<SecretsService>,
        dataverse: Arc<DataverseClient>,
    ) -> Self {
        Self {
            app_handle,
            store,
            secrets,
            dataverse,
            task: Arc::new(Mutex::new(None)),
            control: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start(&self) -> AppResult<OperationResult> {
        self.cleanup_finished().await;

        let state = self.store.get_session_state()?;
        if !matches!(state, SessionState::Ready | SessionState::Paused) {
            return Err(AppError::InvalidStateTransition(format!(
                "cannot start transfer from state {state:?}"
            )));
        }

        {
            let guard = self.task.lock().await;
            if let Some(handle) = guard.as_ref() {
                if !handle.is_finished() {
                    return Err(AppError::InvalidStateTransition(
                        "transfer already running".to_string(),
                    ));
                }
            }
        }

        let candidates = actionable_candidates(self.store.list_upload_candidates()?);
        if candidates.is_empty() {
            return Err(AppError::MissingAnalysis);
        }

        self.store.set_session_state(&SessionState::Uploading)?;
        self.store.mark_started_at_if_missing()?;

        let control = Arc::new(TransferControl::default());
        {
            let mut control_guard = self.control.lock().await;
            *control_guard = Some(control.clone());
        }

        let task = {
            let app = self.app_handle.clone();
            let store = self.store.clone();
            let secrets = self.secrets.clone();
            let dataverse = self.dataverse.clone();
            tokio::spawn(async move {
                let app_for_recovery = app.clone();
                let store_for_recovery = store.clone();
                if let Err(err) =
                    run_transfer_loop(app, store, secrets, dataverse, control).await
                {
                    error!("transfer loop failed: {}", err);
                    let _ = store_for_recovery.cleanup_temp_bundle_file();
                    let _ = store_for_recovery
                        .force_set_session_state(&SessionState::Failed);
                    if let Ok(snapshot) = build_snapshot(
                        &store_for_recovery,
                        SessionState::Failed,
                        Some(format!("Transfer failed: {err}")),
                        None,
                    ) {
                        let _ = store_for_recovery.set_last_snapshot(&snapshot);
                        let _ = app_for_recovery.emit("transfer:snapshot", snapshot);
                    }
                }
            })
        };

        {
            let mut guard = self.task.lock().await;
            *guard = Some(task);
        }

        Ok(OperationResult::ok("Transfer started"))
    }

    pub async fn pause(&self) -> AppResult<OperationResult> {
        self.cleanup_finished().await;

        let control = self
            .control
            .lock()
            .await
            .clone()
            .ok_or(AppError::TransferNotRunning)?;

        control.paused.store(true, Ordering::SeqCst);
        let state = self.store.get_session_state()?;
        if matches!(state, SessionState::Uploading) {
            self.emit_snapshot(
                Some("Pause requested. Current upload step will complete first.".to_string()),
                None,
            )?;
        } else {
            self.store.force_set_session_state(&SessionState::Paused)?;
            self.emit_snapshot(Some("Transfer paused by user".to_string()), None)?;
        }
        Ok(OperationResult::ok("Pause requested"))
    }

    pub async fn resume(&self) -> AppResult<OperationResult> {
        self.cleanup_finished().await;

        if let Some(control) = self.control.lock().await.clone() {
            control.paused.store(false, Ordering::SeqCst);
            self.store.set_session_state(&SessionState::Uploading)?;
            self.emit_snapshot(Some("Transfer resumed".to_string()), None)?;
            return Ok(OperationResult::ok("Transfer resumed"));
        }

        // If task is gone but state is paused/interrupted, start a new worker.
        let state = self.store.get_session_state()?;
        if matches!(state, SessionState::Paused | SessionState::Interrupted) {
            self.store.force_set_session_state(&SessionState::Paused)?;
            return self.start().await;
        }

        Err(AppError::TransferNotRunning)
    }

    pub async fn cancel(&self) -> AppResult<OperationResult> {
        self.cleanup_finished().await;

        let control = self
            .control
            .lock()
            .await
            .clone()
            .ok_or(AppError::TransferNotRunning)?;
        control.cancelled.store(true, Ordering::SeqCst);
        control.paused.store(false, Ordering::SeqCst);

        self.store.set_session_state(&SessionState::Cancelling)?;
        self.emit_snapshot(Some("Cancellation requested".to_string()), None)?;

        Ok(OperationResult::ok("Cancellation requested"))
    }

    pub fn get_snapshot(&self) -> AppResult<Option<TransferSnapshot>> {
        self.store.get_last_snapshot()
    }

    async fn cleanup_finished(&self) {
        let finished = {
            let guard = self.task.lock().await;
            guard
                .as_ref()
                .map(|handle| handle.is_finished())
                .unwrap_or(false)
        };

        if finished {
            let mut task_guard = self.task.lock().await;
            task_guard.take();
            let mut control_guard = self.control.lock().await;
            control_guard.take();
        }
    }

    fn emit_snapshot(
        &self,
        last_message: Option<String>,
        active_file: Option<FileTransferProgress>,
    ) -> AppResult<TransferSnapshot> {
        let snapshot = build_snapshot(
            &self.store,
            self.store.get_session_state()?,
            last_message,
            active_file,
        )?;

        self.store.set_last_snapshot(&snapshot)?;
        self.app_handle
            .emit("transfer:snapshot", snapshot.clone())
            .map_err(|err| internal(format!("cannot emit transfer snapshot: {err}")))?;

        Ok(snapshot)
    }
}

async fn run_transfer_loop(
    app_handle: AppHandle,
    store: Arc<SessionStore>,
    secrets: Arc<SecretsService>,
    dataverse: Arc<DataverseClient>,
    control: Arc<TransferControl>,
) -> AppResult<()> {
    let started_clock = Instant::now();
    info!("Transfer worker started");

    loop {
        if control.cancelled.load(Ordering::SeqCst) {
            info!("Transfer cancellation flag observed");
            finalize_cancelled(&app_handle, &store, started_clock)?;
            return Ok(());
        }

        if control.paused.load(Ordering::SeqCst) {
            if !matches!(store.get_session_state()?, SessionState::Paused) {
                store.set_session_state(&SessionState::Paused)?;
            }
            let snapshot = build_snapshot(
                &store,
                SessionState::Paused,
                Some("Transfer paused".to_string()),
                None,
            )?;
            store.set_last_snapshot(&snapshot)?;
            let _ = app_handle.emit("transfer:snapshot", snapshot);
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            continue;
        }

        let destination = store.get_destination()?.ok_or(AppError::MissingDestination)?;
        let token = secrets
            .get_api_token(&destination.server_url, &destination.dataset_pid)?
            .ok_or(AppError::MissingToken)?;

        let candidates = actionable_candidates(store.list_upload_candidates()?);
        if candidates.is_empty() {
            let final_state = finalize_completed(&app_handle, &store, started_clock)?;
            info!("Transfer worker finished with state: {:?}", final_state);
            return Ok(());
        }

        for candidate in candidates {
            if control.cancelled.load(Ordering::SeqCst) {
                finalize_cancelled(&app_handle, &store, started_clock)?;
                return Ok(());
            }

            while control.paused.load(Ordering::SeqCst) {
                if !matches!(store.get_session_state()?, SessionState::Paused) {
                    store.set_session_state(&SessionState::Paused)?;
                    let snapshot = build_snapshot(
                        &store,
                        SessionState::Paused,
                        Some("Transfer paused".to_string()),
                        None,
                    )?;
                    store.set_last_snapshot(&snapshot)?;
                    let _ = app_handle.emit("transfer:snapshot", snapshot);
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                if control.cancelled.load(Ordering::SeqCst) {
                    finalize_cancelled(&app_handle, &store, started_clock)?;
                    return Ok(());
                }
            }

            let mut attempt = candidate.attempts.saturating_add(1).max(1);
            let mut uploaded_this_file = candidate.uploaded_bytes;
            let candidate_size = candidate.size_bytes;

            loop {
                let item_id = candidate.item_id.clone();
                let file_name = candidate.file_name.clone();

                if attempt == 1 {
                    if !matches!(candidate.state, ItemState::Ready) {
                        store.force_set_item_state(
                            &item_id,
                            ItemState::Retrying,
                            Some("Resuming pending file"),
                        )?;
                    }
                } else {
                    store.force_set_item_state(
                        &item_id,
                        ItemState::Retrying,
                        Some("Retry scheduled"),
                    )?;
                    let backoff = next_backoff(attempt - 1);
                    tokio::time::sleep(backoff).await;
                }

                store.update_item_progress(
                    &item_id,
                    ItemState::Uploading,
                    uploaded_this_file,
                    attempt,
                    Some("Uploading"),
                )?;

                let progress_store = store.clone();
                let progress_app = app_handle.clone();
                let progress_item_id = item_id.clone();
                let progress_file_name = file_name.clone();
                let progress_callback: ProgressFn = Arc::new(move |bytes| {
                    let _ = progress_store.update_item_progress(
                        &progress_item_id,
                        ItemState::Uploading,
                        bytes,
                        attempt,
                        Some("Uploading"),
                    );
                    let snapshot_state = progress_store
                        .get_session_state()
                        .unwrap_or(SessionState::Uploading);
                    if let Ok(snapshot) = build_snapshot(
                        &progress_store,
                        snapshot_state,
                        Some(format!("Uploading {}", progress_file_name)),
                        Some(FileTransferProgress {
                            item_id: progress_item_id.clone(),
                            file_name: progress_file_name.clone(),
                            state: ItemState::Uploading,
                            uploaded_bytes: bytes,
                            total_bytes: candidate_size,
                            attempt,
                            message: Some("Uploading".to_string()),
                        }),
                    ) {
                        let _ = progress_store.set_last_snapshot(&snapshot);
                        let _ = progress_app.emit("transfer:snapshot", snapshot);
                    }
                });

                let upload_result = dataverse
                    .upload_file_auto(&destination, &token, &candidate, progress_callback)
                    .await;

                match upload_result {
                    Ok(mode) => {
                        let mode_message = match mode {
                            crate::services::dataverse_client::UploadModeUsed::Direct => {
                                "Uploaded via direct mode"
                            }
                            crate::services::dataverse_client::UploadModeUsed::Classic => {
                                "Uploaded via classic mode"
                            }
                        };

                        store.update_item_progress(
                            &item_id,
                            ItemState::Uploaded,
                            candidate.size_bytes,
                            attempt,
                            Some(mode_message),
                        )?;

                        let snapshot = build_snapshot(
                            &store,
                            SessionState::Uploading,
                            Some(format!("Uploaded {}", file_name)),
                            Some(FileTransferProgress {
                                item_id: item_id.clone(),
                                file_name: file_name.clone(),
                                state: ItemState::Uploaded,
                                uploaded_bytes: candidate_size,
                                total_bytes: candidate_size,
                                attempt,
                                message: Some(mode_message.to_string()),
                            }),
                        )?;
                        store.set_last_snapshot(&snapshot)?;
                        let _ = app_handle.emit("transfer:snapshot", snapshot);
                        break;
                    }
                    Err(err) => {
                        let retryable = is_retryable(&err);
                        warn!(
                            "Upload failure for {} attempt {}: {} (retryable={})",
                            file_name,
                            attempt,
                            err,
                            retryable
                        );

                        if retryable && attempt < MAX_ATTEMPTS {
                            store.update_item_progress(
                                &item_id,
                                ItemState::Retrying,
                                uploaded_this_file,
                                attempt,
                                Some("Retrying after transient error"),
                            )?;

                            let snapshot = build_snapshot(
                                &store,
                                SessionState::Uploading,
                                Some(format!(
                                    "Retry {} scheduled for {}",
                                    attempt + 1,
                                    file_name
                                )),
                                Some(FileTransferProgress {
                                    item_id: item_id.clone(),
                                    file_name: file_name.clone(),
                                    state: ItemState::Retrying,
                                    uploaded_bytes: uploaded_this_file,
                                    total_bytes: candidate_size,
                                    attempt,
                                    message: Some(err.to_string()),
                                }),
                            )?;
                            store.set_last_snapshot(&snapshot)?;
                            let _ = app_handle.emit("transfer:snapshot", snapshot);

                            attempt += 1;
                            continue;
                        }

                        store.update_item_progress(
                            &item_id,
                            ItemState::Error,
                            uploaded_this_file,
                            attempt,
                            Some(&err.to_string()),
                        )?;

                        let snapshot = build_snapshot(
                            &store,
                            SessionState::Uploading,
                            Some(format!("Failed {}: {}", file_name, err)),
                            Some(FileTransferProgress {
                                item_id: item_id.clone(),
                                file_name: file_name.clone(),
                                state: ItemState::Error,
                                uploaded_bytes: uploaded_this_file,
                                total_bytes: candidate_size,
                                attempt,
                                message: Some(err.to_string()),
                            }),
                        )?;
                        store.set_last_snapshot(&snapshot)?;
                        let _ = app_handle.emit("transfer:snapshot", snapshot);

                        break;
                    }
                }
            }
        }
    }
}

fn actionable_candidates(items: Vec<ScannedItem>) -> Vec<ScannedItem> {
    items
        .into_iter()
        .filter(|item| match item.state {
            ItemState::Ready | ItemState::Uploading | ItemState::Retrying => true,
            ItemState::Error => item.attempts < MAX_ATTEMPTS,
            _ => false,
        })
        .collect()
}

fn finalize_cancelled(
    app_handle: &AppHandle,
    store: &SessionStore,
    started_clock: Instant,
) -> AppResult<()> {
    for item in store.list_upload_candidates()? {
        store.force_set_item_state(
            &item.item_id,
            ItemState::Cancelled,
            Some("Cancelled by user"),
        )?;
    }

    store.set_session_state(&SessionState::CompletedWithErrors)?;
    let report = build_report(store, started_clock, true)?;
    store.set_final_report(&report)?;
    store.cleanup_temp_bundle_file()?;

    let snapshot = build_snapshot(
        store,
        SessionState::CompletedWithErrors,
        Some("Transfer cancelled".to_string()),
        None,
    )?;
    store.set_last_snapshot(&snapshot)?;
    let _ = app_handle.emit("transfer:snapshot", snapshot);
    Ok(())
}

fn finalize_completed(
    app_handle: &AppHandle,
    store: &SessionStore,
    started_clock: Instant,
) -> AppResult<SessionState> {
    let report = build_report(store, started_clock, false)?;

    let state = if report.error_files > 0 || report.cancelled_files > 0 {
        SessionState::CompletedWithErrors
    } else if report.uploaded_files > 0 {
        SessionState::Completed
    } else {
        SessionState::Failed
    };

    store.set_session_state(&state)?;
    store.set_final_report(&report)?;
    store.cleanup_temp_bundle_file()?;

    let completion_message = match state {
        SessionState::Completed => "Transfer completed successfully".to_string(),
        SessionState::CompletedWithErrors => {
            "Transfer finished with errors. Check final report.".to_string()
        }
        SessionState::Failed => "Transfer failed. No file was uploaded.".to_string(),
        _ => "Transfer stopped".to_string(),
    };

    let snapshot = build_snapshot(store, state.clone(), Some(completion_message), None)?;
    store.set_last_snapshot(&snapshot)?;
    let _ = app_handle.emit("transfer:snapshot", snapshot);

    Ok(state)
}

fn build_report(store: &SessionStore, started_clock: Instant, cancelled: bool) -> AppResult<FinalReport> {
    let items = store.list_scanned_items()?;
    let session_id = store.get_session_id()?;
    let started_at = store.get_started_at()?;
    let finished_at = Utc::now();

    let mut uploaded_files = 0_u64;
    let mut skipped_files = 0_u64;
    let mut conflict_files = 0_u64;
    let mut error_files = 0_u64;
    let mut cancelled_files = 0_u64;
    let mut total_bytes = 0_u64;
    let mut uploaded_bytes = 0_u64;

    let mut entries = Vec::with_capacity(items.len());

    for item in items {
        total_bytes = total_bytes.saturating_add(item.size_bytes);
        uploaded_bytes = uploaded_bytes.saturating_add(item.uploaded_bytes);

        match &item.state {
            ItemState::Uploaded => uploaded_files += 1,
            ItemState::SkippedExisting | ItemState::Ignored => skipped_files += 1,
            ItemState::Conflict => conflict_files += 1,
            ItemState::Error => error_files += 1,
            ItemState::Cancelled => cancelled_files += 1,
            _ => {}
        }

        entries.push(FinalReportEntry {
            item_id: item.item_id,
            file_name: item.file_name,
            local_path: item.local_path,
            state: item.state.clone(),
            bytes_uploaded: item.uploaded_bytes,
            total_bytes: item.size_bytes,
            message: item.message,
        });
    }

    if cancelled && cancelled_files == 0 {
        cancelled_files = 1;
    }

    let duration_seconds = started_at
        .map(|started| {
            let diff = finished_at.signed_duration_since(started).num_seconds();
            if diff < 0 {
                0
            } else {
                diff as u64
            }
        })
        .unwrap_or_else(|| started_clock.elapsed().as_secs());

    Ok(FinalReport {
        session_id,
        started_at,
        finished_at: Some(finished_at),
        duration_seconds: Some(duration_seconds),
        total_files: entries.len() as u64,
        uploaded_files,
        skipped_files,
        conflict_files,
        error_files,
        cancelled_files,
        total_bytes,
        uploaded_bytes,
        entries,
    })
}

fn build_snapshot(
    store: &SessionStore,
    state: SessionState,
    last_message: Option<String>,
    active_file: Option<FileTransferProgress>,
) -> AppResult<TransferSnapshot> {
    let items = store.list_scanned_items()?;
    let started_at = store.get_started_at()?;

    let upload_scope: Vec<_> = items
        .iter()
        .filter(|item| matches!(item.decision, Some(crate::domain::models::AnalysisDecisionKind::Ready)))
        .collect();

    let total_bytes = upload_scope.iter().map(|it| it.size_bytes).sum::<u64>();
    let uploaded_bytes = upload_scope
        .iter()
        .map(|it| it.uploaded_bytes.min(it.size_bytes))
        .sum::<u64>();

    let total_files = upload_scope.len() as u64;
    let completed_files = upload_scope
        .iter()
        .filter(|it| matches!(it.state, ItemState::Uploaded))
        .count() as u64;
    let error_files = upload_scope
        .iter()
        .filter(|it| matches!(it.state, ItemState::Error))
        .count() as u64;
    let retrying_files = upload_scope
        .iter()
        .filter(|it| matches!(it.state, ItemState::Retrying))
        .count() as u64;

    let elapsed_secs = started_at
        .as_ref()
        .and_then(|started| (Utc::now() - *started).to_std().ok())
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0);

    let throughput = if elapsed_secs > 0.0 {
        uploaded_bytes as f64 / elapsed_secs
    } else {
        0.0
    };

    let eta_seconds = if throughput > 0.0 && uploaded_bytes < total_bytes {
        let remaining = total_bytes.saturating_sub(uploaded_bytes);
        Some((remaining as f64 / throughput).ceil() as u64)
    } else {
        None
    };

    Ok(TransferSnapshot {
        session_id: store.get_session_id()?,
        state,
        started_at,
        updated_at: Utc::now(),
        total_bytes,
        uploaded_bytes,
        throughput_bytes_per_sec: throughput,
        eta_seconds,
        completed_files,
        total_files,
        error_files,
        retrying_files,
        active_file,
        last_message,
    })
}
