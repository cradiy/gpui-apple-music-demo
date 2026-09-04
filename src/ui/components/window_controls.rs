#[cfg(target_os = "linux")]
use gpui::MouseButton;
use gpui::{AnyElement, App, Div, Window, WindowControlArea, div, prelude::*, px, rgb, rgba, svg};
use uic::assets::LucideIcons;

use crate::{
    resources::RomusIcons,
    services::config::{LinuxTitlebar, LoadedClientConfig},
};

pub const HEIGHT: f32 = 56.;

pub fn visible(cx: &App) -> bool {
    cfg!(target_os = "windows")
        || (cfg!(target_os = "linux")
            && cx.global::<LoadedClientConfig>().config.linux.titlebar == LinuxTitlebar::Inline)
}

pub fn width() -> f32 {
    if cfg!(target_os = "windows") {
        138.
    } else {
        104.
    }
}

pub fn below_titlebar(panel: impl IntoElement, enabled: bool) -> AnyElement {
    if !enabled {
        return panel.into_any_element();
    }
    div()
        .relative()
        .h_full()
        .flex_none()
        .pt(px(HEIGHT))
        .bg(rgb(0x2c2d32))
        .child(
            drag_region()
                .absolute()
                .top_0()
                .left_0()
                .right(px(width()))
                .h(px(HEIGHT)),
        )
        .child(panel)
        .into_any_element()
}

/// Windows delegates caption gestures to native hit testing. Linux needs explicit handlers.
pub fn drag_region() -> Div {
    let region = div().window_control_area(WindowControlArea::Drag);
    #[cfg(target_os = "linux")]
    let region = region
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
        .on_mouse_down(MouseButton::Right, |event, window, _| {
            window.show_window_menu(event.position);
        });
    region
}

pub fn render(window: &Window) -> Div {
    let maximized = window.is_maximized();
    div()
        .h(px(HEIGHT))
        .w(px(width()))
        .flex_none()
        .flex()
        .when(cfg!(target_os = "linux"), |bar| {
            bar.items_center().gap(px(6.)).px(px(10.))
        })
        .children(
            [
                (
                    "window-minimize",
                    WindowControlArea::Min,
                    LucideIcons::Minus.into(),
                ),
                (
                    "window-maximize",
                    WindowControlArea::Max,
                    if maximized {
                        RomusIcons::WindowRestore.into()
                    } else {
                        RomusIcons::WindowMaximize.into()
                    },
                ),
                (
                    "window-close",
                    WindowControlArea::Close,
                    LucideIcons::X.into(),
                ),
            ]
            .map(|(id, area, icon): (_, _, gpui::SharedString)| {
                let button = div()
                    .id(id)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgba(0xe8e6eed9))
                    .when(cfg!(target_os = "windows"), |button| {
                        button
                            .w(px(46.))
                            .h(px(40.))
                            .window_control_area(area)
                            .hover(move |style| {
                                style
                                    .bg(if area == WindowControlArea::Close {
                                        rgba(0xc42b1cff)
                                    } else {
                                        rgba(0xffffff18)
                                    })
                                    .text_color(rgba(0xffffffff))
                            })
                    })
                    .when(cfg!(target_os = "linux"), |button| {
                        button
                            .size(px(24.))
                            .rounded_full()
                            .bg(rgba(0xffffff0e))
                            .hover(|style| style.bg(rgba(0xffffff22)).text_color(rgba(0xffffffff)))
                            .active(|style| style.bg(rgba(0xffffff30)))
                    });
                // Do not install click handlers on Windows: its native caption dispatch owns these.
                #[cfg(not(target_os = "windows"))]
                let button = button.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    match area {
                        WindowControlArea::Min => window.minimize_window(),
                        WindowControlArea::Max => window.zoom_window(),
                        WindowControlArea::Close => window.remove_window(),
                        WindowControlArea::Drag => unreachable!(),
                    }
                });
                button.child(
                    svg()
                        .path(icon)
                        .size(px(if cfg!(target_os = "linux") { 12. } else { 14. }))
                        .text_color(rgba(0xe8e6eeed)),
                )
            }),
        )
}
