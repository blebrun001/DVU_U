use std::time::Duration;

use rand::Rng;

use crate::domain::errors::AppError;

pub fn is_retryable(err: &AppError) -> bool {
    match err {
        AppError::Network(message) => {
            let msg = message.to_lowercase();
            if msg.contains("timeout")
                || msg.contains("temporarily")
                || msg.contains("connection")
                || msg.contains("reset")
            {
                return true;
            }
            if let Some(code) = extract_http_code(&msg) {
                return code == 408 || code == 429 || code >= 500;
            }
            false
        }
        AppError::Reqwest(err) => {
            if err.is_timeout() || err.is_connect() {
                return true;
            }
            if let Some(status) = err.status() {
                return status.as_u16() == 408 || status.as_u16() == 429 || status.as_u16() >= 500;
            }
            false
        }
        AppError::Io(err) => {
            use std::io::ErrorKind;
            matches!(
                err.kind(),
                ErrorKind::TimedOut
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::WouldBlock
                    | ErrorKind::Interrupted
            )
        }
        _ => false,
    }
}

fn extract_http_code(msg: &str) -> Option<u16> {
    msg.split_whitespace()
        .find_map(|part| part.parse::<u16>().ok())
}

pub fn next_backoff(attempt: u32) -> Duration {
    let capped_attempt = attempt.min(6);
    let base_ms = 500_u64 * 2_u64.pow(capped_attempt);
    let jitter = rand::thread_rng().gen_range(0..=300_u64);
    Duration::from_millis(base_ms + jitter)
}

#[cfg(test)]
mod tests {
    use std::io;

    use crate::domain::errors::AppError;

    use super::*;

    #[test]
    fn retryable_status_codes() {
        assert!(extract_http_code("http 503 error").is_some());
    }

    #[test]
    fn backoff_increases() {
        let first = next_backoff(1);
        let second = next_backoff(3);
        assert!(second > first);
    }

    #[test]
    fn retryable_network_5xx() {
        let err = AppError::Network("http 503 service unavailable".to_string());
        assert!(is_retryable(&err));
    }

    #[test]
    fn non_retryable_network_4xx() {
        let err = AppError::Network("http 400 bad request".to_string());
        assert!(!is_retryable(&err));
    }

    #[test]
    fn retryable_io_timeout() {
        let err = AppError::Io(io::Error::new(io::ErrorKind::TimedOut, "timeout"));
        assert!(is_retryable(&err));
    }
}
