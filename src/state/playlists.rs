use std::{collections::HashMap, path::PathBuf};

use gpui::Task;
use gpui_tokio::Tokio;

use crate::services::api::{PlaylistResponse, PlaylistSort, PlaylistsApi};

pub struct PlaylistStore {
    playlists: Vec<PlaylistResponse>,
    covers: HashMap<String, PathBuf>,
    cover_urls: HashMap<String, String>,
    tasks: HashMap<String, Task<()>>,
    loading: bool,
    error: Option<String>,
}

impl PlaylistStore {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        let mut store = Self {
            playlists: Vec::new(),
            covers: HashMap::new(),
            cover_urls: HashMap::new(),
            tasks: HashMap::new(),
            loading: false,
            error: None,
        };
        store.refresh(cx);
        store
    }

    pub fn playlists(&self) -> &[PlaylistResponse] {
        &self.playlists
    }

    pub fn cover(&self, playlist_id: &str) -> Option<PathBuf> {
        self.covers.get(playlist_id).cloned()
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        if self.loading {
            return;
        }
        let Ok(api) = playlists_api(cx) else {
            self.error = Some("The playlist service is unavailable".to_owned());
            cx.notify();
            return;
        };
        self.loading = true;
        self.error = None;
        let request = Tokio::spawn(
            cx,
            async move { api.list(PlaylistSort::CreatedAtAsc).await },
        );
        self.tasks.insert(
            "playlists_refresh".to_owned(),
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |store, cx| {
                    store.tasks.remove("playlists_refresh");
                    store.loading = false;
                    match result {
                        Ok(Ok(playlists)) => {
                            store.playlists = playlists;
                            store.error = None;
                            store.sync_covers(cx);
                        }
                        Ok(Err(error)) => store.error = Some(error.user_message()),
                        Err(error) => {
                            store.error = Some(format!("Playlist request failed: {error}"))
                        }
                    }
                    cx.notify();
                });
            }),
        );
        cx.notify();
    }

    pub fn reload(&mut self, cx: &mut gpui::Context<Self>) {
        self.tasks.clear();
        self.loading = false;
        self.error = None;
        self.refresh(cx);
    }

    pub fn clear(&mut self, cx: &mut gpui::Context<Self>) {
        self.tasks.clear();
        self.playlists.clear();
        self.covers.clear();
        self.cover_urls.clear();
        self.loading = false;
        self.error = None;
        cx.notify();
    }

    fn sync_covers(&mut self, cx: &mut gpui::Context<Self>) {
        let active = self
            .playlists
            .iter()
            .filter_map(|playlist| {
                playlist
                    .cover_url
                    .as_ref()
                    .map(|url| (playlist.id.clone(), url.clone()))
            })
            .collect::<Vec<_>>();
        self.covers
            .retain(|playlist_id, _| active.iter().any(|(active_id, _)| active_id == playlist_id));
        self.cover_urls
            .retain(|playlist_id, _| active.iter().any(|(active_id, _)| active_id == playlist_id));

        let Ok(api) = playlists_api(cx) else {
            return;
        };
        for (playlist_id, cover_url) in active {
            let key = format!("playlist_cover_{playlist_id}");
            if self.tasks.contains_key(&key)
                || self.cover_urls.get(&playlist_id) == Some(&cover_url)
                    && self
                        .covers
                        .get(&playlist_id)
                        .is_some_and(|path| path.is_file())
            {
                continue;
            }
            let Some(cache_path) = playlist_cover_cache_path(&playlist_id, &cover_url) else {
                continue;
            };
            let request_api = api.clone();
            let request_url = cover_url.clone();
            let request_path = cache_path.clone();
            let request = Tokio::spawn(cx, async move {
                let (_, bytes) = request_api.cover(&request_url).await?;
                let parent = request_path.parent().ok_or_else(|| {
                    crate::services::api::ApiError::InvalidResponse(
                        "playlist cover cache path has no parent".to_owned(),
                    )
                })?;
                tokio::fs::create_dir_all(parent).await.map_err(|error| {
                    crate::services::api::ApiError::InvalidResponse(error.to_string())
                })?;
                tokio::fs::write(&request_path, bytes)
                    .await
                    .map_err(|error| {
                        crate::services::api::ApiError::InvalidResponse(error.to_string())
                    })?;
                Ok::<_, crate::services::api::ApiError>(request_path)
            });
            self.cover_urls
                .insert(playlist_id.clone(), cover_url.clone());
            let task_key = key.clone();
            self.tasks.insert(
                key,
                cx.spawn(async move |this, cx| {
                    let result = request.await;
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |store, cx| {
                        store.tasks.remove(&task_key);
                        match result {
                            Ok(Ok(path))
                                if store.cover_urls.get(&playlist_id) == Some(&cover_url) =>
                            {
                                store.covers.insert(playlist_id, path);
                            }
                            Ok(Err(error)) => {
                                log::warn!("failed to synchronize playlist cover: {error}");
                                if store.cover_urls.get(&playlist_id) == Some(&cover_url) {
                                    store.cover_urls.remove(&playlist_id);
                                }
                            }
                            _ => {}
                        }
                        cx.notify();
                    });
                }),
            );
        }
    }
}

fn playlists_api<C: 'static>(cx: &gpui::Context<C>) -> Result<PlaylistsApi, String> {
    let _ = cx;
    PlaylistsApi::local().map_err(|error| error.to_string())
}

fn playlist_cover_cache_path(playlist_id: &str, cover_url: &str) -> Option<PathBuf> {
    let version = cover_url
        .split_once("?v=")
        .map(|(_, version)| version)
        .unwrap_or("current");
    let safe_id = playlist_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect::<String>();
    let safe_version = version
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    Some(
        dirs::cache_dir()?
            .join("romus/playlist-covers")
            .join(format!(
                "{safe_id}-{}.image",
                if safe_version.is_empty() {
                    "current"
                } else {
                    &safe_version
                }
            )),
    )
}
