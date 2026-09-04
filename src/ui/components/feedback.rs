use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use gpui::{
    App, AppContext, Context, Entity, Global, Render, SharedString, Task, Window, div, prelude::*,
    px, relative, rgba,
};

use crate::ui::theme;

const HOLD: Duration = Duration::from_secs(3);
const FADE: Duration = Duration::from_millis(220);

#[derive(Clone, PartialEq)]
struct Message {
    text: SharedString,
    error: bool,
}

struct Notice {
    id: usize,
    message: Message,
    entered: Instant,
    leaving: Option<Instant>,
}

pub struct Feedback {
    follow_cover: bool,
    current: Option<Notice>,
    pending: VecDeque<Message>,
    next_id: usize,
    dismiss_task: Option<Task<()>>,
}

impl Feedback {
    fn push(&mut self, message: Message, cx: &mut Context<Self>) {
        if self
            .current
            .as_ref()
            .is_some_and(|notice| notice.message == message)
            || self.pending.contains(&message)
        {
            return;
        }
        self.pending.push_back(message);
        if self.current.is_none() {
            self.advance(cx);
        }
    }

    fn advance(&mut self, cx: &mut Context<Self>) {
        self.dismiss_task = None;
        self.current = self.pending.pop_front().map(|message| {
            self.next_id += 1;
            Notice {
                id: self.next_id,
                message,
                entered: Instant::now(),
                leaving: None,
            }
        });
        if let Some(notice) = &self.current
            && !notice.message.error
        {
            self.schedule_dismiss(notice.id, HOLD, cx);
        }
        cx.notify();
    }

    fn schedule_dismiss(&mut self, id: usize, delay: Duration, cx: &mut Context<Self>) {
        self.dismiss_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if let Some(notice) = &mut this.current
                        && notice.id == id
                    {
                        notice.leaving = Some(Instant::now());
                        cx.notify();
                    }
                });
            }
            cx.background_executor().timer(FADE).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.current.as_ref().is_some_and(|notice| notice.id == id) {
                        this.advance(cx);
                    }
                });
            }
        }));
    }
}

impl Render for Feedback {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(notice) = &self.current else {
            return div().into_any_element();
        };
        let now = Instant::now();
        let entered = now.saturating_duration_since(notice.entered);
        let leaving = notice
            .leaving
            .map(|started| now.saturating_duration_since(started));
        let opacity = transition_opacity(entered, leaving);
        if entered < FADE || leaving.is_some() {
            window.request_animation_frame();
        }
        let id = notice.id;
        let error = notice.message.error;
        let text = notice.message.text.clone();
        let appearance = if self.follow_cover {
            // Let the live cover-derived backdrop supply the hue, not the dark UI tint.
            theme::frosted_appearance()
                .tint(rgba(0x00000018).into())
                .brightness(0.80)
        } else {
            theme::frosted_appearance()
        };

        div()
            .w_full()
            .max_w(px(480.))
            .px(px(12.))
            .flex()
            .justify_center()
            .child(
                div()
                    .relative()
                    .top(px(7. * (1. - opacity)))
                    .opacity(opacity)
                    .max_w_full()
                    .child(
                        theme::frosted()
                            .appearance(appearance)
                            .id(("player-feedback", id))
                            .rounded(px(20.))
                            .max_w_full()
                            .child(
                                div()
                                    .px(px(20.))
                                    .py(px(11.))
                                    .flex()
                                    .items_center()
                                    .gap(px(14.))
                                    .text_size(px(13.))
                                    .line_height(relative(1.4))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .text_color(if error {
                                                theme::palette().danger
                                            } else {
                                                theme::palette().text
                                            })
                                            .child(text),
                                    )
                                    .when(error, |content| {
                                        content.child(
                                            div()
                                                .id("dismiss-feedback")
                                                .flex_none()
                                                .cursor_pointer()
                                                .text_size(px(11.))
                                                .text_color(rgba(0xb2b3bdcc))
                                                .hover(|this| {
                                                    this.text_color(theme::palette().text)
                                                })
                                                .child("Dismiss")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    if this.current.as_ref().is_some_and(|notice| {
                                                        notice.id == id && notice.leaving.is_none()
                                                    }) {
                                                        this.schedule_dismiss(
                                                            id,
                                                            Duration::ZERO,
                                                            cx,
                                                        );
                                                    }
                                                })),
                                        )
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn transition_opacity(entered: Duration, leaving: Option<Duration>) -> f32 {
    fn smoothstep(value: f32) -> f32 {
        let value = value.clamp(0., 1.);
        value * value * (3. - 2. * value)
    }
    let entrance = smoothstep(entered.as_secs_f32() / FADE.as_secs_f32());
    entrance
        * leaving.map_or(1., |elapsed| {
            1. - smoothstep(elapsed.as_secs_f32() / FADE.as_secs_f32())
        })
}

struct GlobalFeedback(Entity<Feedback>);
impl Global for GlobalFeedback {}

pub fn init(cx: &mut App) {
    if !cx.has_global::<GlobalFeedback>() {
        let feedback = cx.new(|_| Feedback {
            follow_cover: false,
            current: None,
            pending: VecDeque::new(),
            next_id: 0,
            dismiss_task: None,
        });
        cx.set_global(GlobalFeedback(feedback));
    }
}

/// Place inside the playback bar's positioned container, without reserving layout space.
pub fn layer(cx: &mut App) -> gpui::Div {
    render_layer(false, cx)
}

/// Blend into the full-screen player's live cover background.
pub fn cover_layer(cx: &mut App) -> gpui::Div {
    render_layer(true, cx)
}

fn render_layer(follow_cover: bool, cx: &mut App) -> gpui::Div {
    let feedback = cx.global::<GlobalFeedback>().0.clone();
    feedback.update(cx, |feedback, cx| {
        if feedback.follow_cover != follow_cover {
            feedback.follow_cover = follow_cover;
            cx.notify();
        }
    });
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(relative(1.))
        .mb(px(14.))
        .flex()
        .justify_center()
        .child(feedback)
}

pub fn success(message: impl Into<SharedString>, cx: &mut App) {
    show(message.into(), false, cx);
}

pub fn error(message: impl Into<SharedString>, cx: &mut App) {
    show(message.into(), true, cx);
}

fn show(text: SharedString, error: bool, cx: &mut App) {
    let feedback = cx.global::<GlobalFeedback>().0.clone();
    feedback.update(cx, |feedback, cx| {
        feedback.push(Message { text, error }, cx)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_fades_in_and_out_without_overshoot() {
        assert_eq!(transition_opacity(Duration::ZERO, None), 0.);
        assert_eq!(transition_opacity(FADE, None), 1.);
        assert_eq!(transition_opacity(HOLD, Some(FADE)), 0.);
        assert_eq!(transition_opacity(HOLD, Some(HOLD)), 0.);
        let halfway = transition_opacity(FADE / 2, None);
        assert!((halfway - 0.5).abs() < 0.001);
        assert!((transition_opacity(HOLD, Some(FADE / 2)) - 0.5).abs() < 0.001);
    }
}
