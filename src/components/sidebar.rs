use std::time::Duration;

use gpui::{
    FontWeight, WindowControlArea, div, linear_color_stop, linear_gradient, prelude::*, px, rgb,
    rgba, svg,
};
use gpui_effects::{GlassMaterial, GlassPanel};
use uic::assets::LucideIcons;

use crate::{app::AppleMusicApp, state::Page, theme::layout};

fn traffic_light(color: u32) -> impl IntoElement {
    div()
        .size(px(12.))
        .rounded_full()
        .bg(rgb(color))
        .border_1()
        .border_color(rgba(0x0000002b))
}

fn nav_section(label: &'static str) -> impl IntoElement {
    div()
        .h(px(28.))
        .px(px(19.))
        .pt(px(10.))
        .flex()
        .items_center()
        .text_size(px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(0xa5a7b5a8))
        .child(label)
}

fn nav_item(
    id: &'static str,
    label: &'static str,
    icon: LucideIcons,
    page: Page,
    selected: bool,
    cx: &mut gpui::Context<AppleMusicApp>,
) -> impl IntoElement {
    div()
        .id(id)
        .mx(px(9.))
        .h(px(35.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(15.))
        .font_weight(if selected {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .text_color(if selected {
            rgba(0xfff8f9f2)
        } else {
            rgba(0xe8e8eee0)
        })
        .when(selected, |row| row.bg(rgba(0x3a3b4a8c)))
        .when(!selected, |row| {
            row.hover(|style| style.bg(rgba(0xffffff10)))
        })
        .on_click(cx.listener(move |app, _, _, cx| {
            app.navigation.page = page;
            cx.notify();
        }))
        .child(svg().path(icon).size(px(16.)).text_color(if selected {
            rgba(0xff3860ff)
        } else {
            rgba(0xf0f0f5dc)
        }))
        .child(label)
}

fn playlist_art(colors: (u32, u32), icon: Option<LucideIcons>) -> impl IntoElement {
    div()
        .relative()
        .size(px(16.))
        .flex_none()
        .overflow_hidden()
        .rounded(px(3.))
        .bg(linear_gradient(
            135.,
            linear_color_stop(rgb(colors.0), 0.),
            linear_color_stop(rgb(colors.1), 1.),
        ))
        .when_some(icon, |art, icon| {
            art.child(
                svg()
                    .absolute()
                    .inset(px(3.))
                    .path(icon)
                    .text_color(rgba(0xffffffff)),
            )
        })
}

fn playlist_item(
    id: &'static str,
    label: &'static str,
    colors: (u32, u32),
    icon: Option<LucideIcons>,
) -> impl IntoElement {
    div()
        .id(id)
        .mx(px(9.))
        .h(px(35.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(15.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(0xe9e9efdc))
        .hover(|style| style.bg(rgba(0xffffff10)))
        .child(playlist_art(colors, icon))
        .child(div().min_w_0().truncate().child(label))
}

pub fn render(app: &mut AppleMusicApp, cx: &mut gpui::Context<AppleMusicApp>) -> impl IntoElement {
    GlassPanel::new()
        .material(GlassMaterial::Regular)
        .animation("apple-music-sidebar-glass", Duration::from_secs(8))
        .radius(px(25.))
        .w(px(layout::SIDEBAR_WIDTH + layout::SIDEBAR_GUTTER))
        .flex_1()
        .tint(rgba(0x17102f78))
        .border_color(rgba(0x7656d48f))
        .optics([15.0, 1.8, 0.54, 1.12])
        .surface([0.2, 0.54, 0.78, 0.82])
        .shader_tint([0.055, 0.035, 0.16, 0.46])
        .deformation(0.32)
        .wave_strength(0.72)
        .child(
            div()
                .h(px(41.))
                .px(px(15.))
                .flex()
                .items_center()
                .gap(px(8.))
                .window_control_area(WindowControlArea::Drag)
                .child(traffic_light(0xff5f57))
                .child(traffic_light(0xffbd2e))
                .child(traffic_light(0x28c840)),
        )
        .child(nav_item(
            "nav-search",
            "Search",
            LucideIcons::Search,
            Page::Search,
            app.navigation.page == Page::Search,
            cx,
        ))
        .child(nav_item(
            "nav-home",
            "Home",
            LucideIcons::House,
            Page::Home,
            app.navigation.page == Page::Home,
            cx,
        ))
        .child(nav_section("Library"))
        .child(nav_item(
            "nav-recent",
            "Recently Added",
            LucideIcons::Clock3,
            Page::RecentlyAdded,
            app.navigation.page == Page::RecentlyAdded,
            cx,
        ))
        .child(nav_item(
            "nav-artists",
            "Artists",
            LucideIcons::MicVocal,
            Page::Artists,
            app.navigation.page == Page::Artists,
            cx,
        ))
        .child(nav_item(
            "nav-albums",
            "Albums",
            LucideIcons::DiscAlbum,
            Page::Albums,
            app.navigation.page == Page::Albums,
            cx,
        ))
        .child(nav_item(
            "nav-songs",
            "Songs",
            LucideIcons::Music2,
            Page::Songs,
            app.navigation.page == Page::Songs,
            cx,
        ))
        .child(nav_section("Playlists"))
        .child(playlist_item(
            "all-playlist",
            "All Playlists",
            (0xf6f7fb, 0xadafba),
            Some(LucideIcons::LayoutGrid),
        ))
        .child(playlist_item(
            "playlist-like",
            "like",
            (0x403a73, 0xd8a8c0),
            None,
        ))
        .child(playlist_item(
            "playlist-jp-1",
            "Japanese - Anime · Space",
            (0x363fc5, 0xd66b91),
            None,
        ))
        .child(playlist_item(
            "playlist-jp-2",
            "Japanese - Anime · Space (...",
            (0x37b9d6, 0x234c91),
            None,
        ))
        .child(playlist_item(
            "playlist-classic",
            "Classic Rock Selection",
            (0xd7c3b0, 0x5b315c),
            None,
        ))
        .child(playlist_item(
            "playlist-cantonese",
            "Cantonese",
            (0xeb817b, 0x26233f),
            None,
        ))
}
