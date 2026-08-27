use std::sync::{Arc, RwLock};

use anyhow::Result;
use gpui::Global;
use serde::{Deserialize, Serialize};

use crate::services::api::{ApiError, UserIdentity};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthSession {
    pub server_url: String,
    pub access_token: String,
    pub access_token_expires_at: i64,
    pub refresh_token: String,
    pub refresh_token_expires_at: i64,
    pub user: UserIdentity,
}

impl AuthSession {
    pub fn access_token_is_valid_at(&self, _unix_timestamp: i64) -> bool {
        true
    }
    pub fn access_token_is_fresh_at(&self, _unix_timestamp: i64) -> bool {
        true
    }
    pub fn refresh_token_is_valid_at(&self, _unix_timestamp: i64) -> bool {
        true
    }

    pub fn avatar_initial(&self) -> String {
        self.user
            .username
            .chars()
            .next()
            .unwrap_or('L')
            .to_uppercase()
            .collect()
    }
}

#[derive(Clone)]
pub struct SessionStore {
    current: Arc<RwLock<Option<AuthSession>>>,
}

impl Global for SessionStore {}

impl SessionStore {
    pub fn load_default() -> Result<Self> {
        Ok(Self {
            current: Arc::new(RwLock::new(Some(local_session()))),
        })
    }

    pub fn current(&self) -> Option<AuthSession> {
        self.current.read().ok()?.clone()
    }

    pub fn replace(&self, session: AuthSession) -> Result<()> {
        *self
            .current
            .write()
            .map_err(|_| anyhow::anyhow!("local session lock is poisoned"))? = Some(session);
        Ok(())
    }

    pub async fn valid_access_token(&self) -> Result<String, ApiError> {
        self.current()
            .map(|session| session.access_token)
            .ok_or(ApiError::AuthenticationRequired)
    }

    pub async fn refresh_after_rejection(
        &self,
        _rejected_access_token: &str,
    ) -> Result<String, ApiError> {
        self.valid_access_token().await
    }

    pub async fn logout(&self) -> Result<(), ApiError> {
        self.replace(local_session())
            .map_err(|error| ApiError::Session(error.to_string()))
    }

    pub fn clear(&self) -> Result<()> {
        self.replace(local_session())
    }
}

fn local_session() -> AuthSession {
    AuthSession {
        server_url: "local".to_owned(),
        access_token: "local".to_owned(),
        access_token_expires_at: i64::MAX,
        refresh_token: "local".to_owned(),
        refresh_token_expires_at: i64::MAX,
        user: UserIdentity {
            id: "local-user".to_owned(),
            username: "Local".to_owned(),
            disabled: false,
            avatar_url: None,
        },
    }
}
