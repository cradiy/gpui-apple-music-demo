use gpui::{FontWeight, div, prelude::*, px, rgba, svg};
use uic::assets::LucideIcons;

use crate::state::Page;

pub fn render(page: Page) -> impl IntoElement {
    let (title, subtitle, icon) = match page {
        Page::Search => (
            "Searching Your Library",
            "Type an artist, album, or song in the search field.",
            LucideIcons::Search,
        ),
        Page::Home => (
            "Listen Now",
            "Your personal music recommendations will appear here.",
            LucideIcons::House,
        ),
        Page::RecentlyAdded => (
            "Recently Added",
            "Albums you add to your library will appear here.",
            LucideIcons::Clock3,
        ),
        Page::Artists => (
            "Artists",
            "Artists in your music library will appear here.",
            LucideIcons::MicVocal,
        ),
        Page::Albums => (
            "Albums",
            "Albums in your music library will appear here.",
            LucideIcons::DiscAlbum,
        ),
        Page::Songs => (
            "Songs",
            "Songs in your music library will appear here.",
            LucideIcons::Music2,
        ),
    };

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .mt(px(-28.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(10.))
                .child(svg().path(icon).size(px(48.)).text_color(rgba(0xd6d6dc72)))
                .child(
                    div()
                        .text_size(px(21.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(0xd8d8deaa))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgba(0xb5b6c071))
                        .child(subtitle),
                ),
        )
}
