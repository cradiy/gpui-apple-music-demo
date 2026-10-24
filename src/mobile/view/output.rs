use super::*;
use gpui_effects::{FrostedGlass, FrostedGlassAppearance};
use uic::components::{
    bottom_sheet::BottomSheet,
    modal::{Modal, ModalAppearance},
};

impl MobileApp {
    pub(super) fn open_output_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if modal::is_open(cx) {
            return;
        }
        self.sheet_stack = vec![Sheet::Output];
        self.observe_outputs(cx);
        self.allow_bluetooth_outputs(cx);
        let weak = cx.entity().downgrade();
        let safe = window.insets().effective();
        let scale = (window.viewport_size().width - safe.left - safe.right)
            .as_f32()
            .min(640.)
            / 392.;
        let panel: Modal = BottomSheet::new(move |window, cx| {
            let safe = window.insets().effective();
            let l = Layout {
                s: (window.viewport_size().width - safe.left - safe.right)
                    .as_f32()
                    .min(640.)
                    / 392.,
                h: 0.,
            };
            weak.update(cx, |this, cx| {
                div()
                    .w(px(328. * l.s))
                    .px(px(8. * l.s))
                    .pt(px(8. * l.s))
                    .pb(safe.bottom + px(20. * l.s))
                    .child(this.output_panel(l, cx))
            })
            .unwrap_or_else(|_| div())
        })
        .animation_duration(Duration::from_millis(260))
        .avoid_safe_area(false)
        .drag_to_dismiss(false)
        .close_on_backdrop(true)
        .w(px(328. * scale))
        .h(px(340. * scale) + safe.bottom)
        .rounded_none()
        .bg(rgba(0x00000000))
        .border_0()
        .shadow_none()
        .text_color(rgb(TEXT))
        .font_family("Noto Sans CJK SC")
        .into();
        modal::show(
            panel.appearance(
                ModalAppearance::default()
                    .body_padding_x(px(0.))
                    .body_padding_y(px(0.))
                    .section_borders(false)
                    .backdrop(rgba(0x00000030).into()),
            ),
            window,
            cx,
        );
        window.set_back_enabled(true);
        cx.notify();
    }

    pub(super) fn output_panel(&self, l: Layout, cx: &mut Context<Self>) -> Div {
        let glass = FrostedGlassAppearance::dark()
            .blur_radius(px(32. * l.s))
            .saturation(0.9)
            .brightness(0.96)
            .tint(hsla(0., 0., 0.45, 0.3))
            .edge(rgba(0xffffff26).into())
            .edge_width(px(0.6 * l.s))
            .sheen(0.);
        let mut content = div().relative().size_full();
        if let Some(track) = self.track() {
            content = content
                .child(
                    l.rect(20., 20., 58., 58.)
                        .child(cover(Some(track), 58. * l.s)),
                )
                .child(
                    l.text(92., 30., 180., 15., TEXT, track.title.clone())
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    l.text(92., 52., 180., 13., TEXT, track.artist.clone())
                        .opacity(0.65)
                        .truncate(),
                );
        } else {
            content = content.child(
                l.text(20., 32., 264., 20., TEXT, "Audio Output")
                    .font_weight(FontWeight::SEMIBOLD),
            );
        }
        let mut devices = div()
            .id("audio-output-devices")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(8. * l.s));
        for route in &self.output.snapshot.devices {
            let id = route.id.clone();
            let icon = match route.kind {
                AudioOutputKind::BuiltIn => "output-phone",
                AudioOutputKind::Bluetooth => "output-bluetooth",
                AudioOutputKind::Headphones | AudioOutputKind::Usb => "device",
                _ => "volume-high",
            };
            devices = devices.child(
                div()
                    .id(SharedString::from(format!("audio-output-{id}")))
                    .relative()
                    .w_full()
                    .h(px(46. * l.s))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(if route.selected {
                        rgba(0xf1f1f5a8)
                    } else {
                        rgba(0xffffff14)
                    })
                    .text_color(if route.selected {
                        rgb(0x202522)
                    } else {
                        rgb(TEXT)
                    })
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(route.name.clone())
                    .aria_selected(route.selected)
                    .active(|style| style.opacity(0.75))
                    .child(l.rect(16., 12., 22., 22.).child(icons::icon(icon, l.s)))
                    .child(
                        l.rect(48., 10., 178., 27.)
                            .text_size(px(15. * l.s))
                            .truncate()
                            .child(route.name.clone()),
                    )
                    .when(route.selected, |row| {
                        row.child(
                            l.rect(244., 12., 22., 22.)
                                .child(icons::icon("output-check", l.s)),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.select_output(&id, cx))),
            );
        }
        content = content.child(
            l.rect(
                16.,
                98.,
                280.,
                if self.output.error.is_some() {
                    148.
                } else {
                    194.
                },
            )
            .child(devices),
        );
        if let Some(error) = self.output.error.as_ref() {
            content = content.child(
                l.text(
                    20.,
                    if self.output.snapshot.devices.is_empty() {
                        108.
                    } else {
                        262.
                    },
                    272.,
                    12.,
                    TEXT,
                    error.clone(),
                )
                .opacity(0.8),
            );
        } else if self.output.snapshot.devices.is_empty() {
            content = content.child(
                l.text(20., 108., 272., 14., TEXT, "Looking for audio devices…")
                    .opacity(0.65),
            );
        }
        div().w(px(312. * l.s)).h(px(312. * l.s)).child(
            FrostedGlass::with_appearance(glass)
                .size_full()
                .rounded(px(30. * l.s))
                .occlude()
                .child(content),
        )
    }
}
