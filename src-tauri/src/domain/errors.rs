use std::io;

use anyhow::anyhow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("destination not configured")]
    MissingDestination,
    #[error("token missing for destination")]
    MissingToken,
    #[error("nothing to scan: no sources configured")]
    NoSources,
    #[error("analysis not ready")]
    MissingAnalysis,
    #[error("transfer not running")]
    TransferNotRunning,
    #[error("operation cancelled by user")]
    Cancelled,
    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("request error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("keychain error: {0}")]
    Keyring(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<keyring::Error> for AppError {
    fn from(value: keyring::Error) -> Self {
        AppError::Keyring(value.to_string())
    }
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        value.to_string()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        AppError::Internal(value.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn bad_request<T: Into<String>>(message: T) -> AppError {
    AppError::InvalidInput(message.into())
}

pub fn internal<T: Into<String>>(message: T) -> AppError {
    AppError::Internal(message.into())
}

pub fn wrap_internal<E: std::fmt::Display>(err: E) -> AppError {
    AppError::Internal(anyhow!(err.to_string()).to_string())
}
