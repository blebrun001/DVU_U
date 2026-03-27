use std::cmp::min;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;

use futures_util::TryStreamExt;
use reqwest::header::ACCEPT_ENCODING;
use reqwest::multipart;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use tracing::{debug, warn};
use url::form_urlencoded::byte_serialize;

use crate::domain::errors::{bad_request, AppError, AppResult};
use crate::domain::models::{
    DestinationConfigInput, DestinationConfigStored, DestinationErrorKind,
    DestinationValidationResult, ScannedItem,
};
use crate::services::dataverse_url::{
    extract_dataverse_alias, normalize_server_url, resolve_url, server_url_api_candidates,
};

pub type ProgressFn = Arc<dyn Fn(u64) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct RemoteDatasetFile {
    pub path_key: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub checksum_type: Option<String>,
    pub checksum_value: Option<String>,
}

#[derive(Debug, Clone)]
pub enum UploadModeUsed {
    Direct,
    Classic,
}

#[derive(Clone)]
pub struct DataverseClient {
    http: reqwest::Client,
}

impl DataverseClient {
    pub fn new() -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .timeout(Duration::from_secs(60 * 60 * 8))
            .build()?;
        Ok(Self { http })
    }

    pub async fn validate_destination(
        &self,
        input: &DestinationConfigInput,
    ) -> DestinationValidationResult {
        let normalized = match normalize_server_url(&input.server_url) {
            Ok(url) => url,
            Err(message) => {
                return DestinationValidationResult {
                    ok: false,
                    normalized_server_url: None,
                    dataset_title: None,
                    dataset_id: None,
                    upload_supported: None,
                    direct_upload_supported: None,
                    error_kind: Some(DestinationErrorKind::InvalidInput),
                    message: Some(message.to_string()),
                }
            }
        };

        let dataset_url = format!(
            "{}/api/datasets/:persistentId/?persistentId={}",
            normalized,
            encode_query_value(&input.dataset_pid)
        );

        let response = match self
            .http
            .get(dataset_url)
            .header("X-Dataverse-key", input.api_token.clone())
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                return DestinationValidationResult {
                    ok: false,
                    normalized_server_url: Some(normalized),
                    dataset_title: None,
                    dataset_id: None,
                    upload_supported: None,
                    direct_upload_supported: None,
                    error_kind: Some(DestinationErrorKind::Network),
                    message: Some(err.to_string()),
                }
            }
        };

        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return DestinationValidationResult {
                ok: false,
                normalized_server_url: Some(normalized),
                dataset_title: None,
                dataset_id: None,
                upload_supported: Some(false),
                direct_upload_supported: Some(false),
                error_kind: Some(DestinationErrorKind::Auth),
                message: Some(
                    "Authentication failed. Verify API token and permissions.".to_string(),
                ),
            };
        }

        if response.status() == StatusCode::NOT_FOUND {
            return DestinationValidationResult {
                ok: false,
                normalized_server_url: Some(normalized),
                dataset_title: None,
                dataset_id: None,
                upload_supported: Some(false),
                direct_upload_supported: Some(false),
                error_kind: Some(DestinationErrorKind::DatasetNotFound),
                message: Some("Dataset PID not found on target server.".to_string()),
            };
        }

        if !response.status().is_success() {
            return DestinationValidationResult {
                ok: false,
                normalized_server_url: Some(normalized),
                dataset_title: None,
                dataset_id: None,
                upload_supported: Some(false),
                direct_upload_supported: Some(false),
                error_kind: Some(DestinationErrorKind::Unknown),
                message: Some(format!(
                    "Server returned unexpected status: {}",
                    response.status()
                )),
            };
        }

        let payload: Value = match response.json().await {
            Ok(value) => value,
            Err(err) => {
                return DestinationValidationResult {
                    ok: false,
                    normalized_server_url: Some(normalized),
                    dataset_title: None,
                    dataset_id: None,
                    upload_supported: Some(false),
                    direct_upload_supported: Some(false),
                    error_kind: Some(DestinationErrorKind::Unknown),
                    message: Some(format!("Cannot parse dataset response: {err}")),
                }
            }
        };

        let dataset_id = payload
            .get("data")
            .and_then(|data| data.get("id"))
            .and_then(Value::as_i64);

        let dataset_title = extract_dataset_title(&payload);

        let direct_probe_url = format!(
            "{}/api/datasets/:persistentId/uploadurls?persistentId={}&size=1",
            normalized,
            encode_query_value(&input.dataset_pid)
        );

        let mut direct_upload_supported = false;
        let direct_probe = self
            .http
            .get(direct_probe_url)
            .header("X-Dataverse-key", input.api_token.clone())
            .send()
            .await;

        if let Ok(probe_resp) = direct_probe {
            if probe_resp.status().is_success() {
                direct_upload_supported = true;
            } else if probe_resp.status() == StatusCode::FORBIDDEN {
                return DestinationValidationResult {
                    ok: false,
                    normalized_server_url: Some(normalized),
                    dataset_title,
                    dataset_id,
                    upload_supported: Some(false),
                    direct_upload_supported: Some(false),
                    error_kind: Some(DestinationErrorKind::Permission),
                    message: Some(
                        "Token can read dataset but cannot request upload permissions.".to_string(),
                    ),
                };
            }
        }

        DestinationValidationResult {
            ok: true,
            normalized_server_url: Some(normalized),
            dataset_title,
            dataset_id,
            upload_supported: Some(true),
            direct_upload_supported: Some(direct_upload_supported),
            error_kind: None,
            message: Some("Destination validated successfully.".to_string()),
        }
    }

    pub async fn list_recent_datasets(
        &self,
        server_url: &str,
        token: &str,
        limit: usize,
    ) -> AppResult<Vec<crate::domain::models::RecentDatasetOption>> {
        let normalized = normalize_server_url(server_url)?;
        let capped_limit = min(limit.max(1), 50);
        let candidates = server_url_api_candidates(&normalized);
        let subtree = extract_dataverse_alias(&normalized);
        let mut last_404: Option<(String, String)> = None;

        for base_url in candidates {
            let mut url = format!(
                "{}/api/search?q=*&type=dataset&sort=date&order=desc&per_page={}",
                base_url, capped_limit
            );
            if let Some(alias) = subtree.as_ref() {
                url.push_str("&subtree=");
                url.push_str(&encode_query_value(alias));
            }

            let mut request = self.http.get(url);
            request = request.header(ACCEPT_ENCODING, "identity");
            if !token.trim().is_empty() {
                request = request.header("X-Dataverse-key", token.trim());
            }

            let response = request.send().await?;
            if response.status() == StatusCode::NOT_FOUND {
                let body = response.text().await.unwrap_or_default();
                last_404 = Some((base_url, body));
                continue;
            }

            if !response.status().is_success() {
                let status = response.status();
                return Err(AppError::Network(format!(
                    "Dataverse recent dataset lookup failed (HTTP {status})"
                )));
            }

            let body = response.bytes().await.map_err(|err| {
                AppError::Network(format!(
                    "Dataverse recent dataset lookup could not read response body: {err}"
                ))
            })?;

            let payload: Value = serde_json::from_slice(&body).map_err(|err| {
                AppError::Network(format!(
                    "Dataverse recent dataset lookup returned an unreadable payload: {err}"
                ))
            })?;
            let items = payload
                .get("data")
                .and_then(|data| data.get("items").or(Some(data)))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();

            let options = items
                .into_iter()
                .filter_map(|entry| {
                    let persistent_id = entry
                        .get("global_id")
                        .or_else(|| entry.get("globalId"))
                        .and_then(Value::as_str)?
                        .trim()
                        .to_string();
                    if persistent_id.is_empty() {
                        return None;
                    }

                    let title = entry
                        .get("name")
                        .or_else(|| entry.get("title"))
                        .and_then(Value::as_str)
                        .unwrap_or(&persistent_id)
                        .trim()
                        .to_string();

                    Some(crate::domain::models::RecentDatasetOption {
                        persistent_id,
                        title,
                    })
                })
                .collect();

            return Ok(options);
        }

        if let Some((attempted_base, body)) = last_404 {
            return Err(AppError::Network(format!(
                "Dataverse recent dataset lookup failed (HTTP 404): base URL {attempted_base} not found. {body}"
            )));
        }

        Err(AppError::Network(
            "Dataverse recent dataset lookup failed for all server URL variants.".to_string(),
        ))
    }

    pub async fn list_dataset_files(
        &self,
        destination: &DestinationConfigStored,
        token: &str,
    ) -> AppResult<Vec<RemoteDatasetFile>> {
        let url = format!(
            "{}/api/datasets/:persistentId/versions/:latest/files?persistentId={}",
            destination.server_url,
            encode_query_value(&destination.dataset_pid)
        );

        let response = self
            .http
            .get(url)
            .header("X-Dataverse-key", token)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Network(format!(
                "Dataverse file listing failed (HTTP {status}): {body}"
            )));
        }

        let payload: Value = response.json().await?;
        let data = payload
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let files = data
            .into_iter()
            .filter_map(|entry| {
                let file_name = entry
                    .get("label")
                    .and_then(Value::as_str)
                    .map(|it| it.to_string())?;
                let directory = entry
                    .get("directoryLabel")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let path_key = if directory.trim().is_empty() {
                    file_name.clone()
                } else {
                    format!("{}/{}", normalize_path(directory), file_name)
                };
                let size_bytes = entry
                    .get("dataFile")
                    .and_then(|df| df.get("filesize"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);

                let checksum_type = entry
                    .get("dataFile")
                    .and_then(|df| df.get("checksum"))
                    .and_then(|check| {
                        check
                            .get("type")
                            .or_else(|| check.get("@type"))
                            .and_then(Value::as_str)
                    })
                    .map(|it| it.to_string());

                let checksum_value = entry
                    .get("dataFile")
                    .and_then(|df| df.get("checksum"))
                    .and_then(|check| {
                        check
                            .get("value")
                            .or_else(|| check.get("@value"))
                            .and_then(Value::as_str)
                    })
                    .map(|it| it.to_string());

                Some(RemoteDatasetFile {
                    path_key,
                    file_name,
                    size_bytes,
                    checksum_type,
                    checksum_value,
                })
            })
            .collect();

        Ok(files)
    }

    pub async fn upload_file_auto(
        &self,
        destination: &DestinationConfigStored,
        token: &str,
        item: &ScannedItem,
        progress: ProgressFn,
    ) -> AppResult<UploadModeUsed> {
        let prefer_direct = destination.direct_upload_supported;

        if prefer_direct {
            match self
                .upload_file_direct(destination, token, item, progress.clone())
                .await
            {
                Ok(()) => return Ok(UploadModeUsed::Direct),
                Err(DirectUploadError::Unsupported(message)) => {
                    warn!(
                        "Direct upload unavailable for {}: {}",
                        item.file_name, message
                    );
                }
                Err(DirectUploadError::Failed(error)) => {
                    return Err(error);
                }
            }
        }

        self.upload_file_classic(destination, token, item, progress)
            .await?;

        Ok(UploadModeUsed::Classic)
    }

    async fn upload_file_classic(
        &self,
        destination: &DestinationConfigStored,
        token: &str,
        item: &ScannedItem,
        progress: ProgressFn,
    ) -> AppResult<()> {
        let path = Path::new(&item.local_path);

        let file = File::open(path).await?;
        let sent = Arc::new(AtomicU64::new(0));
        let sent_clone = Arc::clone(&sent);
        let callback = progress;

        let stream = ReaderStream::new(file).map_ok(move |chunk| {
            let next =
                sent_clone.fetch_add(chunk.len() as u64, Ordering::SeqCst) + chunk.len() as u64;
            callback(next);
            chunk
        });

        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .essence_str()
            .to_string();

        let part = multipart::Part::stream(reqwest::Body::wrap_stream(stream))
            .file_name(item.file_name.clone())
            .mime_str(&mime)
            .map_err(|err| bad_request(format!("invalid mime: {err}")))?;

        let json_data = build_json_data_for_classic(item)?;
        let form = multipart::Form::new()
            .text("jsonData", json_data.to_string())
            .part("file", part);

        let url = format!(
            "{}/api/datasets/:persistentId/add?persistentId={}",
            destination.server_url,
            encode_query_value(&destination.dataset_pid)
        );

        let response = self
            .http
            .post(url)
            .header("X-Dataverse-key", token)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Network(format!(
                "Classic upload failed for {} (HTTP {}): {}",
                item.file_name, status, body
            )));
        }

        debug!("Classic upload completed for {}", item.file_name);
        Ok(())
    }

    async fn upload_file_direct(
        &self,
        destination: &DestinationConfigStored,
        token: &str,
        item: &ScannedItem,
        progress: ProgressFn,
    ) -> Result<(), DirectUploadError> {
        let url = format!(
            "{}/api/datasets/:persistentId/uploadurls?persistentId={}&size={}",
            destination.server_url,
            encode_query_value(&destination.dataset_pid),
            item.size_bytes
        );

        let response = self
            .http
            .get(url)
            .header("X-Dataverse-key", token)
            .send()
            .await
            .map_err(AppError::from)
            .map_err(DirectUploadError::Failed)?;

        if response.status() == StatusCode::NOT_FOUND
            || response.status() == StatusCode::BAD_REQUEST
        {
            let body = response.text().await.unwrap_or_default();
            return Err(DirectUploadError::Unsupported(body));
        }

        if response.status() == StatusCode::FORBIDDEN {
            return Err(DirectUploadError::Failed(AppError::Network(
                "Direct upload permission denied".to_string(),
            )));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DirectUploadError::Failed(AppError::Network(format!(
                "Direct upload init failed (HTTP {}): {}",
                status, body
            ))));
        }

        let envelope: UploadUrlsEnvelope = response
            .json()
            .await
            .map_err(AppError::from)
            .map_err(DirectUploadError::Failed)?;

        if let Some(single_url) = envelope.data.url.clone() {
            self.put_file_to_url(
                single_url,
                Path::new(&item.local_path),
                item.size_bytes,
                progress.clone(),
            )
            .await
            .map_err(DirectUploadError::Failed)?;
        } else if let Some(urls) = envelope.data.urls.clone() {
            self.upload_multipart(
                destination,
                token,
                Path::new(&item.local_path),
                item.size_bytes,
                envelope.data.part_size,
                urls,
                envelope.data.complete.clone(),
                envelope.data.abort.clone(),
                progress.clone(),
            )
            .await
            .map_err(DirectUploadError::Failed)?;
        } else {
            return Err(DirectUploadError::Unsupported(
                "No upload URL returned by Dataverse".to_string(),
            ));
        }

        let checksum = compute_sha256(Path::new(&item.local_path))
            .await
            .map_err(DirectUploadError::Failed)?;

        self.register_uploaded_file(
            destination,
            token,
            item,
            &envelope.data.storage_identifier,
            &checksum,
        )
        .await
        .map_err(DirectUploadError::Failed)?;

        Ok(())
    }

    async fn put_file_to_url(
        &self,
        upload_url: String,
        path: &Path,
        size: u64,
        progress: ProgressFn,
    ) -> AppResult<()> {
        let file = File::open(path).await?;
        let sent = Arc::new(AtomicU64::new(0));
        let sent_clone = Arc::clone(&sent);
        let callback = progress;

        let stream = ReaderStream::new(file).map_ok(move |chunk| {
            let uploaded =
                sent_clone.fetch_add(chunk.len() as u64, Ordering::SeqCst) + chunk.len() as u64;
            callback(uploaded);
            chunk
        });

        let response = self
            .http
            .put(upload_url)
            .header("x-amz-tagging", "dv-state=temp")
            .header("content-length", size.to_string())
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "Direct upload failed with status {}",
                response.status()
            )));
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn upload_multipart(
        &self,
        destination: &DestinationConfigStored,
        token: &str,
        path: &Path,
        total_size: u64,
        part_size: Option<u64>,
        urls: HashMap<String, String>,
        complete: Option<String>,
        abort: Option<String>,
        progress: ProgressFn,
    ) -> AppResult<()> {
        let part_size = part_size.unwrap_or(5 * 1024 * 1024);
        let mut parts: Vec<(u64, String)> = urls
            .into_iter()
            .filter_map(|(index, url)| index.parse::<u64>().ok().map(|idx| (idx, url)))
            .collect();
        parts.sort_by_key(|(idx, _)| *idx);

        let mut etags = HashMap::new();
        let mut uploaded_so_far = 0_u64;

        for (index, part_url) in parts {
            let Some(part_offset) = index
                .checked_sub(1)
                .and_then(|value| value.checked_mul(part_size))
            else {
                continue;
            };
            let remaining = total_size.saturating_sub(part_offset);
            let length = min(part_size, remaining);
            if length == 0 {
                continue;
            }

            let e_tag = self
                .upload_part(
                    part_url,
                    path,
                    part_offset,
                    length,
                    uploaded_so_far,
                    progress.clone(),
                )
                .await?;
            uploaded_so_far = uploaded_so_far.saturating_add(length);
            etags.insert(index.to_string(), e_tag);
        }

        let complete_url = complete.ok_or_else(|| bad_request("multipart complete URL missing"))?;
        let complete_resolved = resolve_url(&destination.server_url, &complete_url);

        let response = self
            .http
            .put(complete_resolved)
            .header("X-Dataverse-key", token)
            .json(&etags)
            .send()
            .await?;

        if !response.status().is_success() {
            if let Some(abort_url) = abort {
                let abort_resolved = resolve_url(&destination.server_url, &abort_url);
                let _ = self
                    .http
                    .delete(abort_resolved)
                    .header("X-Dataverse-key", token)
                    .send()
                    .await;
            }
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Network(format!(
                "Multipart completion failed (HTTP {}): {}",
                status, body
            )));
        }

        Ok(())
    }

    async fn upload_part(
        &self,
        url: String,
        path: &Path,
        offset: u64,
        length: u64,
        uploaded_before: u64,
        progress: ProgressFn,
    ) -> AppResult<String> {
        let mut file = File::open(path).await?;
        file.seek(SeekFrom::Start(offset)).await?;
        let limited = file.take(length);

        let sent = Arc::new(AtomicU64::new(0));
        let sent_clone = Arc::clone(&sent);
        let callback = progress;

        let stream = ReaderStream::new(limited).map_ok(move |chunk| {
            let part_uploaded =
                sent_clone.fetch_add(chunk.len() as u64, Ordering::SeqCst) + chunk.len() as u64;
            callback(uploaded_before.saturating_add(part_uploaded));
            chunk
        });

        let response = self
            .http
            .put(url)
            .header("x-amz-tagging", "dv-state=temp")
            .header("content-length", length.to_string())
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "Multipart part upload failed with status {}",
                response.status()
            )));
        }

        let e_tag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();

        Ok(e_tag)
    }

    async fn register_uploaded_file(
        &self,
        destination: &DestinationConfigStored,
        token: &str,
        item: &ScannedItem,
        storage_identifier: &str,
        checksum_sha256: &str,
    ) -> AppResult<()> {
        let mut json_data = serde_json::json!({
            "fileName": item.file_name.clone(),
            "mimeType": mime_guess::from_path(&item.local_path)
                .first_or_octet_stream()
                .essence_str(),
            "storageIdentifier": storage_identifier,
            "checksum": {
                "@type": "SHA-256",
                "@value": checksum_sha256
            }
        });

        if let Some(directory) = directory_label(&item.relative_path) {
            json_data["directoryLabel"] = Value::String(directory);
        }

        let form = multipart::Form::new().text("jsonData", json_data.to_string());

        let url = format!(
            "{}/api/datasets/:persistentId/add?persistentId={}",
            destination.server_url,
            encode_query_value(&destination.dataset_pid)
        );

        let response = self
            .http
            .post(url)
            .header("X-Dataverse-key", token)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Network(format!(
                "Registering direct upload failed (HTTP {}): {}",
                status, body
            )));
        }

        Ok(())
    }
}

#[derive(Debug)]
enum DirectUploadError {
    Unsupported(String),
    Failed(AppError),
}

#[derive(Debug, Deserialize)]
struct UploadUrlsEnvelope {
    data: UploadUrlsData,
}

#[derive(Debug, Deserialize, Clone)]
struct UploadUrlsData {
    url: Option<String>,
    urls: Option<HashMap<String, String>>,
    abort: Option<String>,
    complete: Option<String>,
    #[serde(rename = "partSize")]
    part_size: Option<u64>,
    #[serde(rename = "storageIdentifier")]
    storage_identifier: String,
}

fn encode_query_value(value: &str) -> String {
    byte_serialize(value.as_bytes()).collect::<String>()
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_matches('/').to_string()
}

fn extract_dataset_title(payload: &Value) -> Option<String> {
    payload
        .get("data")
        .and_then(|data| data.get("latestVersion"))
        .and_then(|version| version.get("metadataBlocks"))
        .and_then(|blocks| blocks.get("citation"))
        .and_then(|citation| citation.get("fields"))
        .and_then(Value::as_array)
        .and_then(|fields| {
            fields.iter().find_map(|field| {
                if field.get("typeName").and_then(Value::as_str) == Some("title") {
                    field
                        .get("value")
                        .and_then(Value::as_str)
                        .map(|it| it.to_string())
                } else {
                    None
                }
            })
        })
}

fn directory_label(relative_path: &str) -> Option<String> {
    let normalized = normalize_path(relative_path);
    normalized
        .rsplit_once('/')
        .map(|(directory, _)| directory.to_string())
}

fn build_json_data_for_classic(item: &ScannedItem) -> AppResult<Value> {
    let mut payload = serde_json::json!({
        "fileName": item.file_name.clone(),
    });

    if let Some(directory) = directory_label(&item.relative_path) {
        payload["directoryLabel"] = Value::String(directory);
    }

    Ok(payload)
}

pub async fn compute_sha256(path: &Path) -> AppResult<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
