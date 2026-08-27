use gpui::{FontWeight, SharedString, WindowControlArea, div, prelude::*, px, rgba, svg};
use uic::{assets::LucideIcons, components::input::Input};

use crate::{
    app::music_shell::MusicShell,
    domain::{Page, PlayList},
    state::imports,
    ui::{strings, theme},
};

fn search_input(input: Input) -> Input {
    theme::style_search(input.appearance(theme::search_appearance()))
}

fn drag_region() -> impl IntoElement {
    div()
        .h_full()
        .flex_1()
        .window_control_area(WindowControlArea::Drag)
}

pub fn render(
    app: &mut MusicShell,
    compact: bool,
    cx: &mut gpui::Context<MusicShell>,
) -> impl IntoElement {
    let page = app.navigation.page.clone();
    let custom_playlist = matches!(&page, Page::PlayLists(PlayList::Custom(_)));
    let songs_summary = (page == Page::Songs).then(|| app.songs_summary(cx));
    div()
        .h(px(56.))
        .px(px(18.))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba(0xffffff08))
        .when(page == Page::Search, |top_bar| {
            top_bar
                .child(drag_region())
                .child(
                    div()
                        .w(if compact { px(310.) } else { px(412.) })
                        .h(px(35.))
                        .flex_none()
                        .rounded(px(16.))
                        .shadow(vec![
                            gpui::BoxShadow::new(px(0.), px(0.), rgba(0xe7325a42).into())
                                .blur_radius(px(8.)),
                        ])
                        .child(search_input(
                            Input::new(&app.search_input).prefix(
                                svg()
                                    .path(LucideIcons::Search)
                                    .size(px(14.))
                                    .text_color(rgba(0xd4d4dcc4)),
                            ),
                        )),
                )
                .child(drag_region())
        })
        .when(page == Page::Songs, |top_bar| {
            top_bar
                .child(
                    div()
                        .h_full()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .window_control_area(WindowControlArea::Drag)
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(0xf2f0f6ed))
                                .child("Songs"),
                        )
                        .child(
                            div()
                                .mt(px(1.))
                                .truncate()
                                .text_size(px(9.))
                                .text_color(rgba(0xb5b3c08f))
                                .child(songs_summary.unwrap_or_else(|| "Songs".to_owned())),
                        ),
                )
                .child(
                    div()
                        .id("songs-sort-button")
                        .size(px(35.))
                        .flex_none()
                        .rounded_full()
                        .border_1()
                        .border_color(rgba(0xffffff13))
                        .bg(rgba(0x12141ccf))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|button| button.bg(rgba(0xffffff10)))
                        .on_click(cx.listener(|app, _, _, cx| app.toggle_songs_sort(cx)))
                        .child(
                            svg()
                                .path(LucideIcons::ListFilter)
                                .size(px(15.))
                                .text_color(rgba(0xd3d1dab8)),
                        ),
                )
                .child(
                    div()
                        .ml(px(9.))
                        .w(if compact { px(180.) } else { px(238.) })
                        .h(px(35.))
                        .flex_none()
                        .child(search_input(
                            Input::new(&app.songs_search_input).prefix(
                                svg()
                                    .path(LucideIcons::Search)
                                    .size(px(14.))
                                    .text_color(rgba(0xd4d4dcc4)),
                            ),
                        )),
                )
        })
        .when(custom_playlist, |top_bar| {
            top_bar
                .child(drag_region())
                .child(
                    div()
                        .id("playlist-sort-button")
                        .size(px(35.))
                        .flex_none()
                        .rounded_full()
                        .border_1()
                        .border_color(rgba(0xffffff13))
                        .bg(rgba(0x12141ccf))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|button| button.bg(rgba(0xffffff10)))
                        .on_click(cx.listener(|app, _, _, cx| app.toggle_playlist_sort(cx)))
                        .child(
                            svg()
                                .path(LucideIcons::ListFilter)
                                .size(px(15.))
                                .text_color(rgba(0xd3d1dab8)),
                        ),
                )
                .child(
                    div()
                        .ml(px(9.))
                        .w(if compact { px(180.) } else { px(238.) })
                        .h(px(35.))
                        .flex_none()
                        .child(search_input(
                            Input::new(&app.playlist_search_input).prefix(
                                svg()
                                    .path(LucideIcons::Search)
                                    .size(px(14.))
                                    .text_color(rgba(0xd4d4dcc4)),
                            ),
                        )),
                )
        })
        .when(
            page != Page::Search && page != Page::Songs && !custom_playlist,
            |top_bar| {
                let title = page_title(&page);
                top_bar
                    .child(
                        div()
                            .h_full()
                            .flex_1()
                            .flex()
                            .items_center()
                            .window_control_area(WindowControlArea::Drag)
                            .text_size(px(18.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(0xf2f0f6ed))
                            .child(title),
                    )
                    .when(page == Page::Home, |top_bar| {
                        top_bar.child(
                            div()
                                .id("home-add-music")
                                .size(px(32.))
                                .flex_none()
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|button| button.bg(rgba(0xffffff12)))
                                .on_click(cx.listener(|app, _, window, cx| {
                                    imports::show_picker(app.imports.clone(), window, cx);
                                }))
                                .child(
                                    svg()
                                        .path(LucideIcons::CloudUpload)
                                        .size(px(18.))
                                        .text_color(rgba(0xe8e6eed4)),
                                ),
                        )
                    })
            },
        )
}

fn page_title(page: &Page) -> SharedString {
    match page {
        Page::Search => strings::search(),
        Page::Home => strings::home(),
        Page::RecentlyAdded => strings::recently_added(),
        Page::Artists => strings::artists(),
        Page::Artist(_) => strings::artist(),
        Page::Albums => strings::albums(),
        Page::Album(_) => strings::album(),
        Page::Songs => strings::songs(),
        Page::PlayLists(PlayList::All) => strings::all_playlists(),
        Page::PlayLists(PlayList::Like) => strings::like(),
        Page::PlayLists(PlayList::Custom(_)) => strings::playlist(),
    }
    .into()
}
