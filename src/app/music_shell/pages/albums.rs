use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use gpui::{
    ClipboardItem, Entity, EventEmitter, FontWeight, Image, ImageFormat, MouseButton, ObjectFit,
    PathPromptOptions, Render, SharedString, Task, div, img, prelude::*, px, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::{
        context_menu::{self, ContextMenu, ContextMenuItem},
        input::{Input, InputEvent, TextInput},
        modal::{self, Modal},
        toast,
    },
};

use crate::{
    domain::Page,
    resources::RomusIcons,
    services::{
        api::{AlbumResponse, BrowseQuery, PageQuery, PatchAlbumRequest, TrackResponse},
        library_api,
    },
    state::QueueOrigin,
    ui::{
        components::{
            media_cover::{self, CoverSource},
            track_list::{self, TrackMenuBuilder, TrackPlayHandler},
        },
        strings, theme,
    },
    utils::format_duration,
};

const PAGE_SIZE: u32 = 100;
const CARD: f32 = 174.;

pub enum AlbumsPageEvent {
    PlayFrom {
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
    },
    PlayNext(TrackResponse),
    Enqueue(TrackResponse),
    Navigate(Page),
}

pub struct AlbumsPage {
    album_id: Option<String>,
    albums: Vec<AlbumResponse>,
    album: Option<AlbumResponse>,
    tracks: Vec<TrackResponse>,
    total: i64,
    artwork: HashMap<String, Arc<Image>>,
    artwork_tasks: HashMap<String, Task<()>>,
    action_tasks: HashMap<String, Task<()>>,
    request_task: Option<Task<()>>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
    name_input: Entity<TextInput>,
    sort_input: Entity<TextInput>,
    year_input: Entity<TextInput>,
    name: SharedString,
    sort_name: SharedString,
    year: SharedString,
    cover_path: String,
    cover_dirty: bool,
}

impl EventEmitter<AlbumsPageEvent> for AlbumsPage {}

impl AlbumsPage {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        let name_input = cx.new(|cx| TextInput::new(cx).placeholder("Name"));
        cx.subscribe(&name_input, |page, _, event: &InputEvent, cx| {
            page.name = event.text().clone();
            cx.notify();
        })
        .detach();
        let sort_input = cx.new(|cx| TextInput::new(cx).placeholder("Sort Name"));
        cx.subscribe(&sort_input, |page, _, event: &InputEvent, cx| {
            page.sort_name = event.text().clone();
            cx.notify();
        })
        .detach();
        let year_input = cx.new(|cx| TextInput::new(cx).placeholder("Year (Optional)"));
        cx.subscribe(&year_input, |page, _, event: &InputEvent, cx| {
            page.year = event.text().clone();
            cx.notify();
        })
        .detach();
        Self {
            album_id: None,
            albums: Vec::new(),
            album: None,
            tracks: Vec::new(),
            total: 0,
            artwork: HashMap::new(),
            artwork_tasks: HashMap::new(),
            action_tasks: HashMap::new(),
            request_task: None,
            loading: false,
            loaded: false,
            error: None,
            generation: 0,
            name_input,
            sort_input,
            year_input,
            name: SharedString::from(""),
            sort_name: SharedString::from(""),
            year: SharedString::from(""),
            cover_path: String::new(),
            cover_dirty: false,
        }
    }

    pub fn playing_origin(&self) -> Option<QueueOrigin> {
        self.album.as_ref().map(|album| QueueOrigin::Album {
            id: album.id.clone(),
            name: album.name.clone(),
        })
    }

    pub fn show_list(&mut self, cx: &mut gpui::Context<Self>) {
        self.album_id = None;
        self.album = None;
        self.tracks.clear();
        self.error = None;
        if !self.loaded && !self.loading {
            self.load_more(cx);
        }
        cx.notify();
    }

    pub fn show(&mut self, album_id: String, cx: &mut gpui::Context<Self>) {
        if self.album_id.as_deref() == Some(album_id.as_str()) && self.album.is_some() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.album_id = Some(album_id);
        self.album = None;
        self.tracks.clear();
        self.request_task = None;
        self.loading = false;
        self.error = None;
        self.reset_form(cx);
        self.load_detail(cx);
    }

    pub fn reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.album_id = None;
        self.albums.clear();
        self.album = None;
        self.tracks.clear();
        self.total = 0;
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.action_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.loaded = false;
        self.error = None;
        self.reset_form(cx);
        cx.notify();
    }

    fn reset_form(&mut self, cx: &mut gpui::Context<Self>) {
        self.name = SharedString::from("");
        self.sort_name = SharedString::from("");
        self.year = SharedString::from("");
        self.cover_path.clear();
        self.cover_dirty = false;
        self.name_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.sort_input
            .update(cx, |input, cx| input.set_value("", cx));
        self.year_input
            .update(cx, |input, cx| input.set_value("", cx));
    }

    fn load_more(&mut self, cx: &mut gpui::Context<Self>) {
        if self.album_id.is_some()
            || self.loading
            || self.loaded && self.albums.len() as i64 >= self.total
        {
            return;
        }
        let Ok(api) = library_api(cx) else {
            self.error = Some("The library service is unavailable".to_owned());
            cx.notify();
            return;
        };
        let query = BrowseQuery {
            limit: Some(PAGE_SIZE),
            offset: Some(self.albums.len() as u64),
            ..Default::default()
        };
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let request = Tokio::spawn(cx, async move { api.albums(&query).await });
        self.request_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |page, cx| {
                if page.generation != generation {
                    return;
                }
                page.request_task = None;
                page.loading = false;
                match result {
                    Ok(Ok(response)) => {
                        page.total = response.total;
                        page.albums.extend(response.items);
                        page.loaded = true;
                        page.error = None;
                        page.sync_list_artwork(cx);
                    }
                    Ok(Err(error)) => {
                        page.loaded = true;
                        page.error = Some(error.user_message());
                    }
                    Err(error) => {
                        page.loaded = true;
                        page.error = Some(format!("Albums request failed: {error}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn load_detail(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(album_id) = self.album_id.clone() else {
            return;
        };
        let Ok(api) = library_api(cx) else {
            self.error = Some("The library service is unavailable".to_owned());
            cx.notify();
            return;
        };
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let tracks_api = api.clone();
        let request = Tokio::spawn(cx, async move {
            let page = PageQuery {
                limit: Some(200),
                offset: Some(0),
            };
            tokio::join!(
                api.album(&album_id),
                tracks_api.album_tracks(&album_id, &page)
            )
        });
        self.request_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |page, cx| {
                if page.generation != generation {
                    return;
                }
                page.request_task = None;
                page.loading = false;
                match result {
                    Ok((Ok(album), Ok(tracks))) => {
                        page.album = Some(album);
                        let mut items = tracks.items;
                        items.sort_by_key(|track| {
                            (
                                track.disc_number.unwrap_or(1),
                                track.track_number.unwrap_or(i64::MAX),
                            )
                        });
                        page.tracks = items;
                        page.error = None;
                        page.sync_detail_artwork(cx);
                    }
                    Ok((Err(error), _)) | Ok((_, Err(error))) => {
                        page.error = Some(error.user_message());
                    }
                    Err(error) => page.error = Some(format!("Album request failed: {error}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn sync_list_artwork(&mut self, cx: &mut gpui::Context<Self>) {
        let ids = self
            .albums
            .iter()
            .filter(|album| album.artwork_url.is_some())
            .map(|album| album.id.clone())
            .collect::<Vec<_>>();
        self.fetch_album_artwork(ids, cx);
    }

    fn sync_detail_artwork(&mut self, cx: &mut gpui::Context<Self>) {
        let mut album_ids = Vec::new();
        if let Some(album) = &self.album
            && album.artwork_url.is_some()
        {
            album_ids.push(album.id.clone());
        }
        self.fetch_album_artwork(album_ids, cx);
        let track_ids = self
            .tracks
            .iter()
            .filter(|track| track.has_artwork)
            .map(|track| track.id.clone())
            .collect::<Vec<_>>();
        self.fetch_track_artwork(track_ids, cx);
    }

    fn fetch_album_artwork(&mut self, ids: Vec<String>, cx: &mut gpui::Context<Self>) {
        let Ok(api) = library_api(cx) else {
            return;
        };
        for id in ids {
            if self.artwork.contains_key(&id) || self.artwork_tasks.contains_key(&id) {
                continue;
            }
            let request_api = api.clone();
            let request_id = id.clone();
            let request = Tokio::spawn(cx, async move {
                request_api.album_artwork(&request_id, None).await
            });
            let task_id = id.clone();
            self.artwork_tasks.insert(
                id,
                cx.spawn(async move |this, cx| {
                    let result = request.await;
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |page, cx| {
                        page.artwork_tasks.remove(&task_id);
                        if let Ok(Ok(response)) = result
                            && let Some(format) =
                                ImageFormat::from_mime_type(&response.metadata.content_type)
                        {
                            page.artwork.insert(
                                task_id,
                                Arc::new(Image::from_bytes(format, response.bytes)),
                            );
                        }
                        cx.notify();
                    });
                }),
            );
        }
    }

    fn fetch_track_artwork(&mut self, ids: Vec<String>, cx: &mut gpui::Context<Self>) {
        let Ok(api) = library_api(cx) else {
            return;
        };
        for id in ids {
            if self.artwork.contains_key(&id) || self.artwork_tasks.contains_key(&id) {
                continue;
            }
            let request_api = api.clone();
            let request_id = id.clone();
            let request = Tokio::spawn(
                cx,
                async move { request_api.artwork(&request_id, None).await },
            );
            let task_id = id.clone();
            self.artwork_tasks.insert(
                id,
                cx.spawn(async move |this, cx| {
                    let result = request.await;
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |page, cx| {
                        page.artwork_tasks.remove(&task_id);
                        if let Ok(Ok(response)) = result
                            && let Some(format) =
                                ImageFormat::from_mime_type(&response.metadata.content_type)
                        {
                            page.artwork.insert(
                                task_id,
                                Arc::new(Image::from_bytes(format, response.bytes)),
                            );
                        }
                        cx.notify();
                    });
                }),
            );
        }
    }

    fn open_edit(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        let Some(album) = self.album.clone() else {
            return;
        };
        let year = album
            .release_year
            .map(|year| year.to_string())
            .unwrap_or_default();
        self.name = album.name.clone().into();
        self.sort_name = album.sort_name.clone().into();
        self.year = year.clone().into();
        self.cover_path.clear();
        self.cover_dirty = false;
        self.name_input
            .update(cx, |input, cx| input.set_value(album.name, cx));
        self.sort_input
            .update(cx, |input, cx| input.set_value(album.sort_name, cx));
        self.year_input
            .update(cx, |input, cx| input.set_value(year, cx));
        self.show_edit_form(window, cx);
    }

    fn show_edit_form(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        let name_input = self.name_input.clone();
        let sort_input = self.sort_input.clone();
        let year_input = self.year_input.clone();
        let page_for_content = cx.entity();
        let page_for_footer = page_for_content.clone();
        modal::show(
            theme::style_modal(
                Modal::new(move |_, app| {
                    let cover_page = page_for_content.clone();
                    let cover_path = page_for_content.read(app).cover_path.clone();
                    let cover_dirty = page_for_content.read(app).cover_dirty;
                    let album = page_for_content.read(app).album.clone();
                    let artwork = album
                        .as_ref()
                        .and_then(|album| page_for_content.read(app).artwork.get(&album.id))
                        .cloned();
                    let has_cover = !cover_path.is_empty() || (!cover_dirty && artwork.is_some());
                    div()
                        .w_full()
                        .px(px(26.))
                        .pb(px(8.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .mt(px(18.))
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(21.))
                                .font_weight(FontWeight::BOLD)
                                .child("Edit Album"),
                        )
                        .child(
                            div()
                                .mt(px(28.))
                                .flex()
                                .flex_col()
                                .items_center()
                                .child(
                                    div()
                                        .id("album-cover-picker")
                                        .size(px(180.))
                                        .rounded(px(14.))
                                        .overflow_hidden()
                                        .border_4()
                                        .border_color(rgba(0xfa3558ff))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|cover| cover.bg(rgba(0xffffff08)))
                                        .on_click(move |_, _, app| {
                                            cover_page.update(app, |page, cx| {
                                                page.select_cover(cx);
                                            });
                                        })
                                        .when(!cover_path.is_empty(), |cover| {
                                            cover.child(
                                                img(PathBuf::from(cover_path.clone()))
                                                    .size_full()
                                                    .object_fit(ObjectFit::Cover),
                                            )
                                        })
                                        .when(cover_path.is_empty() && !cover_dirty, |cover| {
                                            cover.when_some(artwork, |cover, image| {
                                                cover.child(
                                                    img(image)
                                                        .size_full()
                                                        .object_fit(ObjectFit::Cover),
                                                )
                                            })
                                        })
                                        .when(!has_cover, |cover| {
                                            cover.child(
                                                div()
                                                    .size(px(40.))
                                                    .rounded_full()
                                                    .bg(rgba(0xfa3558ff))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(
                                                        svg()
                                                            .path(LucideIcons::Plus)
                                                            .size(px(22.))
                                                            .text_color(rgba(0xffffffff)),
                                                    ),
                                            )
                                        }),
                                )
                                .when(has_cover, |cover| {
                                    let remove_page = page_for_content.clone();
                                    cover.child(
                                        div()
                                            .id("album-cover-remove")
                                            .mt(px(12.))
                                            .h(px(28.))
                                            .px(px(12.))
                                            .rounded(px(8.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .text_size(px(12.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgba(0xff6684d8))
                                            .hover(|button| button.bg(rgba(0xffffff10)))
                                            .on_click(move |_, _, app| {
                                                remove_page.update(app, |page, cx| {
                                                    page.clear_cover(cx);
                                                });
                                            })
                                            .child("Remove Cover"),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .mt(px(28.))
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(theme::style_field(
                                    Input::new(&name_input).appearance(theme::input_appearance()),
                                ))
                                .child(theme::style_field(
                                    Input::new(&sort_input).appearance(theme::input_appearance()),
                                ))
                                .child(theme::style_field(
                                    Input::new(&year_input).appearance(theme::input_appearance()),
                                )),
                        )
                })
                .footer(move |_, app| {
                    let save_page = page_for_footer.clone();
                    let cancel_page = page_for_footer.clone();
                    let disabled = page_for_footer.read(app).name.trim().is_empty()
                        || page_for_footer
                            .read(app)
                            .action_tasks
                            .contains_key("save_album");
                    form_footer(
                        disabled,
                        move |window, app| {
                            save_page.update(app, |page, cx| page.save(window, cx));
                        },
                        move |window, app| {
                            cancel_page.update(app, |page, cx| page.reset_form(cx));
                            modal::dismiss(window, app);
                        },
                    )
                })
                .w(px(440.))
                .ok_on_enter(false),
            ),
            window,
            cx,
        );
    }

    fn select_cover(&mut self, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Image".into()),
        });
        let task = cx.spawn(async move |this, app| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
                && path.is_file()
            {
                let _ = this.update(app, |page, cx| {
                    page.cover_path = path.to_string_lossy().to_string();
                    page.cover_dirty = true;
                    cx.notify();
                });
            }
        });
        self.action_tasks.insert("pick_cover".to_owned(), task);
    }

    fn clear_cover(&mut self, cx: &mut gpui::Context<Self>) {
        self.cover_path.clear();
        self.cover_dirty = true;
        cx.notify();
    }

    fn save(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        let Some(album_id) = self.album_id.clone() else {
            return;
        };
        if self.name.trim().is_empty() {
            toast::error("An album name is required", cx);
            return;
        }
        let release_year = if self.year.trim().is_empty() {
            Some(None)
        } else {
            match self.year.trim().parse::<i64>() {
                Ok(year) if (1000..=9999).contains(&year) => Some(Some(year)),
                _ => {
                    toast::error("Enter a valid year", cx);
                    return;
                }
            }
        };
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let request = PatchAlbumRequest {
            name: Some(self.name.trim().to_owned()),
            sort_name: Some(self.sort_name.trim().to_owned()),
            release_year,
            album_artist_id: None,
        };
        let cover_path = self
            .cover_dirty
            .then(|| PathBuf::from(&self.cover_path))
            .filter(|path| path.is_file());
        let remove_cover = self.cover_dirty
            && cover_path.is_none()
            && self
                .album
                .as_ref()
                .is_some_and(|album| album.artwork_url.is_some());
        if self.action_tasks.contains_key("save_album") {
            return;
        }
        let request_id = album_id.clone();
        let request = Tokio::spawn(cx, async move {
            let album = api.patch_album(&request_id, &request).await?;
            let cover_error = if let Some(path) = cover_path {
                match tokio::fs::read(&path).await {
                    Ok(bytes) => {
                        let filename = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("album-cover")
                            .to_owned();
                        api.replace_album_artwork(
                            &album.id,
                            filename,
                            image_content_type(&path),
                            bytes,
                        )
                        .await
                        .err()
                    }
                    Err(error) => Some(crate::services::api::ApiError::InvalidResponse(format!(
                        "failed to read album cover: {error}"
                    ))),
                }
            } else if remove_cover {
                api.delete_album_artwork(&album.id).await.err()
            } else {
                None
            };
            let album = if cover_error.is_none() {
                api.album(&album.id).await.unwrap_or(album)
            } else {
                album
            };
            Ok::<_, crate::services::api::ApiError>((album, cover_error))
        });
        self.action_tasks.insert(
            "save_album".to_owned(),
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |page, cx| {
                    page.action_tasks.remove("save_album");
                    match result {
                        Ok(Ok((album, cover_error))) => {
                            if let Some(error) = cover_error {
                                toast::error(error.user_message(), cx);
                            } else {
                                toast::success("Album updated", cx);
                            }
                            page.artwork.remove(&album.id);
                            if let Some(index) =
                                page.albums.iter().position(|item| item.id == album.id)
                            {
                                page.albums[index] = album.clone();
                            }
                            page.album = Some(album);
                            page.reset_form(cx);
                            page.sync_detail_artwork(cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Update album failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
        modal::dismiss(window, cx);
        cx.notify();
    }

    fn confirm_delete(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        let Some(album) = self.album.clone() else {
            return;
        };
        let name = album.name.clone();
        let album_id = album.id;
        let page = cx.entity();
        let appearance = theme::dark_modal_appearance().ok_button(
            theme::dark_modal_appearance()
                .ok_button
                .background(rgba(0xe73558ff).into())
                .border(rgba(0xe73558ff).into())
                .hover_background(rgba(0xf24868ff).into()),
        );
        modal::show(
            theme::style_modal(
                Modal::new(move |_, _| {
                    div()
                        .py(px(8.))
                        .text_size(px(12.))
                        .text_color(rgba(0xc7c5cfca))
                        .child(format!(
                            "Delete “{name}”? Albums that still have tracks cannot be deleted."
                        ))
                })
                .title_text("Delete Album")
                .ok_label("Delete")
                .cancel_label("Cancel")
                .on_ok(move |_, cx| {
                    page.update(cx, |page, cx| page.delete_album(album_id.clone(), cx));
                    true
                })
                .appearance(appearance)
                .w(px(420.)),
            ),
            window,
            cx,
        );
    }

    fn delete_album(&mut self, album_id: String, cx: &mut gpui::Context<Self>) {
        if self.action_tasks.contains_key("delete_album") {
            return;
        }
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let request_id = album_id.clone();
        let request = Tokio::spawn(cx, async move { api.delete_album(&request_id).await });
        self.action_tasks.insert(
            "delete_album".to_owned(),
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |page, cx| {
                    page.action_tasks.remove("delete_album");
                    match result {
                        Ok(Ok(())) => {
                            page.albums.retain(|album| album.id != album_id);
                            page.artwork.remove(&album_id);
                            page.total = page.total.saturating_sub(1);
                            toast::success("Album deleted", cx);
                            cx.emit(AlbumsPageEvent::Navigate(Page::Albums));
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Delete album failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    fn manage_menu(entity: Entity<Self>) -> ContextMenu {
        let edit_entity = entity.clone();
        let delete_entity = entity;
        theme::style_context_menu(
            ContextMenu::new()
                .action_with(
                    |_, _| menu_label(LucideIcons::Pencil, "Edit Album"),
                    move |window, cx| {
                        edit_entity.update(cx, |page, cx| page.open_edit(window, cx));
                    },
                )
                .separator()
                .item(
                    ContextMenuItem::action_with(
                        |_, _| menu_label(LucideIcons::Trash2, "Delete Album"),
                        move |window, cx| {
                            delete_entity.update(cx, |page, cx| page.confirm_delete(window, cx));
                        },
                    )
                    .danger(),
                ),
        )
    }

    fn track_menu(entity: Entity<Self>, track: TrackResponse) -> ContextMenu {
        let copy_track = track.clone();
        let artist_entity = entity;
        let artist_id = track.artist_id.clone();
        let mut menu = ContextMenu::new()
            .action_with(
                |_, _| menu_label(LucideIcons::ListPlus, crate::ui::strings::play_next()),
                {
                    let entity = artist_entity.clone();
                    let queued = track.clone();
                    move |_, cx| {
                        entity.update(cx, |_, cx| {
                            cx.emit(AlbumsPageEvent::PlayNext(queued.clone()));
                        });
                    }
                },
            )
            .action_with(
                |_, _| menu_label(LucideIcons::ListEnd, crate::ui::strings::add_to_queue()),
                {
                    let entity = artist_entity.clone();
                    let queued = track.clone();
                    move |_, cx| {
                        entity.update(cx, |_, cx| {
                            cx.emit(AlbumsPageEvent::Enqueue(queued.clone()));
                        });
                    }
                },
            )
            .separator()
            .action_with(
                |_, _| menu_label(LucideIcons::Copy, "Copy"),
                move |_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(song_clipboard_text(
                        &copy_track,
                    )));
                    toast::success("Song info copied", cx);
                },
            );
        if let Some(artist_id) = artist_id {
            menu = menu.action_with(
                |_, _| menu_label(LucideIcons::MicVocal, "Go to Artist"),
                move |_, cx| {
                    artist_entity.update(cx, |_, cx| {
                        cx.emit(AlbumsPageEvent::Navigate(Page::Artist(
                            artist_id.clone().into(),
                        )));
                    });
                },
            );
        }
        theme::style_context_menu(menu)
    }
}

impl Render for AlbumsPage {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        if self.album_id.is_some() {
            self.render_detail(cx).into_any_element()
        } else {
            self.render_list(cx).into_any_element()
        }
    }
}

impl AlbumsPage {
    fn render_list(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.loaded && !self.loading {
            self.load_more(cx);
        }
        let has_more = self.loaded && (self.albums.len() as i64) < self.total;
        let empty = self.loaded && self.albums.is_empty();
        let artwork = self.artwork.clone();
        let entity = cx.entity();
        let cards = self
            .albums
            .iter()
            .cloned()
            .map(|album| {
                let cover = artwork.get(&album.id).cloned().map(CoverSource::Image);
                album_card(album, cover, entity.clone())
            })
            .collect::<Vec<_>>();
        div()
            .id("albums-list-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(24.))
            .pb(px(118.))
            .text_color(rgba(0xf1eff5ef))
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::BOLD)
                    .child(strings::albums()),
            )
            .when(self.loading && self.albums.is_empty(), |page| {
                page.child(status_text(strings::loading_albums()))
            })
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when(empty && self.error.is_none(), |page| {
                page.child(status_text(strings::albums_empty()))
            })
            .child(
                div()
                    .mt(px(15.))
                    .flex()
                    .flex_wrap()
                    .gap_x(px(17.))
                    .gap_y(px(23.))
                    .children(cards),
            )
            .when(has_more, |page| {
                page.child(load_more(
                    "load-more-albums",
                    self.loading,
                    cx.listener(|page, _, _, cx| page.load_more(cx)),
                ))
            })
    }

    fn render_detail(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let album = self.album.clone();
        let tracks = self.tracks.clone();
        let artwork = self.artwork.clone();
        let entity = cx.entity();
        let menu_builder: TrackMenuBuilder =
            Rc::new(move |track, _, _| AlbumsPage::track_menu(entity.clone(), track));
        let play_entity = cx.entity();
        let on_play: TrackPlayHandler = Rc::new(move |track, _, cx| {
            play_entity.update(cx, |page, cx| {
                let start = page
                    .tracks
                    .iter()
                    .position(|item| item.id == track.id)
                    .unwrap_or(0);
                cx.emit(AlbumsPageEvent::PlayFrom {
                    tracks: page.tracks.clone(),
                    start,
                    shuffle: false,
                });
            });
        });
        div()
            .id("album-detail-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(20.))
            .pb(px(118.))
            .text_color(rgba(0xf1eff5ef))
            .when(self.loading && album.is_none(), |page| {
                page.child(status_text(strings::loading_album()))
            })
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when_some(album, |page, album| {
                let cover = artwork.get(&album.id).cloned().map(CoverSource::Image);
                page.child(album_header(&album, cover, tracks.clone(), cx))
                    .child(
                        div()
                            .mt(px(28.))
                            .when(tracks.is_empty(), |list| {
                                list.child(status_text("This album has no songs"))
                            })
                            .when(!tracks.is_empty(), |list| {
                                list.child(track_list::track_list(
                                    &tracks,
                                    &artwork,
                                    menu_builder,
                                    on_play,
                                ))
                            }),
                    )
            })
    }
}

fn album_card(
    album: AlbumResponse,
    cover: Option<CoverSource>,
    entity: Entity<AlbumsPage>,
) -> impl IntoElement {
    let page = Page::Album(album.id.clone().into());
    let name = album.name.clone();
    let artist = album.album_artist_name.clone();
    div()
        .id(album.id.clone())
        .w(px(CARD))
        .flex()
        .flex_col()
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            entity.update(cx, |_, cx| {
                cx.emit(AlbumsPageEvent::Navigate(page.clone()));
            });
        })
        .child(media_cover::media_cover(
            &name,
            cover,
            px(CARD),
            px(11.),
            true,
        ))
        .child(
            div()
                .mt(px(8.))
                .truncate()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .child(name),
        )
        .child(
            div()
                .mt(px(2.))
                .truncate()
                .text_size(px(10.5))
                .text_color(rgba(0xb3afbb9c))
                .child(artist),
        )
        .when_some(album.release_year, |card, year| {
            card.child(
                div()
                    .mt(px(1.))
                    .text_size(px(10.))
                    .text_color(rgba(0xa8a5b18f))
                    .child(year.to_string()),
            )
        })
}

fn album_header(
    album: &AlbumResponse,
    cover: Option<CoverSource>,
    tracks: Vec<TrackResponse>,
    cx: &mut gpui::Context<AlbumsPage>,
) -> impl IntoElement {
    let entity = cx.entity();
    let artist_id = album.album_artist_id.clone();
    let artist_name = album.album_artist_name.clone();
    let year = album.release_year;
    div()
        .min_h(px(212.))
        .flex()
        .items_end()
        .gap(px(25.))
        .child(media_cover::media_cover(
            &album.name,
            cover,
            px(212.),
            px(14.),
            true,
        ))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .pb(px(6.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(px(32.))
                        .font_weight(FontWeight::BOLD)
                        .child(album.name.clone()),
                )
                .child(
                    div()
                        .id("album-artist-link")
                        .mt(px(12.))
                        .truncate()
                        .text_size(px(14.))
                        .text_color(rgba(0xb8b5c0a6))
                        .cursor_pointer()
                        .hover(|link| link.text_color(rgba(0xff6684d8)))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(AlbumsPageEvent::Navigate(Page::Artist(
                                artist_id.clone().into(),
                            )));
                        }))
                        .child(artist_name),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .text_size(px(13.))
                        .text_color(rgba(0xa8a5b18f))
                        .child(match year {
                            Some(year) => format!(
                                "{year} · {} songs · {}",
                                album.track_count,
                                format_duration(album.duration_ms)
                            ),
                            None => format!(
                                "{} songs · {}",
                                album.track_count,
                                format_duration(album.duration_ms)
                            ),
                        }),
                )
                .child(
                    div()
                        .mt(px(17.))
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .child(play_context_button(
                            "album-play",
                            RomusIcons::MusicPlay,
                            "Play",
                            tracks.clone(),
                            false,
                            cx,
                        ))
                        .child(play_context_button(
                            "album-shuffle",
                            LucideIcons::Shuffle,
                            "Shuffle",
                            tracks,
                            true,
                            cx,
                        ))
                        .child(more_button(entity)),
                ),
        )
}

fn play_context_button(
    id: &'static str,
    icon: impl Into<SharedString>,
    label: &'static str,
    tracks: Vec<TrackResponse>,
    shuffle: bool,
    cx: &mut gpui::Context<AlbumsPage>,
) -> impl IntoElement {
    let enabled = !tracks.is_empty();
    div()
        .id(id)
        .h(px(38.))
        .px(px(18.))
        .rounded_full()
        .bg(rgba(0xffffff0c))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(0xff5576d0))
        .when(enabled, |button| button.cursor_pointer())
        .on_click(cx.listener(move |_, _, _, cx| {
            if tracks.is_empty() {
                return;
            }
            cx.emit(AlbumsPageEvent::PlayFrom {
                tracks: tracks.clone(),
                start: 0,
                shuffle,
            });
        }))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xff5576d0)))
        .child(label)
}

fn more_button(entity: Entity<AlbumsPage>) -> impl IntoElement {
    let menu_entity = entity;
    div()
        .id("album-more")
        .size(px(38.))
        .rounded_full()
        .bg(rgba(0xffffff0c))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|button| button.bg(rgba(0xffffff14)))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let menu = AlbumsPage::manage_menu(menu_entity.clone());
            let _ = context_menu::show(menu, event.position, window, cx);
            cx.stop_propagation();
        })
        .child(
            svg()
                .path(LucideIcons::EllipsisVertical)
                .size(px(16.))
                .text_color(rgba(0xff5576d0)),
        )
}

fn form_footer(
    disabled: bool,
    on_save: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
    on_cancel: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .h(px(74.))
        .px(px(26.))
        .border_t_1()
        .border_color(rgba(0xffffff16))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .id("catalog-edit-cancel")
                .w(px(100.))
                .h(px(40.))
                .rounded_full()
                .bg(rgba(0x292d3aff))
                .text_color(rgba(0xf3f3f6f0))
                .text_size(px(15.))
                .font_weight(FontWeight::MEDIUM)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|button| button.bg(rgba(0x343947ff)))
                .on_click(move |_, window, app| on_cancel(window, app))
                .child("Cancel"),
        )
        .child(
            div()
                .id("catalog-edit-save")
                .w(px(100.))
                .h(px(40.))
                .rounded_full()
                .bg(if disabled {
                    rgba(0x292d39ff)
                } else {
                    rgba(0xfa3558ff)
                })
                .text_color(if disabled {
                    rgba(0x8c8f9b9c)
                } else {
                    rgba(0xffffffff)
                })
                .text_size(px(15.))
                .font_weight(FontWeight::MEDIUM)
                .flex()
                .items_center()
                .justify_center()
                .when(!disabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(|button| button.bg(rgba(0xff496aff)))
                        .on_click(move |_, window, app| on_save(window, app))
                })
                .child("Save"),
        )
}

fn load_more(
    id: &'static str,
    loading: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .mt(px(18.))
        .mx_auto()
        .w(px(112.))
        .h(px(34.))
        .rounded(px(17.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .bg(rgba(0xffffff0d))
        .hover(|button| button.bg(rgba(0xffffff16)))
        .text_size(px(10.5))
        .child(if loading {
            strings::loading()
        } else {
            "Load More"
        })
        .on_click(on_click)
}

fn menu_label(icon: LucideIcons, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xf2f0f6f2)))
        .child(label)
}

fn song_clipboard_text(track: &TrackResponse) -> String {
    let title = track.title.as_deref().unwrap_or("Untitled");
    let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
    match track.album.as_deref() {
        Some(album) => format!("{title} — {artist}\nAlbum: {album}"),
        None => format!("{title} — {artist}"),
    }
}

fn status_text(message: impl Into<String>) -> impl IntoElement {
    div()
        .mt(px(18.))
        .text_size(px(11.))
        .text_color(rgba(0xb3afbbb0))
        .child(message.into())
}

fn image_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}
