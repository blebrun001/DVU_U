use super::errors::{AppError, AppResult};
use super::models::{ItemState, SessionState};

pub fn ensure_session_transition(current: &SessionState, next: &SessionState) -> AppResult<()> {
    use SessionState::*;

    let allowed = matches!(
        (current, next),
        (Draft, Scanning)
            | (Draft, Analyzing)
            | (Draft, Ready)
            | (Draft, Interrupted)
            | (Scanning, Draft)
            | (Scanning, Analyzing)
            | (Analyzing, Ready)
            | (Analyzing, Draft)
            | (Ready, Uploading)
            | (Ready, Draft)
            | (Uploading, Paused)
            | (Uploading, Cancelling)
            | (Uploading, Completed)
            | (Uploading, CompletedWithErrors)
            | (Uploading, Failed)
            | (Uploading, Interrupted)
            | (Paused, Uploading)
            | (Paused, Cancelling)
            | (Paused, Interrupted)
            | (Cancelling, CompletedWithErrors)
            | (Cancelling, Failed)
            | (Interrupted, Paused)
            | (Interrupted, Ready)
            | (Failed, Ready)
            | (CompletedWithErrors, Ready)
            | (Completed, Ready)
    );

    if allowed || std::mem::discriminant(current) == std::mem::discriminant(next) {
        return Ok(());
    }

    Err(AppError::InvalidStateTransition(format!(
        "cannot transition session from {current:?} to {next:?}"
    )))
}

pub fn ensure_item_transition(current: &ItemState, next: &ItemState) -> AppResult<()> {
    use ItemState::*;

    let allowed = matches!(
        (current, next),
        (PendingScan, Ready)
            | (PendingScan, Ignored)
            | (PendingScan, Error)
            | (Ready, Uploading)
            | (Ready, SkippedExisting)
            | (Ready, Conflict)
            | (Ready, Ignored)
            | (Ready, Cancelled)
            | (Uploading, Uploaded)
            | (Uploading, Retrying)
            | (Uploading, Error)
            | (Uploading, Cancelled)
            | (Retrying, Uploading)
            | (Retrying, Error)
            | (Error, Retrying)
            | (Error, Cancelled)
    );

    if allowed || std::mem::discriminant(current) == std::mem::discriminant(next) {
        return Ok(());
    }

    Err(AppError::InvalidStateTransition(format!(
        "cannot transition item from {current:?} to {next:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_session_transition() {
        assert!(ensure_session_transition(&SessionState::Ready, &SessionState::Uploading).is_ok());
    }

    #[test]
    fn invalid_session_transition() {
        assert!(ensure_session_transition(&SessionState::Draft, &SessionState::Completed).is_err());
    }

    #[test]
    fn valid_item_transition() {
        assert!(ensure_item_transition(&ItemState::Ready, &ItemState::Uploading).is_ok());
    }

    #[test]
    fn invalid_item_transition() {
        assert!(ensure_item_transition(&ItemState::Uploaded, &ItemState::Uploading).is_err());
    }
}
