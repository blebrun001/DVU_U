use dataverse_heavy_uploader_lib::services::session_store::SessionStore;
use tempfile::tempdir;

#[test]
fn rotates_session_id_when_runtime_artifacts_are_cleared() {
    let temp = tempdir().expect("temp dir");
    let db_path = temp.path().join("state.sqlite");
    let store = SessionStore::new(db_path).expect("session store init");

    let first = store.get_session_id().expect("first session id");
    store
        .clear_runtime_artifacts()
        .expect("clear runtime artifacts");
    let second = store.get_session_id().expect("second session id");

    assert_ne!(first, second);
}
