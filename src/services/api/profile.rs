use serde::Deserialize;

use super::{ApiError, BinaryMetadata};

#[derive(Clone, Debug, Deserialize)]
pub struct AvatarResponse {
    pub avatar_url: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct ProfileApi;

impl ProfileApi {
    pub async fn avatar(&self, _avatar_url: &str) -> Result<(String, Vec<u8>), ApiError> {
        Err(unavailable())
    }
    pub async fn avatar_metadata(&self, _avatar_url: &str) -> Result<BinaryMetadata, ApiError> {
        Err(unavailable())
    }
    pub async fn update_avatar(
        &self,
        _filename: String,
        _content_type: &str,
        _bytes: Vec<u8>,
    ) -> Result<AvatarResponse, ApiError> {
        Err(unavailable())
    }
    pub async fn delete_avatar(&self) -> Result<(), ApiError> {
        Ok(())
    }
}

fn unavailable() -> ApiError {
    ApiError::InvalidResponse("Profiles are not used by the local player".into())
}
