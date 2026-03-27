use keyring::Entry;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::domain::errors::{AppError, AppResult};

pub struct SecretsService {
    service_name: String,
    session_cache: Mutex<HashMap<String, String>>,
}

impl SecretsService {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            session_cache: Mutex::new(HashMap::new()),
        }
    }

    fn account_key(&self, server_url: &str, dataset_pid: &str) -> String {
        format!("{}::{}", normalize(server_url), dataset_pid.trim())
    }

    pub fn set_api_token(
        &self,
        server_url: &str,
        dataset_pid: &str,
        token: &str,
    ) -> AppResult<()> {
        let account_key = self.account_key(server_url, dataset_pid);
        if let Ok(mut cache) = self.session_cache.lock() {
            cache.insert(account_key.clone(), token.to_string());
        }

        let entry = Entry::new(
            &self.service_name,
            &account_key,
        )?;
        // Keep app usable even if keychain write is unavailable on this machine/session.
        if let Err(err) = entry.set_password(token) {
            tracing::warn!("cannot persist API token in keychain, using session fallback only: {err}");
        }
        Ok(())
    }

    pub fn get_api_token(&self, server_url: &str, dataset_pid: &str) -> AppResult<Option<String>> {
        let account_key = self.account_key(server_url, dataset_pid);

        let entry = Entry::new(
            &self.service_name,
            &account_key,
        )?;
        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => {
                if let Ok(cache) = self.session_cache.lock() {
                    return Ok(cache.get(&account_key).cloned());
                }
                Ok(None)
            }
            Err(err) => {
                if let Ok(cache) = self.session_cache.lock() {
                    if let Some(token) = cache.get(&account_key) {
                        return Ok(Some(token.clone()));
                    }
                }
                Err(AppError::Keyring(err.to_string()))
            }
        }
    }

    pub fn has_api_token(&self, server_url: &str, dataset_pid: &str) -> AppResult<bool> {
        Ok(self.get_api_token(server_url, dataset_pid)?.is_some())
    }
}

fn normalize(url: &str) -> String {
    url.trim().trim_end_matches('/').to_lowercase()
}
