use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tracing::info;
use uuid::Uuid;

use crate::domain::errors::{bad_request, internal, AppError, AppResult};
use crate::domain::models::{
    AnalysisDecisionKind, AnalysisItemDecision, AnalysisSummary, BootstrapState, DestinationBootstrap,
    DestinationConfigStored, FinalReport, HistoryEntry, ItemState, ScanSummary, ScannedItem,
    SessionState, SourceEntry, SourceKind, TransferPlan, TransferSnapshot,
};
use crate::domain::state_machine::{ensure_item_transition, ensure_session_transition};

const KEY_SESSION_ID: &str = "session_id";
const KEY_SESSION_STATE: &str = "session_state";
const KEY_DESTINATION: &str = "destination";
const KEY_SCAN_SUMMARY: &str = "scan_summary";
const KEY_ANALYSIS_SUMMARY: &str = "analysis_summary";
const KEY_LAST_SNAPSHOT: &str = "last_snapshot";
const KEY_FINAL_REPORT: &str = "final_report";
const KEY_STARTED_AT: &str = "started_at";
const KEY_TEMP_BUNDLE_PATH: &str = "temp_bundle_path";

pub struct SessionStore {
    db_path: PathBuf,
}

impl SessionStore {
    pub fn new(db_path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let store = Self { db_path };
        store.init_schema()?;
        store.init_defaults()?;
        store.recover_interrupted_if_needed()?;
        Ok(store)
    }

    fn connection(&self) -> AppResult<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn init_schema(&self) -> AppResult<()> {
        let conn = self.connection()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS kv (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sources (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                kind TEXT NOT NULL,
                recursive INTEGER NOT NULL,
                added_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS batch_items (
                item_id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                local_path TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_at TEXT,
                checksum_sha256 TEXT,
                decision TEXT,
                state TEXT NOT NULL,
                reason TEXT,
                uploaded_bytes INTEGER NOT NULL DEFAULT 0,
                attempts INTEGER NOT NULL DEFAULT 0,
                message TEXT,
                last_updated TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS history (
                session_id TEXT PRIMARY KEY,
                dataset_pid TEXT NOT NULL,
                server_url TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT,
                finished_at TEXT,
                total_files INTEGER NOT NULL,
                uploaded_files INTEGER NOT NULL,
                error_files INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    fn init_defaults(&self) -> AppResult<()> {
        let conn = self.connection()?;
        if self.get_kv(&conn, KEY_SESSION_ID)?.is_none() {
            self.set_kv_raw(&conn, KEY_SESSION_ID, &Uuid::new_v4().to_string())?;
        }
        if self.get_kv(&conn, KEY_SESSION_STATE)?.is_none() {
            self.set_json_kv(&conn, KEY_SESSION_STATE, &SessionState::Draft)?;
        }
        Ok(())
    }

    fn recover_interrupted_if_needed(&self) -> AppResult<()> {
        self.cleanup_orphan_temp_bundle_if_needed()?;

        let state = self.get_session_state()?;
        if matches!(state, SessionState::Uploading | SessionState::Cancelling) {
            info!("Marking previous in-flight transfer as interrupted during startup");
            self.force_set_session_state(&SessionState::Interrupted)?;
            if let Some(mut snapshot) = self.get_last_snapshot()? {
                snapshot.state = SessionState::Interrupted;
                snapshot.last_message = Some(
                    "Application closed during transfer. Review status and resume when ready.".to_string(),
                );
                snapshot.updated_at = Utc::now();
                self.set_last_snapshot(&snapshot)?;
            }
        } else if matches!(state, SessionState::Scanning | SessionState::Analyzing) {
            info!("Recovering stale pre-transfer state to draft during startup");
            self.force_set_session_state(&SessionState::Draft)?;
        }
        Ok(())
    }

    fn cleanup_orphan_temp_bundle_if_needed(&self) -> AppResult<()> {
        let Some(path) = self.get_temp_bundle_path()? else {
            return Ok(());
        };
        let has_item = self.has_scanned_item_with_local_path(&path)?;
        if has_item {
            return Ok(());
        }
        let _ = std::fs::remove_file(&path);
        self.clear_temp_bundle_path()?;
        Ok(())
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn get_session_id(&self) -> AppResult<String> {
        let conn = self.connection()?;
        self.get_kv(&conn, KEY_SESSION_ID)?
            .ok_or_else(|| internal("session id missing"))
    }

    pub fn rotate_session_id(&self) -> AppResult<String> {
        let session_id = Uuid::new_v4().to_string();
        let conn = self.connection()?;
        self.set_kv_raw(&conn, KEY_SESSION_ID, &session_id)?;
        Ok(session_id)
    }

    pub fn get_session_state(&self) -> AppResult<SessionState> {
        let conn = self.connection()?;
        self.get_json_kv::<SessionState>(&conn, KEY_SESSION_STATE)?
            .ok_or_else(|| internal("session state missing"))
    }

    pub fn set_session_state(&self, next: &SessionState) -> AppResult<()> {
        let current = self.get_session_state()?;
        ensure_session_transition(&current, next)?;
        let conn = self.connection()?;
        self.set_json_kv(&conn, KEY_SESSION_STATE, next)
    }

    pub fn force_set_session_state(&self, state: &SessionState) -> AppResult<()> {
        let conn = self.connection()?;
        self.set_json_kv(&conn, KEY_SESSION_STATE, state)
    }

    pub fn mark_started_at_if_missing(&self) -> AppResult<()> {
        let conn = self.connection()?;
        if self.get_kv(&conn, KEY_STARTED_AT)?.is_none() {
            self.set_kv_raw(&conn, KEY_STARTED_AT, &Utc::now().to_rfc3339())?;
        }
        Ok(())
    }

    pub fn clear_started_at(&self) -> AppResult<()> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM kv WHERE key = ?1", [KEY_STARTED_AT])?;
        Ok(())
    }

    pub fn get_started_at(&self) -> AppResult<Option<DateTime<Utc>>> {
        let conn = self.connection()?;
        let raw = self.get_kv(&conn, KEY_STARTED_AT)?;
        Ok(raw.and_then(|value| parse_datetime(&value)))
    }

    pub fn save_destination(&self, destination: &DestinationConfigStored) -> AppResult<()> {
        let conn = self.connection()?;
        self.set_json_kv(&conn, KEY_DESTINATION, destination)
    }

    pub fn get_destination(&self) -> AppResult<Option<DestinationConfigStored>> {
        let conn = self.connection()?;
        self.get_json_kv(&conn, KEY_DESTINATION)
    }

    pub fn add_sources(&self, paths: &[String], recursive: bool) -> AppResult<Vec<SourceEntry>> {
        if paths.is_empty() {
            return Ok(self.list_sources()?);
        }
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        let mut changed = false;

        let mut existing = HashSet::new();
        {
            let mut stmt = tx.prepare("SELECT path FROM sources")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                existing.insert(row?);
            }
        }

        for source in paths {
            let canonical = canonicalize_path(source)?;
            if existing.contains(&canonical) {
                continue;
            }
            let metadata = std::fs::metadata(&canonical).map_err(AppError::Io)?;
            let kind = if metadata.is_dir() {
                SourceKind::Folder
            } else if metadata.is_file() {
                SourceKind::File
            } else {
                continue;
            };
            let entry = SourceEntry {
                id: Uuid::new_v4().to_string(),
                path: canonical.clone(),
                kind,
                recursive,
                added_at: Utc::now(),
            };
            tx.execute(
                "INSERT INTO sources(id, path, kind, recursive, added_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entry.id,
                    entry.path,
                    enum_to_db(&entry.kind)?,
                    if entry.recursive { 1 } else { 0 },
                    entry.added_at.to_rfc3339(),
                ],
            )?;
            existing.insert(canonical);
            changed = true;
        }

        if changed {
            self.clear_runtime_artifacts_tx(&tx)?;
        }
        tx.commit()?;
        if changed {
            self.force_set_session_state(&SessionState::Draft)?;
        }
        self.list_sources()
    }

    pub fn remove_source(&self, source_id: &str) -> AppResult<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        let removed = tx.execute("DELETE FROM sources WHERE id = ?1", [source_id])?;
        if removed > 0 {
            self.clear_runtime_artifacts_tx(&tx)?;
        }
        tx.commit()?;
        if removed > 0 {
            self.force_set_session_state(&SessionState::Draft)?;
        }
        Ok(())
    }

    pub fn list_sources(&self) -> AppResult<Vec<SourceEntry>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, path, kind, recursive, added_at FROM sources ORDER BY added_at ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let entry = SourceEntry {
                id: row.get(0)?,
                path: row.get(1)?,
                kind: enum_from_db(row.get::<_, String>(2)?)?,
                recursive: row.get::<_, i64>(3)? != 0,
                added_at: parse_datetime_required(&row.get::<_, String>(4)?)?,
            };
            items.push(entry);
        }
        Ok(items)
    }

    pub fn clear_sources(&self) -> AppResult<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM sources", [])?;
        self.clear_runtime_artifacts_tx(&tx)?;
        tx.commit()?;
        self.force_set_session_state(&SessionState::Draft)
    }

    pub fn replace_scanned_items(
        &self,
        summary: &ScanSummary,
        items: &[ScannedItem],
    ) -> AppResult<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM batch_items", [])?;
        for item in items.iter().cloned() {
            tx.execute(
                "INSERT INTO batch_items(item_id, source_id, local_path, relative_path, file_name, size_bytes, modified_at, checksum_sha256, decision, state, reason, uploaded_bytes, attempts, message, last_updated)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    item.item_id,
                    item.source_id,
                    item.local_path,
                    item.relative_path,
                    item.file_name,
                    i64_from_u64(item.size_bytes)?,
                    item.modified_at.map(|it| it.to_rfc3339()),
                    item.checksum_sha256,
                    enum_to_db(&ItemState::PendingScan)?,
                    item.reason,
                    i64_from_u64(item.uploaded_bytes)?,
                    i64::from(item.attempts),
                    item.message,
                    Utc::now().to_rfc3339(),
                ],
            )?;
        }
        self.set_json_kv(&tx, KEY_SCAN_SUMMARY, summary)?;
        tx.execute("DELETE FROM kv WHERE key = ?1", [KEY_ANALYSIS_SUMMARY])?;
        tx.execute("DELETE FROM kv WHERE key = ?1", [KEY_LAST_SNAPSHOT])?;
        tx.execute("DELETE FROM kv WHERE key = ?1", [KEY_FINAL_REPORT])?;
        tx.commit()?;
        Ok(())
    }

    pub fn apply_analysis(
        &self,
        summary: &AnalysisSummary,
        decisions: &[AnalysisItemDecision],
    ) -> AppResult<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        let next_session_id = Uuid::new_v4().to_string();

        for decision in decisions.iter().cloned() {
            let state = match &decision.decision {
                AnalysisDecisionKind::Ready => ItemState::Ready,
                AnalysisDecisionKind::SkipExisting => ItemState::SkippedExisting,
                AnalysisDecisionKind::Conflict => ItemState::Conflict,
                AnalysisDecisionKind::Ignored => ItemState::Ignored,
                AnalysisDecisionKind::Error => ItemState::Error,
            };
            tx.execute(
                "UPDATE batch_items SET decision = ?2, state = ?3, reason = ?4, last_updated = ?5 WHERE item_id = ?1",
                params![
                    decision.item_id,
                    enum_to_db(&decision.decision)?,
                    enum_to_db(&state)?,
                    decision.reason,
                    Utc::now().to_rfc3339(),
                ],
            )?;
        }

        self.set_json_kv(&tx, KEY_ANALYSIS_SUMMARY, summary)?;
        self.set_kv_raw(&tx, KEY_SESSION_ID, &next_session_id)?;
        tx.execute("DELETE FROM kv WHERE key = ?1", [KEY_STARTED_AT])?;
        tx.execute("DELETE FROM kv WHERE key = ?1", [KEY_LAST_SNAPSHOT])?;
        tx.execute("DELETE FROM kv WHERE key = ?1", [KEY_FINAL_REPORT])?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_scan_summary(&self) -> AppResult<Option<ScanSummary>> {
        let conn = self.connection()?;
        self.get_json_kv::<ScanSummary>(&conn, KEY_SCAN_SUMMARY)
    }

    pub fn get_analysis_summary(&self) -> AppResult<Option<AnalysisSummary>> {
        let conn = self.connection()?;
        self.get_json_kv::<AnalysisSummary>(&conn, KEY_ANALYSIS_SUMMARY)
    }

    pub fn get_transfer_plan(&self) -> AppResult<Option<TransferPlan>> {
        let summary = match self.get_analysis_summary()? {
            Some(value) => value,
            None => return Ok(None),
        };

        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT item_id, local_path, relative_path, file_name, size_bytes, checksum_sha256, decision, reason
             FROM batch_items WHERE decision IS NOT NULL ORDER BY relative_path ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let decision_raw: String = row.get(6)?;
            items.push(AnalysisItemDecision {
                item_id: row.get(0)?,
                local_path: row.get(1)?,
                relative_path: row.get(2)?,
                file_name: row.get(3)?,
                size_bytes: u64_from_i64(row.get::<_, i64>(4)?)?,
                checksum_sha256: row.get(5)?,
                decision: enum_from_db(decision_raw)?,
                reason: row.get(7)?,
            });
        }

        Ok(Some(TransferPlan {
            session_id: self.get_session_id()?,
            created_at: Utc::now(),
            summary,
            items,
        }))
    }

    pub fn list_scanned_items(&self) -> AppResult<Vec<ScannedItem>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT item_id, source_id, local_path, relative_path, file_name, size_bytes, modified_at,
                    checksum_sha256, decision, state, reason, uploaded_bytes, attempts, message
             FROM batch_items ORDER BY relative_path ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();

        while let Some(row) = rows.next()? {
            let modified_raw: Option<String> = row.get(6)?;
            let decision_raw: Option<String> = row.get(8)?;
            let state_raw: String = row.get(9)?;
            items.push(ScannedItem {
                item_id: row.get(0)?,
                source_id: row.get(1)?,
                local_path: row.get(2)?,
                relative_path: row.get(3)?,
                file_name: row.get(4)?,
                size_bytes: u64_from_i64(row.get::<_, i64>(5)?)?,
                modified_at: modified_raw.and_then(|value| parse_datetime(&value)),
                checksum_sha256: row.get(7)?,
                decision: match decision_raw {
                    Some(raw) => Some(enum_from_db(raw)?),
                    None => None,
                },
                state: enum_from_db(state_raw)?,
                reason: row.get(10)?,
                uploaded_bytes: u64_from_i64(row.get::<_, i64>(11)?)?,
                attempts: row.get::<_, i64>(12)? as u32,
                message: row.get(13)?,
            });
        }

        Ok(items)
    }

    pub fn list_upload_candidates(&self) -> AppResult<Vec<ScannedItem>> {
        let items = self.list_scanned_items()?;
        Ok(items
            .into_iter()
            .filter(|item| {
                matches!(item.decision, Some(AnalysisDecisionKind::Ready))
                    && matches!(
                        item.state,
                        ItemState::Ready | ItemState::Error | ItemState::Retrying | ItemState::Uploading
                    )
            })
            .collect())
    }

    pub fn update_item_progress(
        &self,
        item_id: &str,
        next_state: ItemState,
        uploaded_bytes: u64,
        attempts: u32,
        message: Option<&str>,
    ) -> AppResult<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;

        let current_state_raw: String = tx
            .query_row(
                "SELECT state FROM batch_items WHERE item_id = ?1",
                [item_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| bad_request(format!("item not found: {item_id}")))?;

        let current_state: ItemState = enum_from_db(current_state_raw)?;
        ensure_item_transition(&current_state, &next_state)?;

        tx.execute(
            "UPDATE batch_items
             SET state = ?2, uploaded_bytes = ?3, attempts = ?4, message = ?5, last_updated = ?6
             WHERE item_id = ?1",
            params![
                item_id,
                enum_to_db(&next_state)?,
                i64_from_u64(uploaded_bytes)?,
                i64::from(attempts),
                message,
                Utc::now().to_rfc3339(),
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn force_set_item_state(
        &self,
        item_id: &str,
        state: ItemState,
        message: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.connection()?;
        conn.execute(
            "UPDATE batch_items SET state = ?2, message = ?3, last_updated = ?4 WHERE item_id = ?1",
            params![item_id, enum_to_db(&state)?, message, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn set_last_snapshot(&self, snapshot: &TransferSnapshot) -> AppResult<()> {
        let conn = self.connection()?;
        self.set_json_kv(&conn, KEY_LAST_SNAPSHOT, snapshot)
    }

    pub fn get_last_snapshot(&self) -> AppResult<Option<TransferSnapshot>> {
        let conn = self.connection()?;
        self.get_json_kv::<TransferSnapshot>(&conn, KEY_LAST_SNAPSHOT)
    }

    pub fn set_final_report(&self, report: &FinalReport) -> AppResult<()> {
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        self.set_json_kv(&tx, KEY_FINAL_REPORT, report)?;
        let state = self
            .get_json_kv::<SessionState>(&tx, KEY_SESSION_STATE)?
            .unwrap_or(SessionState::CompletedWithErrors);

        let destination = self.get_json_kv::<DestinationConfigStored>(&tx, KEY_DESTINATION)?;
        let server_url = destination
            .as_ref()
            .map(|cfg| cfg.server_url.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let dataset_pid = destination
            .as_ref()
            .map(|cfg| cfg.dataset_pid.clone())
            .unwrap_or_else(|| "unknown".to_string());

        tx.execute(
            "INSERT OR REPLACE INTO history(session_id, dataset_pid, server_url, state, started_at, finished_at, total_files, uploaded_files, error_files, total_bytes)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                report.session_id.as_str(),
                dataset_pid,
                server_url,
                enum_to_db(&state)?,
                report.started_at.as_ref().map(|it| it.to_rfc3339()),
                report.finished_at.as_ref().map(|it| it.to_rfc3339()),
                i64_from_u64(report.total_files)?,
                i64_from_u64(report.uploaded_files)?,
                i64_from_u64(report.error_files)?,
                i64_from_u64(report.total_bytes)?,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn get_final_report(&self) -> AppResult<Option<FinalReport>> {
        let conn = self.connection()?;
        self.get_json_kv::<FinalReport>(&conn, KEY_FINAL_REPORT)
    }

    pub fn list_history(&self) -> AppResult<Vec<HistoryEntry>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT session_id, dataset_pid, server_url, state, started_at, finished_at, total_files, uploaded_files, error_files, total_bytes
             FROM history ORDER BY COALESCE(finished_at, started_at) DESC LIMIT 100",
        )?;
        let mut rows = stmt.query([])?;
        let mut list = Vec::new();

        while let Some(row) = rows.next()? {
            let started_raw: Option<String> = row.get(4)?;
            let finished_raw: Option<String> = row.get(5)?;
            list.push(HistoryEntry {
                session_id: row.get(0)?,
                dataset_pid: row.get(1)?,
                server_url: row.get(2)?,
                state: enum_from_db(row.get::<_, String>(3)?)?,
                started_at: started_raw.and_then(|it| parse_datetime(&it)),
                finished_at: finished_raw.and_then(|it| parse_datetime(&it)),
                total_files: u64_from_i64(row.get::<_, i64>(6)?)?,
                uploaded_files: u64_from_i64(row.get::<_, i64>(7)?)?,
                error_files: u64_from_i64(row.get::<_, i64>(8)?)?,
                total_bytes: u64_from_i64(row.get::<_, i64>(9)?)?,
            });
        }

        Ok(list)
    }

    pub fn get_bootstrap_state(&self, has_token: bool) -> AppResult<BootstrapState> {
        let destination = self
            .get_destination()?
            .map(|cfg| DestinationBootstrap {
                server_url: cfg.server_url,
                dataset_pid: cfg.dataset_pid,
                has_token,
            });

        Ok(BootstrapState {
            session_id: self.get_session_id()?,
            session_state: self.get_session_state()?,
            destination,
            sources: self.list_sources()?,
            scan_summary: self.get_scan_summary()?,
            transfer_plan: self.get_transfer_plan()?,
            last_snapshot: self.get_last_snapshot()?,
            final_report: self.get_final_report()?,
        })
    }

    pub fn clear_runtime_artifacts(&self) -> AppResult<()> {
        self.cleanup_temp_bundle_file()?;
        let mut conn = self.connection()?;
        let tx = conn.transaction()?;
        self.clear_runtime_artifacts_tx(&tx)?;
        tx.commit()
            .map_err(AppError::Db)
            .map(|_| ())
    }

    pub fn restore_last_interrupted(&self) -> AppResult<()> {
        let state = self.get_session_state()?;
        if !matches!(state, SessionState::Interrupted | SessionState::Paused) {
            return Ok(());
        }
        self.force_set_session_state(&SessionState::Paused)?;
        if let Some(mut snapshot) = self.get_last_snapshot()? {
            snapshot.state = SessionState::Paused;
            snapshot.last_message = Some("Session restored. Press Resume to continue transfer.".to_string());
            snapshot.updated_at = Utc::now();
            self.set_last_snapshot(&snapshot)?;
        }
        Ok(())
    }

    fn clear_runtime_artifacts_tx(&self, tx: &Transaction<'_>) -> AppResult<()> {
        self.set_kv_raw(tx, KEY_SESSION_ID, &Uuid::new_v4().to_string())?;
        tx.execute("DELETE FROM batch_items", [])?;
        tx.execute(
            "DELETE FROM kv WHERE key IN (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                KEY_SCAN_SUMMARY,
                KEY_ANALYSIS_SUMMARY,
                KEY_LAST_SNAPSHOT,
                KEY_FINAL_REPORT,
                KEY_STARTED_AT,
                KEY_TEMP_BUNDLE_PATH,
            ],
        )?;
        Ok(())
    }

    pub fn set_temp_bundle_path(&self, path: &str) -> AppResult<()> {
        let conn = self.connection()?;
        self.set_kv_raw(&conn, KEY_TEMP_BUNDLE_PATH, path)
    }

    pub fn get_temp_bundle_path(&self) -> AppResult<Option<String>> {
        let conn = self.connection()?;
        self.get_kv(&conn, KEY_TEMP_BUNDLE_PATH)
    }

    pub fn clear_temp_bundle_path(&self) -> AppResult<()> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM kv WHERE key = ?1", [KEY_TEMP_BUNDLE_PATH])?;
        Ok(())
    }

    pub fn take_temp_bundle_path(&self) -> AppResult<Option<String>> {
        let path = self.get_temp_bundle_path()?;
        if path.is_some() {
            self.clear_temp_bundle_path()?;
        }
        Ok(path)
    }

    pub fn cleanup_temp_bundle_file(&self) -> AppResult<()> {
        if let Some(path) = self.take_temp_bundle_path()? {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    fn get_kv(&self, conn: &Connection, key: &str) -> AppResult<Option<String>> {
        let value = conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |row| row.get(0))
            .optional()?;
        Ok(value)
    }

    fn set_kv_raw(&self, conn: &Connection, key: &str, value: &str) -> AppResult<()> {
        conn.execute(
            "INSERT INTO kv(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn set_json_kv<T: Serialize>(&self, conn: &Connection, key: &str, value: &T) -> AppResult<()> {
        let payload = serde_json::to_string(value)?;
        self.set_kv_raw(conn, key, &payload)
    }

    fn get_json_kv<T: DeserializeOwned>(
        &self,
        conn: &Connection,
        key: &str,
    ) -> AppResult<Option<T>> {
        let raw = self.get_kv(conn, key)?;
        match raw {
            Some(payload) => Ok(Some(serde_json::from_str(&payload)?)),
            None => Ok(None),
        }
    }

    pub fn summarize_counts_for_snapshot(&self) -> AppResult<(u64, u64, u64, u64, u64)> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT
                SUM(size_bytes) as total_bytes,
                SUM(uploaded_bytes) as uploaded_bytes,
                SUM(CASE WHEN state = ?1 THEN 1 ELSE 0 END) as completed_files,
                SUM(CASE WHEN state = ?2 THEN 1 ELSE 0 END) as error_files,
                SUM(CASE WHEN state = ?3 THEN 1 ELSE 0 END) as retrying_files,
                COUNT(*) as total_files
             FROM batch_items",
        )?;

        let uploaded_enum = enum_to_db(&ItemState::Uploaded)?;
        let error_enum = enum_to_db(&ItemState::Error)?;
        let retry_enum = enum_to_db(&ItemState::Retrying)?;

        let values = stmt.query_row(params![uploaded_enum, error_enum, retry_enum], |row| {
            let total_bytes = row.get::<_, Option<i64>>(0)?.unwrap_or_default();
            let uploaded_bytes = row.get::<_, Option<i64>>(1)?.unwrap_or_default();
            let completed_files = row.get::<_, Option<i64>>(2)?.unwrap_or_default();
            let error_files = row.get::<_, Option<i64>>(3)?.unwrap_or_default();
            let retrying_files = row.get::<_, Option<i64>>(4)?.unwrap_or_default();
            let total_files = row.get::<_, Option<i64>>(5)?.unwrap_or_default();
            Ok((
                total_bytes,
                uploaded_bytes,
                completed_files,
                error_files,
                retrying_files,
                total_files,
            ))
        })?;

        Ok((
            u64_from_i64(values.0)?,
            u64_from_i64(values.1)?,
            u64_from_i64(values.2)?,
            u64_from_i64(values.3)?,
            u64_from_i64(values.4)?,
        ))
    }

    pub fn find_item(&self, item_id: &str) -> AppResult<Option<ScannedItem>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT item_id, source_id, local_path, relative_path, file_name, size_bytes, modified_at,
                    checksum_sha256, decision, state, reason, uploaded_bytes, attempts, message
             FROM batch_items WHERE item_id = ?1",
        )?;

        let item = stmt
            .query_row([item_id], |row| {
                let modified_raw: Option<String> = row.get(6)?;
                let decision_raw: Option<String> = row.get(8)?;
                let state_raw: String = row.get(9)?;
                Ok(ScannedItem {
                    item_id: row.get(0)?,
                    source_id: row.get(1)?,
                    local_path: row.get(2)?,
                    relative_path: row.get(3)?,
                    file_name: row.get(4)?,
                    size_bytes: u64_from_i64(row.get::<_, i64>(5)?).map_err(to_sql_error)?,
                    modified_at: modified_raw.and_then(|it| parse_datetime(&it)),
                    checksum_sha256: row.get(7)?,
                    decision: match decision_raw {
                        Some(raw) => Some(enum_from_db(raw).map_err(to_sql_error)?),
                        None => None,
                    },
                    state: enum_from_db(state_raw).map_err(to_sql_error)?,
                    reason: row.get(10)?,
                    uploaded_bytes: u64_from_i64(row.get::<_, i64>(11)?).map_err(to_sql_error)?,
                    attempts: row.get::<_, i64>(12)? as u32,
                    message: row.get(13)?,
                })
            })
            .optional()?;

        Ok(item)
    }

    fn has_scanned_item_with_local_path(&self, local_path: &str) -> AppResult<bool> {
        let conn = self.connection()?;
        let found: Option<String> = conn
            .query_row(
                "SELECT item_id FROM batch_items WHERE local_path = ?1 LIMIT 1",
                [local_path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }
}

fn canonicalize_path(path: &str) -> AppResult<String> {
    let canonical = std::fs::canonicalize(path).map_err(AppError::Io)?;
    canonical
        .to_str()
        .map(|value| value.to_string())
        .ok_or_else(|| bad_request("source path contains non-utf8 characters"))
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

fn parse_datetime_required(value: &str) -> AppResult<DateTime<Utc>> {
    parse_datetime(value).ok_or_else(|| internal(format!("invalid datetime: {value}")))
}

fn enum_to_db<T: Serialize>(value: &T) -> AppResult<String> {
    Ok(serde_json::to_string(value)?)
}

fn enum_from_db<T: DeserializeOwned>(value: String) -> AppResult<T> {
    Ok(serde_json::from_str(&value)?)
}

fn to_sql_error(error: AppError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(error))
}

fn i64_from_u64(value: u64) -> AppResult<i64> {
    i64::try_from(value).map_err(|_| internal("integer overflow while storing value"))
}

fn u64_from_i64(value: i64) -> AppResult<u64> {
    if value < 0 {
        return Err(internal(format!("negative integer found in persistence layer: {value}")));
    }
    Ok(value as u64)
}
