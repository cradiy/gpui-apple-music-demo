use std::time::Duration;

use gpui::*;

use crate::utils::format_time;

pub fn timeline_tooltip(seconds: f64) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgba(0x020617f2))
        .shadow_lg()
        .text_xs()
        .text_color(rgba(0xffffffe5))
        .child(format_time(Duration::from_secs_f64(seconds.max(0.))))
}
