use std::collections::HashMap;
use std::io::Read;

use sha2::{Digest, Sha256};

use crate::domain::models::{
    AnalysisDecisionKind, AnalysisItemDecision, AnalysisSummary, ScannedItem,
};
use crate::services::dataverse_client::RemoteDatasetFile;

#[derive(Default)]
pub struct AnalyzerService;

impl AnalyzerService {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(
        &self,
        scanned_items: &[ScannedItem],
        remote_files: &[RemoteDatasetFile],
    ) -> (AnalysisSummary, Vec<AnalysisItemDecision>) {
        let mut summary = AnalysisSummary::default();

        let mut remote_by_path: HashMap<String, Vec<&RemoteDatasetFile>> = HashMap::new();
        let mut remote_by_name_size: HashMap<(String, u64), Vec<&RemoteDatasetFile>> =
            HashMap::new();

        for remote in remote_files {
            remote_by_path
                .entry(normalize_relative(&remote.path_key))
                .or_default()
                .push(remote);
            remote_by_name_size
                .entry((remote.file_name.clone(), remote.size_bytes))
                .or_default()
                .push(remote);
        }

        let mut decisions = Vec::with_capacity(scanned_items.len());

        for item in scanned_items {
            summary.total_files += 1;
            summary.total_bytes += item.size_bytes;

            let normalized_relative = normalize_relative(&item.relative_path);
            let decision = if let Some(remotes) = remote_by_path.get(&normalized_relative) {
                let same_size = remotes
                    .iter()
                    .any(|remote| remote.size_bytes == item.size_bytes);
                if same_size {
                    AnalysisItemDecision {
                        item_id: item.item_id.clone(),
                        local_path: item.local_path.clone(),
                        relative_path: item.relative_path.clone(),
                        file_name: item.file_name.clone(),
                        size_bytes: item.size_bytes,
                        checksum_sha256: None,
                        decision: AnalysisDecisionKind::SkipExisting,
                        reason: Some(
                            "File already exists remotely with same path and size".to_string(),
                        ),
                    }
                } else {
                    AnalysisItemDecision {
                        item_id: item.item_id.clone(),
                        local_path: item.local_path.clone(),
                        relative_path: item.relative_path.clone(),
                        file_name: item.file_name.clone(),
                        size_bytes: item.size_bytes,
                        checksum_sha256: None,
                        decision: AnalysisDecisionKind::Conflict,
                        reason: Some(
                            "Remote file exists at same path with different size. Safe-skip policy keeps it unchanged."
                                .to_string(),
                        ),
                    }
                }
            } else if let Some(remotes) =
                remote_by_name_size.get(&(item.file_name.clone(), item.size_bytes))
            {
                if remotes.len() == 1 {
                    let mut checksum_sha256 = None;
                    let remote = remotes[0];
                    if let Some(remote_sha256) = remote_checksum_sha256(remote) {
                        let local_checksum = compute_local_sha256(&item.local_path);
                        checksum_sha256 = local_checksum.clone();
                        match local_checksum {
                            Some(local) if local.eq_ignore_ascii_case(remote_sha256) => {
                                AnalysisItemDecision {
                                    item_id: item.item_id.clone(),
                                    local_path: item.local_path.clone(),
                                    relative_path: item.relative_path.clone(),
                                    file_name: item.file_name.clone(),
                                    size_bytes: item.size_bytes,
                                    checksum_sha256,
                                    decision: AnalysisDecisionKind::SkipExisting,
                                    reason: Some(
                                        "Duplicate confirmed by SHA-256 checksum (same name + size)."
                                            .to_string(),
                                    ),
                                }
                            }
                            Some(_) => AnalysisItemDecision {
                                item_id: item.item_id.clone(),
                                local_path: item.local_path.clone(),
                                relative_path: item.relative_path.clone(),
                                file_name: item.file_name.clone(),
                                size_bytes: item.size_bytes,
                                checksum_sha256,
                                decision: AnalysisDecisionKind::Conflict,
                                reason: Some(
                                    "Checksum mismatch for same name + size candidate."
                                        .to_string(),
                                ),
                            },
                            None => AnalysisItemDecision {
                                item_id: item.item_id.clone(),
                                local_path: item.local_path.clone(),
                                relative_path: item.relative_path.clone(),
                                file_name: item.file_name.clone(),
                                size_bytes: item.size_bytes,
                                checksum_sha256: None,
                                decision: AnalysisDecisionKind::Conflict,
                                reason: Some(
                                    "Cannot compute local checksum for ambiguous candidate."
                                        .to_string(),
                                ),
                            },
                        }
                    } else {
                        AnalysisItemDecision {
                            item_id: item.item_id.clone(),
                            local_path: item.local_path.clone(),
                            relative_path: item.relative_path.clone(),
                            file_name: item.file_name.clone(),
                            size_bytes: item.size_bytes,
                            checksum_sha256,
                            decision: AnalysisDecisionKind::SkipExisting,
                            reason: Some(
                                "Likely duplicate detected by file name + size (different folder)."
                                    .to_string(),
                            ),
                        }
                    }
                } else {
                    AnalysisItemDecision {
                        item_id: item.item_id.clone(),
                        local_path: item.local_path.clone(),
                        relative_path: item.relative_path.clone(),
                        file_name: item.file_name.clone(),
                        size_bytes: item.size_bytes,
                        checksum_sha256: None,
                        decision: AnalysisDecisionKind::Conflict,
                        reason: Some(
                            "Multiple remote candidates match name + size. Manual review recommended."
                                .to_string(),
                        ),
                    }
                }
            } else {
                AnalysisItemDecision {
                    item_id: item.item_id.clone(),
                    local_path: item.local_path.clone(),
                    relative_path: item.relative_path.clone(),
                    file_name: item.file_name.clone(),
                    size_bytes: item.size_bytes,
                    checksum_sha256: None,
                    decision: AnalysisDecisionKind::Ready,
                    reason: None,
                }
            };

            match &decision.decision {
                AnalysisDecisionKind::Ready => {
                    summary.to_upload_files += 1;
                    summary.to_upload_bytes += item.size_bytes;
                }
                AnalysisDecisionKind::SkipExisting => summary.skipped_existing_files += 1,
                AnalysisDecisionKind::Conflict => summary.conflict_files += 1,
                AnalysisDecisionKind::Ignored => summary.ignored_files += 1,
                AnalysisDecisionKind::Error => summary.error_files += 1,
            }

            decisions.push(decision);
        }

        if summary.to_upload_files == 0 {
            summary
                .blocking_errors
                .push("No files are eligible for upload after analysis.".to_string());
        }

        (summary, decisions)
    }
}

fn normalize_relative(input: &str) -> String {
    input
        .replace('\\', "/")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string()
}

fn remote_checksum_sha256(remote: &RemoteDatasetFile) -> Option<&str> {
    let kind = remote.checksum_type.as_deref()?;
    if kind.eq_ignore_ascii_case("sha-256") || kind.eq_ignore_ascii_case("sha256") {
        return remote.checksum_value.as_deref();
    }
    None
}

fn compute_local_sha256(path: &str) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::models::{ItemState, ScannedItem};

    #[test]
    fn detects_existing_by_path_and_size() {
        let analyzer = AnalyzerService::new();
        let scanned = vec![ScannedItem {
            item_id: "1".to_string(),
            source_id: "s".to_string(),
            local_path: "/tmp/a.txt".to_string(),
            relative_path: "data/a.txt".to_string(),
            file_name: "a.txt".to_string(),
            size_bytes: 42,
            modified_at: Some(Utc::now()),
            checksum_sha256: None,
            decision: None,
            state: ItemState::PendingScan,
            reason: None,
            uploaded_bytes: 0,
            attempts: 0,
            message: None,
        }];

        let remote = vec![RemoteDatasetFile {
            path_key: "data/a.txt".to_string(),
            file_name: "a.txt".to_string(),
            size_bytes: 42,
            checksum_type: None,
            checksum_value: None,
        }];

        let (summary, decisions) = analyzer.analyze(&scanned, &remote);
        assert_eq!(summary.skipped_existing_files, 1);
        assert!(matches!(
            decisions.first().map(|item| &item.decision),
            Some(AnalysisDecisionKind::SkipExisting)
        ));
    }

    #[test]
    fn flags_conflict_when_same_path_but_different_size() {
        let analyzer = AnalyzerService::new();
        let scanned = vec![ScannedItem {
            item_id: "1".to_string(),
            source_id: "s".to_string(),
            local_path: "/tmp/a.txt".to_string(),
            relative_path: "data/a.txt".to_string(),
            file_name: "a.txt".to_string(),
            size_bytes: 99,
            modified_at: Some(Utc::now()),
            checksum_sha256: None,
            decision: None,
            state: ItemState::PendingScan,
            reason: None,
            uploaded_bytes: 0,
            attempts: 0,
            message: None,
        }];

        let remote = vec![RemoteDatasetFile {
            path_key: "data/a.txt".to_string(),
            file_name: "a.txt".to_string(),
            size_bytes: 42,
            checksum_type: None,
            checksum_value: None,
        }];

        let (summary, decisions) = analyzer.analyze(&scanned, &remote);
        assert_eq!(summary.conflict_files, 1);
        assert!(matches!(
            decisions.first().map(|item| &item.decision),
            Some(AnalysisDecisionKind::Conflict)
        ));
    }

    #[test]
    fn marks_ready_when_no_remote_match_exists() {
        let analyzer = AnalyzerService::new();
        let scanned = vec![ScannedItem {
            item_id: "1".to_string(),
            source_id: "s".to_string(),
            local_path: "/tmp/new.bin".to_string(),
            relative_path: "incoming/new.bin".to_string(),
            file_name: "new.bin".to_string(),
            size_bytes: 2048,
            modified_at: Some(Utc::now()),
            checksum_sha256: None,
            decision: None,
            state: ItemState::PendingScan,
            reason: None,
            uploaded_bytes: 0,
            attempts: 0,
            message: None,
        }];

        let (summary, decisions) = analyzer.analyze(&scanned, &[]);
        assert_eq!(summary.to_upload_files, 1);
        assert_eq!(summary.to_upload_bytes, 2048);
        assert!(matches!(
            decisions.first().map(|item| &item.decision),
            Some(AnalysisDecisionKind::Ready)
        ));
    }

    #[test]
    fn escalates_to_checksum_for_ambiguous_name_and_size() {
        let analyzer = AnalyzerService::new();
        let temp_path = PathBuf::from(std::env::temp_dir())
            .join(format!("dvu_checksum_{}.bin", Uuid::new_v4()));
        std::fs::write(&temp_path, b"abc").expect("write temp payload");
        let size = std::fs::metadata(&temp_path).expect("temp metadata").len();
        let expected_checksum =
            compute_local_sha256(temp_path.to_str().expect("temp path utf8")).expect("sha256");

        let scanned = vec![ScannedItem {
            item_id: "1".to_string(),
            source_id: "s".to_string(),
            local_path: temp_path.to_string_lossy().to_string(),
            relative_path: "folder/a.txt".to_string(),
            file_name: "a.txt".to_string(),
            size_bytes: size,
            modified_at: Some(Utc::now()),
            checksum_sha256: None,
            decision: None,
            state: ItemState::PendingScan,
            reason: None,
            uploaded_bytes: 0,
            attempts: 0,
            message: None,
        }];

        let remote = vec![RemoteDatasetFile {
            path_key: "other/a.txt".to_string(),
            file_name: "a.txt".to_string(),
            size_bytes: size,
            checksum_type: Some("SHA-256".to_string()),
            checksum_value: Some(expected_checksum.clone()),
        }];

        let (summary, decisions) = analyzer.analyze(&scanned, &remote);
        let decision = decisions.first().expect("decision");
        assert_eq!(summary.skipped_existing_files, 1);
        assert!(matches!(
            decision.decision,
            AnalysisDecisionKind::SkipExisting
        ));
        assert_eq!(
            decision.checksum_sha256.as_deref(),
            Some(expected_checksum.as_str())
        );

        let _ = std::fs::remove_file(temp_path);
    }
}
