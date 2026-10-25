use super::*;
use uic::components::modal::{Modal, ModalAppearance};

impl MobileApp {
    pub(super) fn show_pending_error(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if modal::is_open(cx) {
            return;
        }
        let Some(details) = self.error.take() else {
            return;
        };
        let retry = self.current.clone().filter(|_| {
            self.player
                .as_ref()
                .is_some_and(|player| matches!(player.read(cx).state(), PlaybackState::Error(_)))
        });
        let playback = retry.is_some();
        let scale = Layout::for_window(window).s.min(1.4);
        let view = cx.entity().downgrade();
        let dialog = Modal::new(move |_, _| {
            div()
                .flex()
                .flex_col()
                .gap(px(16. * scale))
                .child(div().text_size(px(15. * scale)).child(if playback {
                    "Playback stopped. Try playing this song again."
                } else {
                    "The action could not be completed. Please try again."
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6. * scale))
                        .text_color(rgb(SECONDARY))
                        .text_size(px(12. * scale))
                        .child("Details")
                        .child(
                            div()
                                .id("error-details")
                                .max_h(px(144. * scale))
                                .overflow_y_scroll()
                                .p(px(12. * scale))
                                .rounded(px(12. * scale))
                                .bg(black().opacity(0.15))
                                .child(details.clone()),
                        ),
                )
        })
        .title_text(if playback {
            "Unable to play song"
        } else {
            "Something went wrong"
        })
        .footer(move |_, _| {
            let retry_view = view.clone();
            let retry_id = retry.clone();
            let button = |id: &'static str, label: &'static str| {
                div()
                    .id(id)
                    .h(px(44. * scale))
                    .px(px(20. * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12. * scale))
                    .text_size(px(15. * scale))
                    .cursor_pointer()
                    .child(label)
            };
            div()
                .flex()
                .justify_end()
                .gap(px(10. * scale))
                .child(
                    button("dismiss-error", "Close")
                        .bg(white().opacity(0.08))
                        .on_click(|_, window, cx| modal::dismiss(window, cx)),
                )
                .when(retry_id.is_some(), |footer| {
                    footer.child(
                        button("retry-playback", "Try Again")
                            .bg(rgb(ACCENT))
                            .on_click(move |_, window, cx| {
                                modal::dismiss(window, cx);
                                if let Some(id) = &retry_id {
                                    let _ =
                                        retry_view.update(cx, |this, cx| this.play_track(id, cx));
                                }
                            }),
                    )
                })
        })
        .appearance(
            ModalAppearance::default()
                .section_borders(false)
                .backdrop(black().opacity(0.5)),
        )
        .w(px(344. * scale))
        .max_w(relative(0.9))
        .rounded(px(24. * scale))
        .border_color(white().opacity(0.1))
        .bg(rgb(SURFACE))
        .text_color(rgb(TEXT))
        .font_family("Noto Sans CJK SC");
        modal::show(dialog, window, cx);
        window.set_back_enabled(true);
        cx.notify();
    }
}
