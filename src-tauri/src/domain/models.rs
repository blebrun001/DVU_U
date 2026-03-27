use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Draft,
    Scanning,
    Analyzing,
    Ready,
    Uploading,
    Paused,
    Cancelling,
    Completed,
    CompletedWithErrors,
    Failed,
    Interrupted,
}

impl Default for SessionState {
    fn default() -> Self {
        Self::Draft
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    PendingScan,
    Ignored,
    Ready,
    Uploading,
    Uploaded,
    SkippedExisting,
    Conflict,
    Retrying,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationConfigInput {
    pub server_url: String,
    pub dataset_pid: String,
    pub api_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationConfigStored {
    pub server_url: String,
    pub dataset_pid: String,
    pub direct_upload_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationErrorKind {
    Network,
    Auth,
    DatasetNotFound,
    Permission,
    InvalidInput,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationValidationResult {
    pub ok: bool,
    pub normalized_server_url: Option<String>,
    pub dataset_title: Option<String>,
    pub dataset_id: Option<i64>,
    pub upload_supported: Option<bool>,
    pub direct_upload_supported: Option<bool>,
    pub error_kind: Option<DestinationErrorKind>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentDatasetsInput {
    pub server_url: String,
    pub api_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentDatasetOption {
    pub persistent_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeBatchInput {
    pub keep_structure: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgressEvent {
    pub step: u32,
    pub total_steps: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceEntry {
    pub id: String,
    pub path: String,
    pub kind: SourceKind,
    pub recursive: bool,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub total_files: u64,
    pub total_bytes: u64,
    pub unreadable_count: u64,
    pub ignored_symlink_count: u64,
    pub duplicate_path_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedItem {
    pub item_id: String,
    pub source_id: String,
    pub local_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub checksum_sha256: Option<String>,
    pub decision: Option<AnalysisDecisionKind>,
    pub state: ItemState,
    pub reason: Option<String>,
    pub uploaded_bytes: u64,
    pub attempts: u32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisDecisionKind {
    Ready,
    SkipExisting,
    Conflict,
    Ignored,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisItemDecision {
    pub item_id: String,
    pub local_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub checksum_sha256: Option<String>,
    pub decision: AnalysisDecisionKind,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSummary {
    pub total_files: u64,
    pub total_bytes: u64,
    pub to_upload_files: u64,
    pub to_upload_bytes: u64,
    pub skipped_existing_files: u64,
    pub conflict_files: u64,
    pub ignored_files: u64,
    pub error_files: u64,
    pub blocking_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPlan {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub summary: AnalysisSummary,
    pub items: Vec<AnalysisItemDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTransferProgress {
    pub item_id: String,
    pub file_name: String,
    pub state: ItemState,
    pub uploaded_bytes: u64,
    pub total_bytes: u64,
    pub attempt: u32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSnapshot {
    pub session_id: String,
    pub state: SessionState,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub throughput_bytes_per_sec: f64,
    pub eta_seconds: Option<u64>,
    pub completed_files: u64,
    pub total_files: u64,
    pub error_files: u64,
    pub retrying_files: u64,
    pub active_file: Option<FileTransferProgress>,
    pub last_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalReportEntry {
    pub item_id: String,
    pub file_name: String,
    pub local_path: String,
    pub state: ItemState,
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalReport {
    pub session_id: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<u64>,
    pub total_files: u64,
    pub uploaded_files: u64,
    pub skipped_files: u64,
    pub conflict_files: u64,
    pub error_files: u64,
    pub cancelled_files: u64,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub entries: Vec<FinalReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub session_id: String,
    pub dataset_pid: String,
    pub server_url: String,
    pub state: SessionState,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub total_files: u64,
    pub uploaded_files: u64,
    pub error_files: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub session_id: String,
    pub session_state: SessionState,
    pub destination: Option<DestinationBootstrap>,
    pub sources: Vec<SourceEntry>,
    pub scan_summary: Option<ScanSummary>,
    pub transfer_plan: Option<TransferPlan>,
    pub last_snapshot: Option<TransferSnapshot>,
    pub final_report: Option<FinalReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationBootstrap {
    pub server_url: String,
    pub dataset_pid: String,
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub ok: bool,
    pub message: Option<String>,
}

impl OperationResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: Some(message.into()),
        }
    }

    pub fn simple_ok() -> Self {
        Self {
            ok: true,
            message: None,
        }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: Some(message.into()),
        }
    }
}
