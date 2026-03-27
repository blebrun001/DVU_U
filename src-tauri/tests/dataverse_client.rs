use std::sync::Arc;

use chrono::Utc;
use dataverse_heavy_uploader_lib::domain::models::{
    AnalysisDecisionKind, DestinationConfigInput, DestinationConfigStored, DestinationErrorKind,
    ItemState, ScannedItem,
};
use dataverse_heavy_uploader_lib::services::dataverse_client::{DataverseClient, UploadModeUsed};
use tempfile::tempdir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn validates_destination_and_detects_direct_upload_fallback() {
    let server = MockServer::start().await;

    let dataset_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": "OK",
        "data": {
            "id": 123,
            "latestVersion": {
                "metadataBlocks": {
                    "citation": {
                        "fields": [
                            { "typeName": "title", "value": "Test Dataset" }
                        ]
                    }
                }
            }
        }
    }));

    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/"))
        .and(query_param("persistentId", "doi:10.9999/FK2/TEST"))
        .and(header("X-Dataverse-key", "token"))
        .respond_with(dataset_response)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/uploadurls"))
        .and(query_param("persistentId", "doi:10.9999/FK2/TEST"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DataverseClient::new().expect("client init");
    let result = client
        .validate_destination(&DestinationConfigInput {
            server_url: server.uri(),
            dataset_pid: "doi:10.9999/FK2/TEST".to_string(),
            api_token: "token".to_string(),
        })
        .await;

    assert!(result.ok);
    assert_eq!(result.dataset_id, Some(123));
    assert_eq!(result.direct_upload_supported, Some(false));
}

#[tokio::test]
async fn lists_remote_dataset_files() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/versions/:latest/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "OK",
            "data": [
                {
                    "label": "a.txt",
                    "directoryLabel": "sub/path",
                    "dataFile": {
                        "filesize": 12,
                        "checksum": {
                            "type": "MD5",
                            "value": "abc"
                        }
                    }
                }
            ]
        })))
        .mount(&server)
        .await;

    let client = DataverseClient::new().expect("client init");
    let files = client
        .list_dataset_files(
            &DestinationConfigStored {
                server_url: server.uri(),
                dataset_pid: "doi:10.9999/FK2/TEST".to_string(),
                direct_upload_supported: false,
            },
            "token",
        )
        .await
        .expect("list files should succeed");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path_key, "sub/path/a.txt");
    assert_eq!(files[0].size_bytes, 12);
}

#[tokio::test]
async fn maps_auth_failure_during_destination_validation() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = DataverseClient::new().expect("client init");
    let result = client
        .validate_destination(&DestinationConfigInput {
            server_url: server.uri(),
            dataset_pid: "doi:10.9999/FK2/TEST".to_string(),
            api_token: "bad-token".to_string(),
        })
        .await;

    assert!(!result.ok);
    assert!(matches!(
        result.error_kind,
        Some(DestinationErrorKind::Auth)
    ));
}

#[tokio::test]
async fn maps_dataset_not_found_during_destination_validation() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = DataverseClient::new().expect("client init");
    let result = client
        .validate_destination(&DestinationConfigInput {
            server_url: server.uri(),
            dataset_pid: "doi:10.9999/FK2/MISSING".to_string(),
            api_token: "token".to_string(),
        })
        .await;

    assert!(!result.ok);
    assert!(matches!(
        result.error_kind,
        Some(DestinationErrorKind::DatasetNotFound)
    ));
}

#[tokio::test]
async fn falls_back_to_classic_when_direct_is_unavailable() {
    let server = MockServer::start().await;
    let temp = tempdir().expect("temp dir");
    let file_path = temp.path().join("payload.bin");
    std::fs::write(&file_path, vec![1_u8; 32]).expect("write payload");

    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/uploadurls"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/datasets/:persistentId/add"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "OK"
        })))
        .mount(&server)
        .await;

    let client = DataverseClient::new().expect("client init");
    let destination = DestinationConfigStored {
        server_url: server.uri(),
        dataset_pid: "doi:10.9999/FK2/TEST".to_string(),
        direct_upload_supported: true,
    };

    let item = ScannedItem {
        item_id: "item-1".to_string(),
        source_id: "source-1".to_string(),
        local_path: file_path.to_string_lossy().to_string(),
        relative_path: "payload.bin".to_string(),
        file_name: "payload.bin".to_string(),
        size_bytes: 32,
        modified_at: Some(Utc::now()),
        checksum_sha256: None,
        decision: Some(AnalysisDecisionKind::Ready),
        state: ItemState::Ready,
        reason: None,
        uploaded_bytes: 0,
        attempts: 0,
        message: None,
    };

    let mode = client
        .upload_file_auto(&destination, "token", &item, Arc::new(|_| {}))
        .await
        .expect("upload should fallback to classic");

    assert!(matches!(mode, UploadModeUsed::Classic));
}

#[tokio::test]
async fn uploads_in_direct_mode_when_single_url_is_available() {
    let server = MockServer::start().await;
    let temp = tempdir().expect("temp dir");
    let file_path = temp.path().join("direct.bin");
    std::fs::write(&file_path, vec![7_u8; 64]).expect("write payload");

    let single_url = format!("{}/upload/single", server.uri());
    Mock::given(method("GET"))
        .and(path("/api/datasets/:persistentId/uploadurls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "url": single_url,
                "storageIdentifier": "s3://bucket/direct.bin"
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/upload/single"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/datasets/:persistentId/add"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "OK"
        })))
        .mount(&server)
        .await;

    let client = DataverseClient::new().expect("client init");
    let destination = DestinationConfigStored {
        server_url: server.uri(),
        dataset_pid: "doi:10.9999/FK2/TEST".to_string(),
        direct_upload_supported: true,
    };

    let item = ScannedItem {
        item_id: "item-2".to_string(),
        source_id: "source-1".to_string(),
        local_path: file_path.to_string_lossy().to_string(),
        relative_path: "direct.bin".to_string(),
        file_name: "direct.bin".to_string(),
        size_bytes: 64,
        modified_at: Some(Utc::now()),
        checksum_sha256: None,
        decision: Some(AnalysisDecisionKind::Ready),
        state: ItemState::Ready,
        reason: None,
        uploaded_bytes: 0,
        attempts: 0,
        message: None,
    };

    let mode = client
        .upload_file_auto(&destination, "token", &item, Arc::new(|_| {}))
        .await
        .expect("direct upload should succeed");

    assert!(matches!(mode, UploadModeUsed::Direct));
}
