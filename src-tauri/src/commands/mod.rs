use std::time::{Duration, Instant};
use tauri::{Emitter, State};

use crate::domain::errors::{bad_request, AppError, AppResult};
use crate::domain::models::{
    AnalysisDecisionKind, AnalysisItemDecision, AnalysisProgressEvent, AnalysisSummary,
    AnalyzeBatchInput, DestinationConfigInput, DestinationConfigStored, DestinationErrorKind,
    DestinationValidationResult, FinalReport, HistoryEntry, ItemState, OperationResult,
    RecentDatasetOption, RecentDatasetsInput, ScanSummary, ScannedItem, SessionState, SourceEntry,
    SourceKind, TransferPlan, TransferSnapshot,
};
use crate::services::dataverse_url::normalize_server_url;
use crate::{AppState, SharedAppServices};

#[tauri::command]
pub async fn load_bootstrap_state(
    state: State<'_, AppState>,
) -> Result<crate::domain::models::BootstrapState, String> {
    with_services(state, |services| {
        let destination = services.store.get_destination()?;
        let has_token = if let Some(cfg) = destination.as_ref() {
            services
                .secrets
                .has_api_token(&cfg.server_url, &cfg.dataset_pid)?
        } else {
            false
        };
        services.store.get_bootstrap_state(has_token)
    })
}

#[tauri::command]
pub async fn save_destination(
    state: State<'_, AppState>,
    input: DestinationConfigInput,
) -> Result<OperationResult, String> {
    with_services(state, |services| {
        ensure_transfer_not_active(services)?;
        let normalized = normalize_server_url(&input.server_url)?;
        let dataset_pid = input.dataset_pid.trim().to_string();
        if dataset_pid.is_empty() {
            return Err(bad_request("dataset PID is required"));
        }
        let token = resolve_api_token(services, &normalized, &dataset_pid, &input.api_token)?;

        let stored = DestinationConfigStored {
            server_url: normalized.clone(),
            dataset_pid: dataset_pid.clone(),
            direct_upload_supported: false,
        };

        services.store.save_destination(&stored)?;
        services
            .secrets
            .set_api_token(&stored.server_url, &stored.dataset_pid, &token)?;
        services.store.clear_runtime_artifacts()?;
        services
            .store
            .force_set_session_state(&crate::domain::models::SessionState::Draft)?;

        Ok(OperationResult::ok("Destination saved"))
    })
}

#[tauri::command]
pub async fn test_destination(
    state: State<'_, AppState>,
    input: DestinationConfigInput,
) -> Result<DestinationValidationResult, String> {
    let services = state.0.clone();
    ensure_transfer_not_active(&services).map_err(|err| err.to_string())?;
    let normalized = match normalize_server_url(&input.server_url) {
        Ok(value) => value,
        Err(err) => {
            return Ok(invalid_destination_result(
                DestinationErrorKind::InvalidInput,
                err.to_string(),
                None,
            ));
        }
    };

    let dataset_pid = input.dataset_pid.trim().to_string();
    if dataset_pid.is_empty() {
        return Ok(invalid_destination_result(
            DestinationErrorKind::InvalidInput,
            "Dataset PID is required.".to_string(),
            Some(normalized),
        ));
    }

    let token = match resolve_api_token(&services, &normalized, &dataset_pid, &input.api_token) {
        Ok(value) => value,
        Err(AppError::MissingToken) => {
            return Ok(invalid_destination_result(
                DestinationErrorKind::InvalidInput,
                "API token is required. Enter a token or save one for this destination."
                    .to_string(),
                Some(normalized),
            ));
        }
        Err(err) => return Err(err.to_string()),
    };

    let test_input = DestinationConfigInput {
        server_url: normalized.clone(),
        dataset_pid: dataset_pid.clone(),
        api_token: token.clone(),
    };
    let result = services.dataverse.validate_destination(&test_input).await;

    if result.ok {
        let stored = DestinationConfigStored {
            server_url: result
                .normalized_server_url
                .clone()
                .unwrap_or_else(|| normalized.clone()),
            dataset_pid: dataset_pid.clone(),
            direct_upload_supported: result.direct_upload_supported.unwrap_or(false),
        };

        if let Err(err) = services.store.save_destination(&stored) {
            return Err(err.to_string());
        }

        if let Err(err) =
            services
                .secrets
                .set_api_token(&stored.server_url, &stored.dataset_pid, &token)
        {
            return Err(err.to_string());
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn list_recent_datasets(
    state: State<'_, AppState>,
    input: RecentDatasetsInput,
) -> Result<Vec<RecentDatasetOption>, String> {
    let services = state.0.clone();
    let normalized = normalize_server_url(&input.server_url).map_err(|err| err.to_string())?;
    services
        .dataverse
        .list_recent_datasets(&normalized, input.api_token.trim(), 10)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn add_sources(
    state: State<'_, AppState>,
    paths: Vec<String>,
    recursive: bool,
) -> Result<Vec<SourceEntry>, String> {
    with_services(state, |services| {
        ensure_transfer_not_active(services)?;
        services.store.add_sources(&paths, recursive)
    })
}

#[tauri::command]
pub async fn remove_source(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<OperationResult, String> {
    with_services(state, |services| {
        ensure_transfer_not_active(services)?;
        services.store.remove_source(&source_id)?;
        Ok(OperationResult::ok("Source removed"))
    })
}

#[tauri::command]
pub async fn clear_sources(state: State<'_, AppState>) -> Result<OperationResult, String> {
    with_services(state, |services| {
        ensure_transfer_not_active(services)?;
        services.store.clear_sources()?;
        Ok(OperationResult::ok("Sources cleared"))
    })
}

#[tauri::command]
pub async fn scan_sources(state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let services = state.0.clone();
    ensure_transfer_not_active(&services).map_err(|err| err.to_string())?;
    services.clear_preflight_cancel();

    services
        .store
        .force_set_session_state(&crate::domain::models::SessionState::Scanning)
        .map_err(|err| err.to_string())?;

    let result: Result<ScanSummary, String> = (|| {
        let sources = services
            .store
            .list_sources()
            .map_err(|err| err.to_string())?;
        let outcome = services
            .scanner
            .scan_sources(&sources)
            .map_err(|err| err.to_string())?;
        services
            .store
            .replace_scanned_items(&outcome.summary, &outcome.items)
            .map_err(|err| err.to_string())?;
        services
            .store
            .force_set_session_state(&crate::domain::models::SessionState::Draft)
            .map_err(|err| err.to_string())?;
        Ok(outcome.summary)
    })();

    if result.is_err() {
        let _ = services
            .store
            .force_set_session_state(&crate::domain::models::SessionState::Draft);
    }

    result
}

#[tauri::command]
pub async fn analyze_batch(
    state: State<'_, AppState>,
    input: Option<AnalyzeBatchInput>,
) -> Result<TransferPlan, String> {
    let services = state.0.clone();
    ensure_transfer_not_active(&services).map_err(|err| err.to_string())?;
    services.clear_preflight_cancel();
    let keep_structure = input.and_then(|it| it.keep_structure).unwrap_or(false);

    services
        .store
        .force_set_session_state(&crate::domain::models::SessionState::Analyzing)
        .map_err(|err| err.to_string())?;

    let result: Result<TransferPlan, String> = async {
        ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
        emit_analysis_progress(&services, 1, 6, "Validating destination access");

        let mut destination = services
            .store
            .get_destination()
            .map_err(|err| err.to_string())?
            .ok_or_else(|| AppError::MissingDestination.to_string())?;

        let token = services
            .secrets
            .get_api_token(&destination.server_url, &destination.dataset_pid)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| AppError::MissingToken.to_string())?;

        let validation = services
            .dataverse
            .validate_destination(&DestinationConfigInput {
                server_url: destination.server_url.clone(),
                dataset_pid: destination.dataset_pid.clone(),
                api_token: token.clone(),
            })
            .await;

        if !validation.ok {
            return Err(validation
                .message
                .unwrap_or_else(|| "Destination validation failed.".to_string()));
        }

        ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
        emit_analysis_progress(&services, 2, 6, "Loading selected sources");

        destination.direct_upload_supported = validation.direct_upload_supported.unwrap_or(false);
        services
            .store
            .save_destination(&destination)
            .map_err(|err| err.to_string())?;

        let sources = services
            .store
            .list_sources()
            .map_err(|err| err.to_string())?;
        let can_keep_structure = sources.len() > 1
            || sources
                .iter()
                .any(|source| matches!(source.kind, SourceKind::Folder));

        if keep_structure && can_keep_structure {
            emit_analysis_progress(&services, 3, 6, "Preparing ZIP bundle");
            services
                .store
                .cleanup_temp_bundle_file()
                .map_err(|err| err.to_string())?;

            let mut last_percent = 0_u64;
            let mut last_emit = Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now);
            let artifact = services
                .bundle
                .build_bundle_with_progress(&sources, |progress| {
                    let total_bytes = progress.total_bytes.max(1);
                    let percent = progress
                        .processed_bytes
                        .saturating_mul(100)
                        .saturating_div(total_bytes)
                        .min(100);
                    let should_emit = percent == 100
                        || percent >= last_percent.saturating_add(2)
                        || last_emit.elapsed() >= Duration::from_millis(700);
                    if !should_emit {
                        return Ok(());
                    }

                    last_percent = percent;
                    last_emit = Instant::now();
                    let current = progress.current_entry.unwrap_or_else(|| "…".to_string());
                    let message = format!(
                        "Preparing ZIP bundle: {percent}% ({}/{}) - {}",
                        progress.processed_files, progress.total_files, current
                    );
                    emit_analysis_progress(&services, 3, 6, &message);
                    ensure_preflight_not_cancelled(&services)?;
                    Ok(())
                })
                .map_err(|err| err.to_string())?;
            ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
            let synthetic_item = ScannedItem {
                item_id: uuid::Uuid::new_v4().to_string(),
                source_id: "bundle".to_string(),
                local_path: artifact.archive_path.clone(),
                relative_path: artifact.file_name.clone(),
                file_name: artifact.file_name.clone(),
                size_bytes: artifact.size_bytes,
                modified_at: Some(chrono::Utc::now()),
                checksum_sha256: None,
                decision: None,
                state: ItemState::PendingScan,
                reason: None,
                uploaded_bytes: 0,
                attempts: 0,
                message: None,
            };
            let scan_summary = ScanSummary {
                total_files: 1,
                total_bytes: artifact.size_bytes,
                unreadable_count: 0,
                ignored_symlink_count: 0,
                duplicate_path_count: 0,
            };
            services
                .store
                .replace_scanned_items(&scan_summary, &[synthetic_item.clone()])
                .map_err(|err| err.to_string())?;
            services
                .store
                .set_temp_bundle_path(&artifact.archive_path)
                .map_err(|err| err.to_string())?;

            ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
            emit_analysis_progress(&services, 4, 6, "Building transfer plan");
            let analysis_summary = AnalysisSummary {
                total_files: 1,
                total_bytes: artifact.size_bytes,
                to_upload_files: 1,
                to_upload_bytes: artifact.size_bytes,
                skipped_existing_files: 0,
                conflict_files: 0,
                ignored_files: 0,
                error_files: 0,
                blocking_errors: Vec::new(),
            };
            let decision = AnalysisItemDecision {
                item_id: synthetic_item.item_id.clone(),
                local_path: synthetic_item.local_path.clone(),
                relative_path: synthetic_item.relative_path.clone(),
                file_name: synthetic_item.file_name.clone(),
                size_bytes: synthetic_item.size_bytes,
                checksum_sha256: None,
                decision: AnalysisDecisionKind::Ready,
                reason: Some("keep structure enabled: uploading archive bundle".to_string()),
            };
            services
                .store
                .apply_analysis(&analysis_summary, &[decision])
                .map_err(|err| err.to_string())?;
            emit_analysis_progress(&services, 5, 6, "Saving analysis result");
            ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
            services
                .store
                .set_session_state(&crate::domain::models::SessionState::Ready)
                .map_err(|err| err.to_string())?;
            emit_analysis_progress(&services, 6, 6, "Analysis completed");
            return services
                .store
                .get_transfer_plan()
                .map_err(|err| err.to_string())?
                .ok_or_else(|| "Analysis result missing".to_string());
        }

        let scanned_items = services
            .store
            .list_scanned_items()
            .map_err(|err| err.to_string())?;

        if scanned_items.is_empty() {
            return Err("No files scanned. Run source scan before analysis.".to_string());
        }

        ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
        emit_analysis_progress(&services, 3, 6, "Loading dataset file index from server");
        let remote_files = services
            .dataverse
            .list_dataset_files(&destination, &token)
            .await
            .map_err(|err| err.to_string())?;

        ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
        emit_analysis_progress(&services, 4, 6, "Comparing local files with dataset");
        let (summary, decisions) = services.analyzer.analyze(&scanned_items, &remote_files);

        ensure_preflight_not_cancelled(&services).map_err(|err| err.to_string())?;
        emit_analysis_progress(&services, 5, 6, "Saving analysis result");
        services
            .store
            .apply_analysis(&summary, &decisions)
            .map_err(|err| err.to_string())?;

        services
            .store
            .set_session_state(&crate::domain::models::SessionState::Ready)
            .map_err(|err| err.to_string())?;

        emit_analysis_progress(&services, 6, 6, "Analysis completed");

        services
            .store
            .get_transfer_plan()
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "Analysis result missing".to_string())
    }
    .await;

    if result.is_err() {
        if services.is_preflight_cancel_requested() {
            emit_analysis_progress(&services, 6, 6, "Analysis cancelled by user");
        } else {
            emit_analysis_progress(&services, 6, 6, "Analysis failed");
        }
        let _ = services
            .store
            .force_set_session_state(&crate::domain::models::SessionState::Draft);
    }
    services.clear_preflight_cancel();

    result
}

#[tauri::command]
pub async fn start_transfer(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let services = state.0.clone();
    services
        .transfer
        .start()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn pause_transfer(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let services = state.0.clone();
    services
        .transfer
        .pause()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn resume_transfer(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let services = state.0.clone();
    services
        .transfer
        .resume()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn cancel_transfer(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let services = state.0.clone();
    let session_state = services
        .store
        .get_session_state()
        .map_err(|err| err.to_string())?;

    if matches!(
        session_state,
        SessionState::Scanning | SessionState::Analyzing
    ) {
        services.request_preflight_cancel();
        let _ = services.store.force_set_session_state(&SessionState::Draft);
        emit_analysis_progress(&services, 6, 6, "Cancellation requested");
        return Ok(OperationResult::ok("Cancellation requested"));
    }

    services
        .transfer
        .cancel()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn get_transfer_snapshot(
    state: State<'_, AppState>,
) -> Result<Option<TransferSnapshot>, String> {
    with_services(state, |services| services.transfer.get_snapshot())
}

#[tauri::command]
pub async fn get_analysis_summary(
    state: State<'_, AppState>,
) -> Result<Option<AnalysisSummary>, String> {
    with_services(state, |services| services.store.get_analysis_summary())
}

#[tauri::command]
pub async fn get_final_report(state: State<'_, AppState>) -> Result<Option<FinalReport>, String> {
    with_services(state, |services| services.store.get_final_report())
}

#[tauri::command]
pub async fn list_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    with_services(state, |services| services.store.list_history())
}

#[tauri::command]
pub async fn restore_last_interrupted(
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    with_services(state, |services| {
        services.store.restore_last_interrupted()?;
        Ok(OperationResult::ok("Interrupted session restored"))
    })
}

fn resolve_api_token(
    services: &SharedAppServices,
    server_url: &str,
    dataset_pid: &str,
    input_token: &str,
) -> AppResult<String> {
    let token = input_token.trim();
    if !token.is_empty() {
        return Ok(token.to_string());
    }

    services
        .secrets
        .get_api_token(server_url, dataset_pid)?
        .ok_or(AppError::MissingToken)
}

fn invalid_destination_result(
    kind: DestinationErrorKind,
    message: String,
    normalized_server_url: Option<String>,
) -> DestinationValidationResult {
    DestinationValidationResult {
        ok: false,
        normalized_server_url,
        dataset_title: None,
        dataset_id: None,
        upload_supported: Some(false),
        direct_upload_supported: Some(false),
        error_kind: Some(kind),
        message: Some(message),
    }
}

fn ensure_transfer_not_active(services: &SharedAppServices) -> AppResult<()> {
    let state = services.store.get_session_state()?;
    if matches!(
        state,
        SessionState::Uploading | SessionState::Paused | SessionState::Cancelling
    ) {
        return Err(AppError::InvalidStateTransition(
            "Operation not allowed while transfer is active or paused.".to_string(),
        ));
    }
    Ok(())
}

fn with_services<T>(
    state: State<'_, AppState>,
    f: impl FnOnce(&SharedAppServices) -> AppResult<T>,
) -> Result<T, String> {
    let services = state.0.clone();
    f(&services).map_err(|err| err.to_string())
}

fn ensure_preflight_not_cancelled(services: &SharedAppServices) -> AppResult<()> {
    if services.is_preflight_cancel_requested() {
        return Err(AppError::Cancelled);
    }
    Ok(())
}

fn emit_analysis_progress(
    services: &SharedAppServices,
    step: u32,
    total_steps: u32,
    message: &str,
) {
    let event = AnalysisProgressEvent {
        step,
        total_steps,
        message: message.to_string(),
    };
    let _ = services.app_handle.emit("analysis:progress", event);
}
