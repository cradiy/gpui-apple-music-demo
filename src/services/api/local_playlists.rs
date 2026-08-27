use super::{
    ApiError, PlaylistCoverResponse, PlaylistDetailResponse, PlaylistItemResponse,
    PlaylistItemSort, PlaylistResponse, PlaylistSort, PlaylistWriteRequest,
};
use crate::services::local::LocalStore;

#[derive(Clone)]
pub struct PlaylistsApi {
    store: LocalStore,
}

impl PlaylistsApi {
    pub fn local() -> Result<Self, ApiError> {
        Ok(Self {
            store: LocalStore::shared()?,
        })
    }

    pub async fn create(
        &self,
        request: &PlaylistWriteRequest,
    ) -> Result<PlaylistResponse, ApiError> {
        self.store
            .create_playlist(request.name.clone(), request.description.clone())
    }

    pub async fn list(&self, sort: PlaylistSort) -> Result<Vec<PlaylistResponse>, ApiError> {
        let mut playlists = self.store.playlists()?;
        match sort {
            PlaylistSort::Custom => {}
            PlaylistSort::CreatedAtAsc => playlists.sort_by_key(|playlist| playlist.created_at),
            PlaylistSort::CreatedAtDesc => {
                playlists.sort_by_key(|playlist| std::cmp::Reverse(playlist.created_at))
            }
            PlaylistSort::NameAsc => playlists.sort_by_key(|playlist| playlist.name.to_lowercase()),
            PlaylistSort::NameDesc => {
                playlists.sort_by_key(|playlist| std::cmp::Reverse(playlist.name.to_lowercase()))
            }
        }
        Ok(playlists)
    }

    pub async fn reorder(
        &self,
        playlist_ids: &[String],
    ) -> Result<Vec<PlaylistResponse>, ApiError> {
        self.store.reorder_playlists(playlist_ids)
    }

    pub async fn get(
        &self,
        playlist_id: &str,
        sort: PlaylistItemSort,
    ) -> Result<PlaylistDetailResponse, ApiError> {
        let mut detail = self.store.playlist(playlist_id)?;
        match sort {
            PlaylistItemSort::Custom => {}
            PlaylistItemSort::AddedAtAsc => detail.items.sort_by_key(|item| item.added_at),
            PlaylistItemSort::AddedAtDesc => detail
                .items
                .sort_by_key(|item| std::cmp::Reverse(item.added_at)),
            PlaylistItemSort::NameAsc => detail
                .items
                .sort_by_key(|item| item.track.title.clone().unwrap_or_default().to_lowercase()),
            PlaylistItemSort::NameDesc => detail.items.sort_by_key(|item| {
                std::cmp::Reverse(item.track.title.clone().unwrap_or_default().to_lowercase())
            }),
        }
        Ok(detail)
    }

    pub async fn replace(
        &self,
        playlist_id: &str,
        request: &PlaylistWriteRequest,
    ) -> Result<PlaylistResponse, ApiError> {
        self.store.replace_playlist(
            playlist_id,
            request.name.clone(),
            request.description.clone(),
        )
    }

    pub async fn cover(&self, cover_url: &str) -> Result<(String, Vec<u8>), ApiError> {
        let playlist_id = local_playlist_id(cover_url)?;
        let artwork = self
            .store
            .artwork("playlist", playlist_id)?
            .ok_or_else(not_found)?;
        Ok((artwork.mime_type, artwork.data))
    }

    pub async fn update_cover(
        &self,
        playlist_id: &str,
        _filename: String,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<PlaylistCoverResponse, ApiError> {
        let artwork = self
            .store
            .set_artwork("playlist", playlist_id, content_type, bytes)?;
        Ok(PlaylistCoverResponse {
            cover_url: format!(
                "local://playlist/{playlist_id}/cover?v={}",
                artwork.updated_at
            ),
            content_type: artwork.content_type,
            size_bytes: artwork.size_bytes,
            updated_at: artwork.updated_at,
        })
    }

    pub async fn delete_cover(&self, playlist_id: &str) -> Result<(), ApiError> {
        self.store.delete_artwork("playlist", playlist_id)
    }
    pub async fn delete(&self, playlist_id: &str) -> Result<(), ApiError> {
        self.store.delete_playlist(playlist_id)
    }

    pub async fn add_item(
        &self,
        playlist_id: &str,
        track_id: &str,
    ) -> Result<PlaylistItemResponse, ApiError> {
        self.store.add_playlist_item(playlist_id, track_id)
    }

    pub async fn reorder_items(
        &self,
        playlist_id: &str,
        item_ids: &[String],
    ) -> Result<Vec<PlaylistItemResponse>, ApiError> {
        self.store.reorder_playlist_items(playlist_id, item_ids)
    }

    pub async fn remove_item(&self, playlist_id: &str, item_id: &str) -> Result<(), ApiError> {
        self.store.remove_playlist_item(playlist_id, item_id)
    }
}

fn local_playlist_id(url: &str) -> Result<&str, ApiError> {
    url.strip_prefix("local://playlist/")
        .and_then(|value| value.split('/').next())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::InvalidResponse("invalid local playlist cover URL".into()))
}

fn not_found() -> ApiError {
    ApiError::Http {
        status: 404,
        code: "not_found".into(),
        message: "playlist cover was not found".into(),
        request_id: None,
    }
}
