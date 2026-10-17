use std::{cell::Cell, rc::Rc};

use gpui::{CursorStyle, DragMoveEvent, Render, div, prelude::*, px, rgba};

use super::MusicShell;
use crate::ui::theme::layout;

const SIDEBAR_MIN: f32 = 200.;
const SIDEBAR_MAX: f32 = 380.;
const LYRICS_MIN: f32 = 280.;
const LYRICS_MAX: f32 = 600.;
const CONTENT_MIN: f32 = 400.;
// Sidebar's two 8 px margins, plus the shell's two 1 px borders.
const SHELL_INSET: f32 = 18.;

pub(super) struct PanelWidths {
    sidebar: f32,
    lyrics: f32,
}

impl Default for PanelWidths {
    fn default() -> Self {
        Self {
            sidebar: layout::SIDEBAR_WIDTH + layout::SIDEBAR_GUTTER,
            lyrics: layout::LYRICS_WIDTH,
        }
    }
}

impl PanelWidths {
    /// Fit the current window without losing the user's preferred widths.
    pub fn fitted(&self, window: f32, lyrics: bool, queue: bool) -> (f32, f32) {
        let available = window - SHELL_INSET - CONTENT_MIN;
        let right = if lyrics {
            self.lyrics
                .clamp(LYRICS_MIN, LYRICS_MAX)
                .min((available - SIDEBAR_MIN).max(LYRICS_MIN))
        } else if queue {
            layout::LYRICS_WIDTH
        } else {
            0.
        };
        let sidebar = self
            .sidebar
            .clamp(SIDEBAR_MIN, SIDEBAR_MAX)
            .min((available - right).max(SIDEBAR_MIN));
        (sidebar, right)
    }

    fn resize(&mut self, side: Side, requested: f32, window: f32, lyrics: bool, queue: bool) {
        let (sidebar, right) = self.fitted(window, lyrics, queue);
        let available = window - SHELL_INSET - CONTENT_MIN;
        match side {
            Side::Sidebar => {
                self.sidebar = requested.clamp(
                    SIDEBAR_MIN,
                    (available - right).clamp(SIDEBAR_MIN, SIDEBAR_MAX),
                )
            }
            Side::Lyrics => {
                self.lyrics = requested.clamp(
                    LYRICS_MIN,
                    (available - sidebar).clamp(LYRICS_MIN, LYRICS_MAX),
                )
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Sidebar,
    Lyrics,
}

struct ResizeDrag {
    side: Side,
    initial_width: f32,
    start_x: Rc<Cell<f32>>,
}

struct EmptyDrag;
impl Render for EmptyDrag {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size(px(0.))
    }
}

/// GPUI's drag capture keeps resizing even when the pointer leaves the thin
/// handle. The drag carries its original width so hitting a limit cannot drift.
pub(super) fn handle(
    side: Side,
    width: f32,
    lyrics: bool,
    queue: bool,
    cx: &gpui::Context<MusicShell>,
) -> impl IntoElement {
    let drag = ResizeDrag {
        side,
        initial_width: width,
        start_x: Rc::new(Cell::new(0.)),
    };
    div()
        .id(match side {
            Side::Sidebar => "sidebar-resize",
            Side::Lyrics => "lyrics-resize",
        })
        .absolute()
        .top(px(8.))
        .bottom(px(8.))
        .w(px(8.))
        .cursor(CursorStyle::ResizeLeftRight)
        .flex()
        .justify_center()
        .group("panel-resize")
        .child(
            div()
                .w(px(1.))
                .h_full()
                .bg(rgba(0xffffff00))
                .group_hover("panel-resize", |line| line.bg(rgba(0xffffff38))),
        )
        .when(matches!(side, Side::Sidebar), |handle| {
            handle.left(px(width + 16. - 4.))
        })
        .when(matches!(side, Side::Lyrics), |handle| {
            handle.right(px(width - 4.))
        })
        .on_drag(drag, |drag, _, window, cx| {
            drag.start_x.set(window.mouse_position().x.as_f32());
            cx.new(|_| EmptyDrag)
        })
        .on_drag_move::<ResizeDrag>(cx.listener(
            move |app, event: &DragMoveEvent<ResizeDrag>, window, cx| {
                let drag = event.drag(cx);
                if drag.side != side {
                    return;
                }
                let delta = event.event.position.x.as_f32() - drag.start_x.get();
                let requested = drag.initial_width
                    + match drag.side {
                        Side::Sidebar => delta,
                        Side::Lyrics => -delta,
                    };
                app.panel_widths.resize(
                    drag.side,
                    requested,
                    window.bounds().size.width.as_f32(),
                    lyrics,
                    queue,
                );
                cx.notify();
            },
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserve_center_and_restore_preferences_after_window_resize() {
        let widths = PanelWidths {
            sidebar: 380.,
            lyrics: 600.,
        };
        let (left, right) = widths.fitted(1080., true, false);
        assert!(left >= SIDEBAR_MIN && right >= LYRICS_MIN);
        assert!(1080. - SHELL_INSET - left - right >= CONTENT_MIN);
        assert_eq!(widths.fitted(1600., true, false), (380., 600.));
    }

    #[test]
    fn drag_limits_reserve_the_opposite_panel() {
        let mut widths = PanelWidths::default();
        widths.resize(Side::Sidebar, 9999., 1080., true, false);
        widths.resize(Side::Lyrics, 9999., 1080., true, false);
        let (left, right) = widths.fitted(1080., true, false);
        assert!(1080. - SHELL_INSET - left - right >= CONTENT_MIN);
        widths.resize(Side::Sidebar, -100., 1080., true, false);
        widths.resize(Side::Lyrics, -100., 1080., true, false);
        assert_eq!(widths.fitted(1080., true, false), (SIDEBAR_MIN, LYRICS_MIN));
        let (left, right) = widths.fitted(1080., false, true);
        assert_eq!(right, layout::LYRICS_WIDTH);
        assert!(1080. - SHELL_INSET - left - right >= CONTENT_MIN);
    }
}
