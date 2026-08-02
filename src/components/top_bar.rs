use gpui::{MouseButton, div, prelude::*, px, rgba, svg};
use uic::{
    assets::LucideIcons,
    components::input::{Input, InputAppearance},
};

use crate::app::AppleMusicApp;

fn appearance() -> InputAppearance {
    InputAppearance {
        background: rgba(0x12141cbf).into(),
        foreground: rgba(0xe2e2e7e8).into(),
        placeholder: rgba(0xe2e2e7c2).into(),
        border: rgba(0xffffff16).into(),
        focus_border: rgba(0xec4168d9).into(),
        caret: rgba(0xf14367cf).into(),
        selection: rgba(0xec41683d).into(),
        caret_width: px(1.),
        caret_height: px(17.),
        height: px(35.),
        radius: px(16.),
        border_width: px(2.),
        padding_x: px(11.),
        gap: px(7.),
        font_size: px(10.5),
    }
}

fn drag_region() -> impl IntoElement {
    div()
        .h_full()
        .flex_1()
        .on_mouse_down(MouseButton::Left, |_, window, _| {
            window.start_window_move();
        })
}

pub fn render(
    app: &mut AppleMusicApp,
    compact: bool,
    _cx: &mut gpui::Context<AppleMusicApp>,
) -> impl IntoElement {
    div()
        .h(px(56.))
        .px(px(18.))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba(0xffffff08))
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
                .child(
                    Input::new(&app.search_input)
                        .appearance(appearance())
                        .prefix(
                            svg()
                                .path(LucideIcons::Search)
                                .size(px(14.))
                                .text_color(rgba(0xd4d4dcc4)),
                        ),
                ),
        )
        .child(drag_region())
}
