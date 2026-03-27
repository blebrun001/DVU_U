use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};

use tauri::{AppHandle, Manager};
use tracing_subscriber::EnvFilter;

use crate::domain::errors::{internal, AppResult};
use crate::services::analyzer::AnalyzerService;
use crate::services::bundle_service::BundleService;
use crate::services::dataverse_client::DataverseClient;
use crate::services::reporting::ReportingService;
use crate::services::scanner::ScannerService;
use crate::services::secrets::SecretsService;
use crate::services::session_store::SessionStore;
use crate::services::transfer_engine::TransferEngine;

static LOG_INIT: Once = Once::new();

pub struct AppServices {
    pub app_handle: AppHandle,
    pub preflight_cancel_requested: Arc<AtomicBool>,
    pub store: Arc<SessionStore>,
    pub secrets: Arc<SecretsService>,
    pub dataverse: Arc<DataverseClient>,
    pub bundle: Arc<BundleService>,
    pub scanner: Arc<ScannerService>,
    pub analyzer: Arc<AnalyzerService>,
    pub reporting: Arc<ReportingService>,
    pub transfer: Arc<TransferEngine>,
    pub data_dir: PathBuf,
}

impl AppServices {
    pub fn bootstrap(app: &AppHandle) -> AppResult<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|err| internal(format!("cannot locate app data directory: {err}")))?
            .join("dvu_u");

        std::fs::create_dir_all(&data_dir)?;
        init_logging(&data_dir);

        let store = Arc::new(SessionStore::new(data_dir.join("state.sqlite"))?);
        let preflight_cancel_requested = Arc::new(AtomicBool::new(false));
        let secrets = Arc::new(SecretsService::new("org.dataverse.dvuu"));
        let dataverse = Arc::new(DataverseClient::new()?);
        let bundle = Arc::new(BundleService::new(data_dir.join("temp"))?);
        let scanner = Arc::new(ScannerService::new());
        let analyzer = Arc::new(AnalyzerService::new());
        let reporting = Arc::new(ReportingService::new(data_dir.join("reports"))?);
        let transfer = Arc::new(TransferEngine::new(
            app.clone(),
            store.clone(),
            secrets.clone(),
            dataverse.clone(),
        ));

        Ok(Self {
            app_handle: app.clone(),
            preflight_cancel_requested,
            store,
            secrets,
            dataverse,
            bundle,
            scanner,
            analyzer,
            reporting,
            transfer,
            data_dir,
        })
    }

    pub fn request_preflight_cancel(&self) {
        self.preflight_cancel_requested
            .store(true, Ordering::Relaxed);
    }

    pub fn clear_preflight_cancel(&self) {
        self.preflight_cancel_requested
            .store(false, Ordering::Relaxed);
    }

    pub fn is_preflight_cancel_requested(&self) -> bool {
        self.preflight_cancel_requested.load(Ordering::Relaxed)
    }
}

fn init_logging(data_dir: &PathBuf) {
    LOG_INIT.call_once(|| {
        let logs_dir = data_dir.join("logs");
        if std::fs::create_dir_all(&logs_dir).is_err() {
            return;
        }

        let file_appender = tracing_appender::rolling::daily(logs_dir, "app.log");
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_ansi(false)
            .with_writer(file_appender)
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}
