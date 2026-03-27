use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tracing::warn;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::domain::errors::{bad_request, AppError, AppResult};
use crate::domain::models::{ItemState, ScanSummary, ScannedItem, SourceEntry, SourceKind};

pub struct ScanOutcome {
    pub summary: ScanSummary,
    pub items: Vec<ScannedItem>,
}

#[derive(Default)]
pub struct ScannerService;

impl ScannerService {
    pub fn new() -> Self {
        Self
    }

    pub fn scan_sources(&self, sources: &[SourceEntry]) -> AppResult<ScanOutcome> {
        if sources.is_empty() {
            return Err(AppError::NoSources);
        }

        let mut summary = ScanSummary::default();
        let mut items = Vec::new();
        let mut seen_local_paths = HashSet::new();

        for source in sources {
            let source_path = PathBuf::from(&source.path);
            if !source_path.exists() {
                summary.unreadable_count += 1;
                warn!("source missing from filesystem: {}", source.path);
                continue;
            }

            match &source.kind {
                SourceKind::File => {
                    self.push_file_item(
                        source,
                        &source_path,
                        source_path
                            .file_name()
                            .and_then(|it| it.to_str())
                            .ok_or_else(|| bad_request("invalid file name"))?
                            .to_string(),
                        &mut seen_local_paths,
                        &mut summary,
                        &mut items,
                    )?;
                }
                SourceKind::Folder => {
                    let mut walker = WalkDir::new(&source_path).follow_links(false).min_depth(1);
                    if !source.recursive {
                        walker = walker.max_depth(1);
                    }

                    for entry in walker {
                        let entry = match entry {
                            Ok(item) => item,
                            Err(_) => {
                                summary.unreadable_count += 1;
                                continue;
                            }
                        };

                        if entry.file_type().is_symlink() {
                            summary.ignored_symlink_count += 1;
                            continue;
                        }

                        if !entry.file_type().is_file() {
                            continue;
                        }

                        let relative = entry
                            .path()
                            .strip_prefix(&source_path)
                            .ok()
                            .map(normalize_relative)
                            .unwrap_or_else(|| {
                                entry
                                    .path()
                                    .file_name()
                                    .and_then(|it| it.to_str())
                                    .unwrap_or("unknown")
                                    .to_string()
                            });

                        self.push_file_item(
                            source,
                            entry.path(),
                            relative,
                            &mut seen_local_paths,
                            &mut summary,
                            &mut items,
                        )?;
                    }
                }
            }
        }

        Ok(ScanOutcome { summary, items })
    }

    fn push_file_item(
        &self,
        source: &SourceEntry,
        path: &Path,
        relative_path: String,
        seen_local_paths: &mut HashSet<String>,
        summary: &mut ScanSummary,
        items: &mut Vec<ScannedItem>,
    ) -> AppResult<()> {
        let canonical = std::fs::canonicalize(path)?;
        let canonical_str = canonical
            .to_str()
            .ok_or_else(|| bad_request("path contains non-utf8 characters"))?
            .to_string();

        if seen_local_paths.contains(&canonical_str) {
            summary.duplicate_path_count += 1;
            return Ok(());
        }
        seen_local_paths.insert(canonical_str.clone());

        let metadata = std::fs::metadata(&canonical)?;
        let modified_at = metadata
            .modified()
            .ok()
            .map(system_time_to_utc);

        let file_name = Path::new(&relative_path)
            .file_name()
            .and_then(|it| it.to_str())
            .ok_or_else(|| bad_request("invalid file name"))?
            .to_string();

        let size_bytes = metadata.len();
        summary.total_files += 1;
        summary.total_bytes += size_bytes;

        items.push(ScannedItem {
            item_id: Uuid::new_v4().to_string(),
            source_id: source.id.clone(),
            local_path: canonical_str,
            relative_path,
            file_name,
            size_bytes,
            modified_at,
            checksum_sha256: None,
            decision: None,
            state: ItemState::PendingScan,
            reason: None,
            uploaded_bytes: 0,
            attempts: 0,
            message: None,
        });

        Ok(())
    }
}

fn normalize_relative(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn system_time_to_utc(value: SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(value)
}
