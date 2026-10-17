use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gpui::{
    ClickEvent, Div, Entity, Focusable, FontWeight, MouseButton, ObjectFit, PathPromptOptions,
    SharedString, Stateful, Task, Window, WindowControlArea, div, img, prelude::*, px, relative,
    rgba, svg,
};
use gpui_effects::FrostedGlassAppearance;
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::{
        context_menu::{ContextMenu, ContextMenuExt, ContextMenuItem},
        input::{Input, InputAppearance, InputEvent, TextInput},
        modal::{self, Modal},
    },
};
use uuid::Uuid;

use crate::ui::components::feedback as toast;
use crate::{
    domain::Page,
    services::{
        api::{PlaylistResponse, PlaylistWriteRequest, PlaylistsApi},
        config::LoadedClientConfig,
    },
    state::PlaylistStore,
    ui::{
        components::media_cover::{self, CoverSource},
        theme,
    },
};

pub fn input_appearance() -> InputAppearance {
    theme::input_appearance()
}

fn description_input_appearance() -> InputAppearance {
    InputAppearance {
        focus_border: rgba(0xfa2d55a8).into(),
        ..input_appearance()
    }
}

pub struct Sidebar {
    selected_page: Page,
    playlists_expanded: bool,
    playlists: Entity<PlaylistStore>,
    tasks: HashMap<String, Task<()>>,

    playlist_cover_path: String,
    playlist_cover_dirty: bool,
    editing_playlist_id: Option<String>,
    playlist_input: Entity<TextInput>,
    playlist_desc_input: Entity<TextInput>,
    playlist_desc: SharedString,
    playlist_name: SharedString,
}

impl Sidebar {
    pub fn new(
        cx: &mut gpui::Context<Sidebar>,
        selected: Page,
        playlists: Entity<PlaylistStore>,
    ) -> Self {
        cx.observe(&playlists, |_, _, cx| cx.notify()).detach();
        let playlist_input = cx.new(|cx| TextInput::new(cx).placeholder("Playlist Title"));
        cx.subscribe(&playlist_input, |app, _entity, event: &InputEvent, cx| {
            app.playlist_name = event.text().clone();
            cx.notify();
        })
        .detach();

        let playlist_desc = cx.new(|cx| {
            TextInput::new(cx)
                .placeholder("Description (Optional)")
                .mode(uic::components::input::InputMode::Multiline)
        });
        cx.subscribe(&playlist_desc, |app, _entity, event: &InputEvent, cx| {
            app.playlist_desc = event.text().clone();
            cx.notify();
        })
        .detach();

        Self {
            selected_page: selected,
            playlists_expanded: true,
            playlist_input,
            playlist_name: SharedString::from(""),
            tasks: HashMap::new(),
            playlists,
            playlist_desc: SharedString::from(""),
            playlist_desc_input: playlist_desc,
            playlist_cover_path: String::new(),
            playlist_cover_dirty: false,
            editing_playlist_id: None,
        }
    }

    pub fn set_selected_page(&mut self, page: Page, cx: &mut gpui::Context<Self>) {
        self.selected_page = page;
        cx.notify();
    }

    pub fn select_playlist_cover(&mut self, cx: &mut gpui::Context<Sidebar>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Image".into()),
        });
        let id = Uuid::new_v4();
        let task = cx.spawn(async move |this, app| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
                && path.is_file()
            {
                let _ = this.update(app, |this, cx| {
                    this.playlist_cover_path = path.to_string_lossy().to_string();
                    this.playlist_cover_dirty = true;
                    this.tasks.remove(&id.to_string());
                    cx.notify();
                });
            }
        });
        self.tasks.insert(id.to_string(), task);
    }

    pub fn clear_playlist_cover(&mut self, cx: &mut gpui::Context<Sidebar>) {
        self.playlist_cover_path.clear();
        self.playlist_cover_dirty = true;
        cx.notify();
    }

    pub fn playlists_api(&self, cx: &gpui::Context<Sidebar>) -> Option<PlaylistsApi> {
        let _ = cx;
        PlaylistsApi::local().ok()
    }

    pub fn open_playlist_modal(&mut self, window: &mut Window, cx: &mut gpui::Context<Sidebar>) {
        self.reset_playlist_form(cx);
        self.show_playlist_form(window, cx);
    }

    pub fn open_edit_playlist(
        &mut self,
        playlist: PlaylistResponse,
        window: &mut Window,
        cx: &mut gpui::Context<Sidebar>,
    ) {
        self.reset_playlist_form(cx);
        self.editing_playlist_id = Some(playlist.id.clone());
        self.playlist_name = playlist.name.clone().into();
        self.playlist_desc = playlist.description.clone().unwrap_or_default().into();
        self.playlist_cover_path = self
            .playlists
            .read(cx)
            .cover(&playlist.id)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.playlist_cover_dirty = false;
        self.playlist_input.update(cx, |input, cx| {
            input.set_value(playlist.name, cx);
        });
        self.playlist_desc_input.update(cx, |input, cx| {
            input.set_value(playlist.description.unwrap_or_default(), cx);
        });
        self.show_playlist_form(window, cx);
    }

    pub fn confirm_delete_playlist(
        &mut self,
        playlist: PlaylistResponse,
        window: &mut Window,
        cx: &mut gpui::Context<Sidebar>,
    ) {
        let name = playlist.name.clone();
        let playlist_id = playlist.id;
        let sidebar = cx.entity();
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
                            "Delete “{name}”? The playlist will be removed, but songs stay in your library."
                        ))
                })
                .title_text("Delete Playlist")
                .ok_label("Delete")
                .cancel_label("Cancel")
                .on_ok(move |_, cx| {
                    sidebar.update(cx, |sidebar, cx| {
                        sidebar.delete_playlist(playlist_id.clone(), cx);
                    });
                    true
                })
                .appearance(appearance)
                .w(px(420.)),
            ),
            window,
            cx,
        );
    }

    fn show_playlist_form(&mut self, window: &mut Window, cx: &mut gpui::Context<Sidebar>) {
        let input = self.playlist_input.clone();
        let desc = self.playlist_desc_input.clone();
        let input_for_focus = input.clone();
        let shell = cx.entity();
        let shell_for_content = shell.clone();
        let shell_for_footer = shell.clone();
        modal::show(
            theme::style_modal(
                Modal::new(move |_, app| {
                    let shell_for_cover = shell_for_content.clone();
                    let cover_path = shell_for_content.read(app).playlist_cover_path.clone();
                    let has_cover = !cover_path.is_empty();
                    div()
                        .w_full()
                        .h(px(600.))
                        .px(px(26.))
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
                                .child(
                                    if shell_for_content.read(app).editing_playlist_id.is_some() {
                                        "Edit Playlist"
                                    } else {
                                        "New Playlist"
                                    },
                                ),
                        )
                        .child(
                            div()
                                .mt(px(42.))
                                .flex()
                                .flex_col()
                                .items_center()
                                .child(
                                    div()
                                        .id("playlist-cover-picker")
                                        .border_4()
                                        .rounded(px(14.))
                                        .border_color(rgba(0xfa3558ff))
                                        .size(px(220.))
                                        .overflow_hidden()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|cover| cover.bg(rgba(0xffffff08)))
                                        .on_click(move |_, _, app| {
                                            shell_for_cover.update(app, |this, cx| {
                                                this.select_playlist_cover(cx);
                                            });
                                        })
                                        .when(has_cover, |cover| {
                                            cover.child(
                                                img(PathBuf::from(cover_path))
                                                    .size_full()
                                                    .object_fit(ObjectFit::Cover),
                                            )
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
                                    let remove_shell = shell_for_content.clone();
                                    cover.child(
                                        div()
                                            .id("playlist-cover-remove")
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
                                                remove_shell.update(app, |sidebar, cx| {
                                                    sidebar.clear_playlist_cover(cx);
                                                });
                                            })
                                            .child("Remove Cover"),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .mt(px(48.))
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(theme::style_field(
                                    Input::new(&input).appearance(input_appearance()),
                                ))
                                .child(
                                    theme::style_field(
                                        Input::new(&desc)
                                            .appearance(description_input_appearance())
                                            .rows(3),
                                    )
                                    .bg(rgba(0x282925e8))
                                    .h(px(88.)),
                                ),
                        )
                })
                .footer(move |_, app| {
                    let create_shell = shell_for_footer.clone();
                    let cancel_shell = shell_for_footer.clone();
                    let editing = shell_for_footer.read(app).editing_playlist_id.is_some();
                    let create_disabled = {
                        let sidebar = shell_for_footer.read(app);
                        sidebar.playlist_name.trim().is_empty()
                            || sidebar.tasks.contains_key("playlists_new")
                            || sidebar.tasks.contains_key("playlists_save")
                    };
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
                                .id("playlist-create-cancel")
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
                                .on_click(move |_, window, app| {
                                    cancel_shell.update(app, |sidebar, cx| {
                                        sidebar.reset_playlist_form(cx);
                                    });
                                    modal::dismiss(window, app);
                                })
                                .child("Cancel"),
                        )
                        .child(
                            div()
                                .id("playlist-create-submit")
                                .w(px(100.))
                                .h(px(40.))
                                .rounded_full()
                                .bg(if create_disabled {
                                    rgba(0x292d39ff)
                                } else {
                                    rgba(0xfa3558ff)
                                })
                                .text_color(if create_disabled {
                                    rgba(0x8c8f9b9c)
                                } else {
                                    rgba(0xffffffff)
                                })
                                .text_size(px(15.))
                                .font_weight(FontWeight::MEDIUM)
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(!create_disabled, |button| {
                                    button
                                        .cursor_pointer()
                                        .hover(|button| button.bg(rgba(0xff496aff)))
                                        .on_click(move |_, window, app| {
                                            create_shell.update(app, |sidebar, cx| {
                                                sidebar.submit_playlist_form(cx);
                                            });
                                            modal::dismiss(window, app);
                                        })
                                })
                                .child(if editing { "Save" } else { "Create" }),
                        )
                }),
            )
            .w(px(460.))
            .max_w(relative(0.92))
            .max_h(relative(0.94))
            .rounded(px(34.))
            .border(px(0.))
            .ok_on_enter(false),
            window,
            cx,
        );
        input_for_focus.focus_handle(cx).focus(window, cx);
    }

    fn reset_playlist_form(&mut self, cx: &mut gpui::Context<Sidebar>) {
        self.playlist_name = SharedString::from("");
        self.playlist_desc = SharedString::from("");
        self.playlist_cover_path.clear();
        self.playlist_cover_dirty = false;
        self.editing_playlist_id = None;
        self.playlist_input.update(cx, |input, cx| {
            input.clear(cx);
            cx.notify();
        });
        self.playlist_desc_input.update(cx, |input, cx| {
            input.clear(cx);
            cx.notify();
        });
        cx.notify();
    }

    pub fn submit_playlist_form(&mut self, cx: &mut gpui::Context<Sidebar>) {
        if self.editing_playlist_id.is_some() {
            self.save_playlist(cx);
        } else {
            self.add_playlist(cx);
        }
    }

    pub fn add_playlist(&mut self, cx: &gpui::Context<Sidebar>) {
        let Some(playlists_api) = self.playlists_api(cx) else {
            return;
        };
        let name = self.playlist_name.clone();
        let desc = self.playlist_desc.clone();
        let cover_path = (!self.playlist_cover_path.is_empty())
            .then(|| PathBuf::from(&self.playlist_cover_path));

        let request = Tokio::spawn(cx, async move {
            let playlist = playlists_api
                .create(&PlaylistWriteRequest {
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                })
                .await?;
            let cover_error = if let Some(path) = cover_path {
                match tokio::fs::read(&path).await {
                    Ok(bytes) => {
                        let filename = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("playlist-cover")
                            .to_owned();
                        playlists_api
                            .update_cover(&playlist.id, filename, image_content_type(&path), bytes)
                            .await
                            .err()
                    }
                    Err(error) => Some(crate::services::api::ApiError::InvalidResponse(format!(
                        "failed to read playlist cover: {error}"
                    ))),
                }
            } else {
                None
            };
            Ok::<_, crate::services::api::ApiError>((playlist, cover_error))
        });

        self.tasks.insert(
            "playlists_new".to_string(),
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };

                this.update(cx, |this, cx| {
                    this.tasks.remove("playlists_new");
                    if let Ok(Ok((_playlist, cover_error))) = result {
                        if let Some(error) = cover_error {
                            log::warn!(
                                "playlist created but its cover could not be uploaded: {error}"
                            );
                        }
                        this.reset_playlist_form(cx);
                        this.refresh(cx);
                        cx.notify();
                    }
                });
            }),
        );
    }

    fn save_playlist(&mut self, cx: &mut gpui::Context<Sidebar>) {
        let Some(playlist_id) = self.editing_playlist_id.clone() else {
            return;
        };
        let Some(playlists_api) = self.playlists_api(cx) else {
            toast::error("The playlist service is unavailable", cx);
            return;
        };
        let name = self.playlist_name.clone();
        let desc = self.playlist_desc.clone();
        let cover_path = self
            .playlist_cover_dirty
            .then(|| PathBuf::from(&self.playlist_cover_path))
            .filter(|path| path.is_file());
        let remove_cover = self.playlist_cover_dirty
            && cover_path.is_none()
            && self
                .playlists
                .read(cx)
                .playlists()
                .iter()
                .any(|playlist| playlist.id == playlist_id && playlist.cover_url.is_some());

        let request = Tokio::spawn(cx, async move {
            let playlist = playlists_api
                .replace(
                    &playlist_id,
                    &PlaylistWriteRequest {
                        name: name.to_string(),
                        description: Some(desc.to_string()),
                    },
                )
                .await?;
            let cover_error = if let Some(path) = cover_path {
                match tokio::fs::read(&path).await {
                    Ok(bytes) => {
                        let filename = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("playlist-cover")
                            .to_owned();
                        playlists_api
                            .update_cover(&playlist.id, filename, image_content_type(&path), bytes)
                            .await
                            .err()
                    }
                    Err(error) => Some(crate::services::api::ApiError::InvalidResponse(format!(
                        "failed to read playlist cover: {error}"
                    ))),
                }
            } else if remove_cover {
                playlists_api.delete_cover(&playlist.id).await.err()
            } else {
                None
            };
            Ok::<_, crate::services::api::ApiError>((playlist, cover_error))
        });

        self.tasks.insert(
            "playlists_save".to_string(),
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |this, cx| {
                    this.tasks.remove("playlists_save");
                    match result {
                        Ok(Ok((_playlist, cover_error))) => {
                            if let Some(error) = cover_error {
                                toast::error(error.user_message(), cx);
                            } else {
                                toast::success("Playlist updated", cx);
                            }
                            this.reset_playlist_form(cx);
                            this.refresh(cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Update playlist failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    pub fn delete_playlist(&mut self, playlist_id: String, cx: &mut gpui::Context<Sidebar>) {
        let Some(playlists_api) = self.playlists_api(cx) else {
            toast::error("The playlist service is unavailable", cx);
            return;
        };
        let viewing = matches!(
            &self.selected_page,
            Page::PlayLists(crate::domain::PlayList::Custom(id)) if id.as_ref() == playlist_id
        );
        let request_id = playlist_id.clone();
        let request = Tokio::spawn(cx, async move { playlists_api.delete(&request_id).await });
        let task_key = format!("playlists_delete_{playlist_id}");
        self.tasks.insert(
            task_key.clone(),
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |this, cx| {
                    this.tasks.remove(&task_key);
                    match result {
                        Ok(Ok(())) => {
                            toast::success("Playlist deleted", cx);
                            if viewing {
                                this.selected_page = Page::PlayLists(crate::domain::PlayList::All);
                                cx.emit(SidebarEvent::Select(this.selected_page.clone()));
                            }
                            this.refresh(cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Delete playlist failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    pub fn refresh(&mut self, cx: &mut gpui::Context<Sidebar>) {
        self.playlists
            .update(cx, |playlists, cx| playlists.refresh(cx));
    }

    #[cfg(any())]
    fn sync_playlist_covers(&mut self, cx: &mut gpui::Context<Sidebar>) {
        let targets = self
            .playlists_list
            .iter()
            .filter_map(|playlist| {
                playlist
                    .cover_url
                    .as_ref()
                    .map(|url| (playlist.id.clone(), url.clone()))
            })
            .collect::<Vec<_>>();
        let active = targets
            .iter()
            .map(|(playlist_id, _)| playlist_id.as_str())
            .collect::<HashSet<_>>();
        self.playlist_covers
            .retain(|playlist_id, _| active.contains(playlist_id.as_str()));
        self.playlist_cover_urls
            .retain(|playlist_id, _| active.contains(playlist_id.as_str()));

        let Some(playlists_api) = self.playlists_api(cx) else {
            return;
        };
        for (playlist_id, cover_url) in targets {
            let task_key = format!("playlist_cover_{playlist_id}");
            if self.tasks.contains_key(&task_key) {
                continue;
            }
            if self.playlist_cover_urls.get(&playlist_id) == Some(&cover_url)
                && self
                    .playlist_covers
                    .get(&playlist_id)
                    .is_some_and(|path| path.is_file())
            {
                continue;
            }
            let Some(cache_path) = playlist_cover_cache_path(&playlist_id, &cover_url) else {
                continue;
            };
            let api = playlists_api.clone();
            let request_url = cover_url.clone();
            let request_path = cache_path.clone();
            let request = Tokio::spawn(cx, async move {
                let (_, bytes) = api
                    .cover(&request_url)
                    .await
                    .map_err(|error| error.to_string())?;
                let parent = request_path
                    .parent()
                    .ok_or_else(|| "playlist cover cache path has no parent".to_owned())?;
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| error.to_string())?;
                tokio::fs::write(&request_path, bytes)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(request_path)
            });
            let task_key_for_future = task_key.clone();
            let playlist_id_for_future = playlist_id.clone();
            let cover_url_for_future = cover_url.clone();
            self.playlist_cover_urls
                .insert(playlist_id.clone(), cover_url);
            self.tasks.insert(
                task_key,
                cx.spawn(async move |this, cx| {
                    let result = request.await;
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |this, cx| {
                        this.tasks.remove(&task_key_for_future);
                        match result {
                            Ok(Ok(path))
                                if this.playlist_cover_urls.get(&playlist_id_for_future)
                                    == Some(&cover_url_for_future) =>
                            {
                                this.playlist_covers.insert(playlist_id_for_future, path);
                                cx.notify();
                            }
                            Ok(Err(error)) => {
                                log::warn!("failed to synchronize playlist cover: {error}");
                                if this.playlist_cover_urls.get(&playlist_id_for_future)
                                    == Some(&cover_url_for_future)
                                {
                                    this.playlist_cover_urls.remove(&playlist_id_for_future);
                                }
                            }
                            _ => {}
                        }
                    });
                }),
            );
        }
    }
}

pub enum SidebarEvent {
    Select(Page),
}

impl gpui::EventEmitter<SidebarEvent> for Sidebar {}

impl gpui::Render for Sidebar {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = &cx.global::<LoadedClientConfig>().config;
        let playlists = self
            .playlists
            .read(cx)
            .playlists()
            .iter()
            .map(|playlist| {
                (
                    playlist.clone(),
                    self.playlists.read(cx).cover(&playlist.id),
                )
            })
            .collect::<Vec<_>>();
        theme::frosted()
            .appearance(FrostedGlassAppearance {
                blur_radius: px(20.),
                tint: rgba(0x17102f90).into(),
                edge: rgba(0x7656d48f).into(),
                ..theme::frosted_appearance()
            })
            .rounded(px(25.))
            .w_full()
            .flex_1()
            .child(
                div()
                    .size_full()
                    .when_some(config.fonts.font_family.clone(), |this, val| {
                        this.font_family(val)
                    })
                    .flex()
                    .flex_col()
                    .when(cfg!(target_os = "macos"), |sidebar| {
                        sidebar.child(
                            crate::ui::components::window_controls::drag_region()
                                .h(px(41.))
                                .px(px(15.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .window_control_area(WindowControlArea::Drag),
                        )
                    })
                    .child(div().mt_4())
                    .child(nav_item(
                        "nav-search",
                        "Search",
                        LucideIcons::Search,
                        Page::Search,
                        self.selected_page == Page::Search,
                        cx,
                    ))
                    .child(nav_item(
                        "nav-home",
                        "Home",
                        LucideIcons::House,
                        Page::Home,
                        self.selected_page == Page::Home,
                        cx,
                    ))
                    .child(nav_section("Library"))
                    .child(nav_item(
                        "nav-artists",
                        "Artists",
                        LucideIcons::MicVocal,
                        Page::Artists,
                        matches!(self.selected_page, Page::Artists | Page::Artist(_)),
                        cx,
                    ))
                    .child(nav_item(
                        "nav-albums",
                        "Albums",
                        LucideIcons::DiscAlbum,
                        Page::Albums,
                        matches!(self.selected_page, Page::Albums | Page::Album(_)),
                        cx,
                    ))
                    .child(nav_item(
                        "nav-songs",
                        "Songs",
                        LucideIcons::Music2,
                        Page::Songs,
                        self.selected_page == Page::Songs,
                        cx,
                    ))
                    .child(
                        nav_section("Playlists").child(
                            div()
                                .ml_auto()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    svg()
                                        .path(LucideIcons::Plus)
                                        .id("add-playlists")
                                        .on_click(cx.listener(
                                            |sidebar, event: &ClickEvent, window, cx| {
                                                if let ClickEvent::Mouse(click) = event
                                                    && click.down.button == MouseButton::Left
                                                {
                                                    sidebar.open_playlist_modal(window, cx);
                                                }
                                            },
                                        ))
                                        .invisible()
                                        .group_hover("Playlists-group", |this| this.visible())
                                        .size_4()
                                        .text_color(rgba(0xa5a7b5a8)),
                                )
                                .child(
                                    div()
                                        .id("toggle-playlists")
                                        .size(px(20.))
                                        .rounded(px(5.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .invisible()
                                        .group_hover("Playlists-group", |this| this.visible())
                                        // .hover(|button| button.bg(rgba(0xffffff0d)))
                                        .on_click(cx.listener(
                                            |sidebar, event: &ClickEvent, _, cx| {
                                                if let ClickEvent::Mouse(click) = event
                                                    && click.down.button == MouseButton::Left
                                                {
                                                    sidebar.playlists_expanded =
                                                        !sidebar.playlists_expanded;
                                                    cx.notify();
                                                }
                                            },
                                        ))
                                        .child(
                                            svg()
                                                .path(if self.playlists_expanded {
                                                    LucideIcons::ChevronDown
                                                } else {
                                                    LucideIcons::ChevronRight
                                                })
                                                .size(px(14.))
                                                .text_color(rgba(0xa5a7b5c8)),
                                        ),
                                ),
                        ),
                    )
                    .when(self.playlists_expanded, |sidebar| {
                        sidebar
                            .child(nav_item(
                                "all-playlist",
                                "All Playlists",
                                LucideIcons::LayoutGrid,
                                Page::PlayLists(crate::domain::PlayList::All),
                                self.selected_page == Page::PlayLists(crate::domain::PlayList::All),
                                cx,
                            ))
                            .child(nav_item(
                                "playlist-like",
                                "Like",
                                LucideIcons::Heart,
                                Page::PlayLists(crate::domain::PlayList::Like),
                                self.selected_page
                                    == Page::PlayLists(crate::domain::PlayList::Like),
                                cx,
                            ))
                            .children(playlists.into_iter().map(|(item, cover)| {
                                let selected = self.selected_page
                                    == Page::PlayLists(crate::domain::PlayList::Custom(
                                        SharedString::from(&item.id),
                                    ));
                                playlist_nav_item(item, cover, selected, cx)
                            }))
                    })
                    .child(div().flex_1()),
            )
    }
}

pub fn playlist_manage_menu(sidebar: Entity<Sidebar>, playlist: PlaylistResponse) -> ContextMenu {
    let edit_sidebar = sidebar.clone();
    let edit_playlist = playlist.clone();
    let delete_sidebar = sidebar;
    let delete_playlist = playlist;
    theme::style_context_menu(
        ContextMenu::new()
            .action_with(
                |_, _| playlist_menu_label(LucideIcons::Pencil, "Edit Playlist"),
                move |window, cx| {
                    edit_sidebar.update(cx, |sidebar, cx| {
                        sidebar.open_edit_playlist(edit_playlist.clone(), window, cx);
                    });
                },
            )
            .separator()
            .item(
                ContextMenuItem::action_with(
                    |_, _| playlist_menu_label(LucideIcons::Trash2, "Delete Playlist"),
                    move |window, cx| {
                        delete_sidebar.update(cx, |sidebar, cx| {
                            sidebar.confirm_delete_playlist(delete_playlist.clone(), window, cx);
                        });
                    },
                )
                .danger(),
            ),
    )
}

fn playlist_menu_label(icon: LucideIcons, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xf2f0f6f2)))
        .child(label)
}

fn playlist_nav_item(
    playlist: PlaylistResponse,
    cover: Option<PathBuf>,
    selected: bool,
    cx: &gpui::Context<Sidebar>,
) -> impl IntoElement {
    let page = Page::PlayLists(crate::domain::PlayList::Custom(SharedString::from(
        &playlist.id,
    )));
    let sidebar = cx.entity();
    let menu_playlist = playlist.clone();
    let label = playlist.name.clone();
    div()
        .id(playlist.id.clone())
        .mx(px(9.))
        .h(px(35.))
        .px(px(9.))
        .flex()
        .pl_7()
        .items_center()
        .gap(px(8.))
        .rounded(px(12.))
        .cursor_pointer()
        .text_size(px(15.))
        .font_weight(FontWeight::BOLD)
        .text_color(if selected {
            rgba(0xff3860ff)
        } else {
            rgba(0xf0f0f5dc)
        })
        .when(selected, |row| row.bg(rgba(0x3a3b4a8c)))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.selected_page = page.clone();
            cx.emit(SidebarEvent::Select(page.clone()));
            cx.notify();
        }))
        .context_menu(move |_, _| playlist_manage_menu(sidebar.clone(), menu_playlist.clone()))
        .child(playlist_cover_art(&label, cover))
        .child(div().min_w_0().truncate().child(label))
}

fn playlist_cover_art(name: &str, cover: Option<PathBuf>) -> impl IntoElement {
    media_cover::media_cover(name, cover.map(CoverSource::Path), px(16.), px(3.), false)
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

#[cfg(any())]
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
    let version = if safe_version.is_empty() {
        "current"
    } else {
        &safe_version
    };
    Some(
        dirs::cache_dir()?
            .join("romus/playlist-covers")
            .join(format!("{safe_id}-{version}.image")),
    )
}

fn nav_section(label: &'static str) -> Stateful<Div> {
    div()
        .id(label)
        .group(format!("{label}-group"))
        .h(px(28.))
        .px(px(19.))
        .pt(px(10.))
        .flex()
        .items_center()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(0xa5a7b5a8))
        .child(label)
}

fn nav_item(
    id: &str,
    label: &str,
    icon: LucideIcons,
    page: Page,
    selected: bool,
    cx: &gpui::Context<Sidebar>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(id))
        .mx(px(9.))
        .h(px(35.))
        .px(px(9.))
        .flex()
        .pl_7()
        .items_center()
        .gap(px(8.))
        .rounded(px(12.))
        .cursor_pointer()
        .text_size(px(15.))
        .font_weight(FontWeight::BOLD)
        .text_color(if selected {
            rgba(0xff3860ff)
        } else {
            rgba(0xf0f0f5dc)
        })
        .when(selected, |row| row.bg(rgba(0x3a3b4a8c)))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.selected_page = page.clone();
            cx.emit(SidebarEvent::Select(page.clone()));
            cx.notify();
        }))
        .child(svg().path(icon).size(px(16.)).text_color(if selected {
            rgba(0xff3860ff)
        } else {
            rgba(0xf0f0f5dc)
        }))
        .child(SharedString::from(label))
}
