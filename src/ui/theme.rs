use gpui::{AnyElement, Hsla, IntoElement, ParentElement, Styled, px, rgba, transparent_black};
use gpui_effects::{FrostedGlass, FrostedGlassAppearance};
use uic::components::{
    context_menu::{ContextMenu, ContextMenuAppearance, ContextMenuSurfaceState},
    input::{Input, InputAppearance},
    modal::{Modal, ModalAppearance},
};

pub const ACCENT: u32 = 0xfa2d55;

/// Named colors for the current dark UI. Swap this struct later for a Theme global.
#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Hsla,
    pub surface: Hsla,
    pub surface_raised: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub border_subtle: Hsla,
    pub focus: Hsla,
    pub danger: Hsla,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            accent: rgba(0xfa2d55ff).into(),
            surface: rgba(0x1c202cff).into(),
            surface_raised: rgba(0x12141ccf).into(),
            text: rgba(0xf1f1f5ee).into(),
            text_muted: rgba(0xb2b3bd8c).into(),
            border_subtle: rgba(0xffffff1a).into(),
            focus: rgba(0xfa2d55cc).into(),
            danger: rgba(0xff6684ff).into(),
        }
    }
}

pub fn palette() -> Palette {
    Palette::dark()
}

pub mod layout {
    pub const SIDEBAR_WIDTH: f32 = 280.;
    pub const COMPACT_SIDEBAR_WIDTH: f32 = 80.;
    pub const SIDEBAR_GUTTER: f32 = 7.;
    pub const LYRICS_WIDTH: f32 = 360.;
    pub const HIDE_LYRICS_BELOW: f32 = 1080.;
    pub const COMPACT_TOP_BAR_BELOW: f32 = 980.;
    pub const MINI_PLAYER_WIDTH: f32 = 480.;
    pub const MINI_PLAYER_HEIGHT: f32 = 80.;
}

pub fn frosted_appearance() -> FrostedGlassAppearance {
    FrostedGlassAppearance {
        blur_radius: px(18.),
        saturation: 1.08,
        brightness: 0.90,
        tint: rgba(0x141722b0).into(),
        edge: rgba(0xffffff26).into(),
        edge_width: px(1.),
        sheen: 0.02,
        merge_distance: px(44.),
    }
}

pub fn frosted() -> FrostedGlass {
    FrostedGlass::with_appearance(frosted_appearance())
}

pub fn input_appearance() -> InputAppearance {
    let p = palette();
    InputAppearance {
        placeholder: p.text_muted,
        focus_border: p.focus,
        caret: p.accent,
        selection: rgba(0xfa2d5545).into(),
        caret_width: px(1.),
        caret_height: px(21.),
    }
}

pub fn search_appearance() -> InputAppearance {
    let p = palette();
    InputAppearance {
        placeholder: rgba(0xe2e2e7c2).into(),
        focus_border: p.focus,
        caret: p.accent,
        selection: rgba(0xec41683d).into(),
        caret_width: px(1.),
        caret_height: px(17.),
    }
}

pub fn style_field(input: Input) -> Input {
    let p = palette();
    input
        .h(px(44.))
        .rounded(px(12.))
        .border_1()
        .border_color(p.border_subtle)
        .px(px(12.))
        .gap(px(9.))
        .text_size(px(14.))
        .text_color(p.text)
        .bg(rgba(0x10121acf))
}

pub fn style_search(input: Input) -> Input {
    let p = palette();
    input
        .h(px(35.))
        .rounded(px(16.))
        .border_1()
        .border_color(p.border_subtle)
        .px(px(12.))
        .gap(px(8.))
        .text_size(px(13.))
        .text_color(p.text)
        .bg(p.surface_raised)
}

pub fn context_menu_appearance() -> ContextMenuAppearance {
    ContextMenuAppearance {
        muted_foreground: rgba(0xaaa7b58f).into(),
        danger_foreground: rgba(0xff6684ff).into(),
        selected_background: rgba(0xfa35583d).into(),
        selected_foreground: rgba(0xffffffff).into(),
        separator: rgba(0xffffff20).into(),
        item_height: px(39.),
        item_padding_x: px(12.),
        item_radius: px(10.),
        separator_margin: px(6.),
    }
}

pub fn style_context_menu(menu: ContextMenu) -> ContextMenu {
    menu.appearance(context_menu_appearance())
        .w(px(280.))
        .max_h(px(560.))
        .p(px(10.))
        .rounded(px(16.))
        .border(px(0.))
        .bg(transparent_black())
        .text_color(rgba(0xf2f0f6f2))
        .surface(frosted_menu_surface)
}

fn frosted_menu_surface(
    state: ContextMenuSurfaceState,
    content: AnyElement,
    _: &mut gpui::Window,
    _: &mut gpui::App,
) -> AnyElement {
    frosted()
        .id((
            "romus-menu-glass",
            state.session_id.saturating_mul(10) + state.depth as u64,
        ))
        .rounded(px(16.))
        .shadow_lg()
        .child(content)
        .into_any_element()
}

pub fn style_fullscreen_context_menu(menu: ContextMenu) -> ContextMenu {
    style_context_menu(menu)
        .appearance(ContextMenuAppearance {
            selected_background: rgba(0xffffff22).into(),
            separator: rgba(0xffffff12).into(),
            muted_foreground: rgba(0xffffff80).into(),
            ..context_menu_appearance()
        })
        .text_color(rgba(0xffffffe5))
        .surface(|state, content, _, _| {
            frosted()
                .appearance(FrostedGlassAppearance {
                    blur_radius: px(24.),
                    tint: rgba(0xffffff13).into(),
                    brightness: 1.,
                    saturation: 1.,
                    edge: rgba(0xffffff0a).into(),
                    sheen: 0.,
                    ..frosted_appearance()
                })
                .id((
                    "romus-fullscreen-menu-glass",
                    state.session_id.saturating_mul(10) + state.depth as u64,
                ))
                .rounded(px(16.))
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(6.), rgba(0x00000018).into())
                        .blur_radius(px(20.))
                        .spread_radius(px(-4.)),
                ])
                .child(content)
                .into_any_element()
        })
}

pub fn dark_modal_appearance() -> ModalAppearance {
    let defaults = ModalAppearance::default();
    defaults
        .backdrop(rgba(0x080a10a8).into())
        .section_borders(false)
        .section_border(rgba(0xffffff16).into())
        .ok_button(
            defaults
                .ok_button
                .background(rgba(0xfa3558ff).into())
                .foreground(rgba(0xffffffff).into())
                .border(rgba(0xfa3558ff).into())
                .hover_background(rgba(0xff496aff).into())
                .radius(px(10.)),
        )
        .cancel_button(
            defaults
                .cancel_button
                .background(rgba(0xffffff0c).into())
                .foreground(rgba(0xf4f4f7e8).into())
                .border(rgba(0xffffff18).into())
                .hover_background(rgba(0xffffff16).into())
                .radius(px(10.)),
        )
}

pub fn style_modal(modal: Modal) -> Modal {
    modal
        .appearance(dark_modal_appearance())
        .bg(rgba(0x1c202cff))
        .text_color(rgba(0xf4f4f7f2))
        .rounded(px(22.))
        .border_1()
        .border_color(rgba(0xffffff1f))
}
