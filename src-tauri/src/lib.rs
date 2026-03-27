use std::sync::Arc;

mod commands;
pub mod domain;
pub mod services;

use services::app_services::AppServices;
use tauri::{Manager, RunEvent};

pub type SharedAppServices = Arc<AppServices>;

pub struct AppState(pub SharedAppServices);

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let services = AppServices::bootstrap(&app.handle())?;
            app.manage(AppState(Arc::new(services)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_bootstrap_state,
            commands::save_destination,
            commands::test_destination,
            commands::list_recent_datasets,
            commands::add_sources,
            commands::remove_source,
            commands::clear_sources,
            commands::scan_sources,
            commands::analyze_batch,
            commands::start_transfer,
            commands::pause_transfer,
            commands::resume_transfer,
            commands::cancel_transfer,
            commands::get_transfer_snapshot,
            commands::get_analysis_summary,
            commands::get_final_report,
            commands::list_history,
            commands::restore_last_interrupted,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                let state = app_handle.state::<AppState>();
                let _ = state.0.store.cleanup_temp_bundle_file();
                let _ = state.0.bundle.cleanup_temp_archives();
            }
        });
}
