use chrono::Utc;
use dataverse_heavy_uploader_lib::domain::models::{FinalReport, FinalReportEntry, ItemState};
use dataverse_heavy_uploader_lib::services::reporting::{ExportFormat, ReportingService};
use tempfile::tempdir;

#[test]
fn exports_json_and_csv_reports() {
    let temp = tempdir().expect("temp dir");
    let service = ReportingService::new(temp.path().to_path_buf()).expect("service creation");

    let report = FinalReport {
        session_id: "session-1".to_string(),
        started_at: Some(Utc::now()),
        finished_at: Some(Utc::now()),
        duration_seconds: Some(42),
        total_files: 2,
        uploaded_files: 1,
        skipped_files: 0,
        conflict_files: 0,
        error_files: 1,
        cancelled_files: 0,
        total_bytes: 300,
        uploaded_bytes: 200,
        entries: vec![FinalReportEntry {
            item_id: "i-1".to_string(),
            file_name: "file.bin".to_string(),
            local_path: "/tmp/file.bin".to_string(),
            state: ItemState::Uploaded,
            bytes_uploaded: 200,
            total_bytes: 200,
            message: Some("ok".to_string()),
        }],
    };

    let json_path = service
        .export(&report, ExportFormat::Json)
        .expect("json export");
    let csv_path = service.export(&report, ExportFormat::Csv).expect("csv export");

    assert!(json_path.exists());
    assert!(csv_path.exists());

    let json = std::fs::read_to_string(json_path).expect("json read");
    let csv = std::fs::read_to_string(csv_path).expect("csv read");
    assert!(json.contains("\"sessionId\": \"session-1\""));
    assert!(csv.contains("item_id,file_name,local_path,state,bytes_uploaded,total_bytes,message"));
    assert!(csv.contains("i-1,file.bin,/tmp/file.bin,uploaded,200,200,ok"));
}
