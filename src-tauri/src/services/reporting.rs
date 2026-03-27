use std::fs::{self, File};
use std::path::PathBuf;

use chrono::Utc;

use crate::domain::errors::{internal, AppResult};
use crate::domain::models::FinalReport;

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Json,
    Csv,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
        }
    }
}

pub struct ReportingService {
    reports_dir: PathBuf,
}

impl ReportingService {
    pub fn new(reports_dir: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&reports_dir)?;
        Ok(Self { reports_dir })
    }

    pub fn export(&self, report: &FinalReport, format: ExportFormat) -> AppResult<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let file_name = format!(
            "{}_{}_report.{}",
            report.session_id,
            timestamp,
            format.extension()
        );
        let output_path = self.reports_dir.join(file_name);

        match format {
            ExportFormat::Json => {
                let data = serde_json::to_string_pretty(report)?;
                fs::write(&output_path, data)?;
            }
            ExportFormat::Csv => {
                let file = File::create(&output_path)?;
                let mut writer = csv::Writer::from_writer(file);
                writer.write_record([
                    "item_id",
                    "file_name",
                    "local_path",
                    "state",
                    "bytes_uploaded",
                    "total_bytes",
                    "message",
                ])?;

                for entry in &report.entries {
                    let state = serde_json::to_string(&entry.state)?;
                    let uploaded = entry.bytes_uploaded.to_string();
                    let total = entry.total_bytes.to_string();
                    writer.write_record([
                        entry.item_id.as_str(),
                        entry.file_name.as_str(),
                        entry.local_path.as_str(),
                        state.trim_matches('"'),
                        uploaded.as_str(),
                        total.as_str(),
                        entry.message.as_deref().unwrap_or(""),
                    ])?;
                }

                writer.flush()?;
            }
        }

        if !output_path.exists() {
            return Err(internal("report export failed: output file missing"));
        }

        Ok(output_path)
    }
}
