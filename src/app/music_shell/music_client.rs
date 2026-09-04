use gpui::{Image, ImageFormat, Task};
use gpui_tokio::Tokio;
use std::sync::Arc;

use crate::ui::components::feedback as toast;
use crate::{
    app::music_shell::{MusicEvent, MusicShell},
    services::profile_api,
    services::session::SessionStore,
    services::{api::ApiError, library_api},
};

impl MusicShell {
    pub fn toggle_current_favorite(&mut self, cx: &mut gpui::Context<Self>) {
        if self.favorite_task.is_some() {
            return;
        }
        let Some(track) = self.playback.read(cx).queue.current().cloned() else {
            return;
        };
        let liked = !track.favorite;
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let track_id = track.id;
        let request = Tokio::spawn(cx, async move {
            if liked {
                api.favorite(&track_id).await
            } else {
                api.unfavorite(&track_id).await
            }
        });
        self.favorite_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |shell, cx| {
                shell.favorite_task = None;
                match result {
                    Ok(Ok(updated)) => {
                        shell.playback.update(cx, |playback, cx| {
                            playback.update_queue_track(updated, cx);
                        });
                        shell
                            .content_shell
                            .update(cx, |content, cx| content.refresh_library(cx));
                        toast::success(if liked { "Song liked" } else { "Song unliked" }, cx);
                    }
                    Ok(Err(error)) => toast::error(error.user_message(), cx),
                    Err(error) => toast::error(format!("Like request failed: {error}"), cx),
                }
                cx.notify();
            });
        }));
    }

    pub fn signed_in(&mut self, cx: &mut gpui::Context<Self>) {
        self.restore_ui_memory(cx);
        self.playback
            .update(cx, |playback, cx| playback.restore(cx));
        self.songs_search_input
            .update(cx, |input, cx| input.clear(cx));
        self.playlist_search_input
            .update(cx, |input, cx| input.clear(cx));
        self.imports.update(cx, |imports, cx| imports.clear(cx));
        self.playlists
            .update(cx, |playlists, cx| playlists.reload(cx));
        self.content_shell
            .update(cx, |content, cx| content.signed_in(cx));
        self.user_avatar = None;
        self.avatar_task = Self::start_avatar_request(cx);
        cx.notify();
    }

    pub fn logout(&mut self, cx: &mut gpui::Context<Self>) {
        if self.logging_out {
            return;
        }
        let session_store = cx.global::<SessionStore>().clone();
        let Some(_session) = session_store.current() else {
            cx.emit(MusicEvent::SignedOut);
            return;
        };

        self.logging_out = true;
        cx.notify();
        let request = Tokio::spawn(cx, async move { session_store.logout().await });
        self.logout_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |shell, cx| {
                match result {
                    Ok(Ok(())) => log::info!("signed out"),
                    Ok(Err(error)) => {
                        log::warn!("unable to reset local session: {error}")
                    }
                    Err(error) => {
                        log::warn!("logout task failed; clearing local session: {error}")
                    }
                }
                shell.logging_out = false;
                shell.clear_local_session(cx);
                cx.emit(MusicEvent::SignedOut);
                cx.notify();
            });
        }));
    }

    pub(super) fn start_avatar_request(cx: &mut gpui::Context<Self>) -> Option<Task<()>> {
        let session_store = cx.global::<SessionStore>().clone();
        let session = session_store.current()?;
        let avatar_url = session.user.avatar_url.clone()?;
        let api = profile_api(cx).ok()?;
        let request = Tokio::spawn(cx, async move { api.avatar(&avatar_url).await });
        Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |shell, cx| {
                match result {
                    Ok(Ok((content_type, bytes))) => {
                        if let Some(format) = ImageFormat::from_mime_type(&content_type) {
                            shell.user_avatar = Some(Arc::new(Image::from_bytes(format, bytes)));
                        } else {
                            log::warn!("local avatar has unsupported type {content_type}");
                        }
                    }
                    Ok(Err(ApiError::AuthenticationRequired)) => {
                        log::info!("the saved session is no longer valid");
                        cx.emit(MusicEvent::SignedOut);
                    }
                    Ok(Err(error)) => log::warn!("unable to load user avatar: {error}"),
                    Err(error) => log::warn!("avatar task failed: {error}"),
                }
                cx.notify();
            });
        }))
    }
}
