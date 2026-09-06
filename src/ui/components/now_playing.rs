use std::{sync::Arc, time::Instant};

use gpui::{
    AnyElement, ClipboardItem, Div, FontWeight, Image, ImageFormat, MouseButton, ObjectFit,
    SharedString, Stateful, Window, div, img, prelude::*, px, rgb, rgba, svg,
};
use gpui_effects::{
    ColorFlowOptions, LiquidGlass, LiquidGlassAppearance, color_flow, paint_liquid_glass,
};
use uic::{
    assets::LucideIcons,
    components::context_menu::{self, ContextMenu},
};

use crate::ui::components::feedback as toast;
use crate::{
    app::music_shell::{EffectsViewer, MusicShell, pages::songs},
    domain::Page,
    resources::RomusIcons,
    services::api::TrackResponse,
    ui::{
        components::{lyrics_panel, player_bar, queue_panel},
        strings, theme,
    },
};

const FALLBACK_COVER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="800" viewBox="0 0 800 800">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#ff375f"/><stop offset=".5" stop-color="#6e5bff"/><stop offset="1" stop-color="#10131d"/></linearGradient></defs>
<rect width="800" height="800" rx="120" fill="url(#g)"/><path d="M510 175v360c0 74-60 134-134 134s-134-60-134-134 60-134 134-134c28 0 54 9 75 23V232l59-57z" fill="white" fill-opacity=".9"/>
</svg>"##;

/// Fade out the visible panel before replacing it, then fade in the new panel.
/// Keep the current visibility when reversing direction during a rapid toggle.
#[derive(Default)]
pub struct PanelTransition {
    frame: Option<PanelTransitionFrame>,
}

struct PanelTransitionFrame {
    lyrics: bool,
    visibility: f32,
    at: Instant,
}

impl PanelTransition {
    fn sample(&mut self, target_lyrics: bool, now: Instant) -> (bool, f32) {
        const FADE_SECONDS: f32 = 0.16;
        let frame = self.frame.get_or_insert(PanelTransitionFrame {
            lyrics: target_lyrics,
            visibility: 1.,
            at: now,
        });
        let step = now.saturating_duration_since(frame.at).as_secs_f32() / FADE_SECONDS;
        frame.at = now;
        if frame.lyrics == target_lyrics {
            frame.visibility = (frame.visibility + step).min(1.);
        } else if step < frame.visibility {
            frame.visibility -= step;
        } else {
            // Change the layout only at zero opacity; apply any remaining frame
            // time to the incoming panel so slow frames do not extend the fade.
            frame.lyrics = target_lyrics;
            frame.visibility = (step - frame.visibility).min(1.);
        }
        let t = frame.visibility;
        (frame.lyrics, t * t * (3. - 2. * t))
    }
}

pub fn render(
    app: &mut MusicShell,
    window: &mut Window,
    window_width: f32,
    window_height: f32,
    cx: &mut gpui::Context<MusicShell>,
) -> AnyElement {
    let (playback, current_track) = {
        let controller = app.playback.read(cx);
        (
            controller.snapshot.clone(),
            controller.queue.current().cloned(),
        )
    };
    let artwork = playback_artwork(&playback);
    let track_id = current_track.as_ref().map(|track| track.id.clone());
    let (effect_artwork, background_time) = {
        let needs_refresh = app.effects_viewer.as_ref().is_none_or(|effect| {
            effect.track_id != track_id || !Arc::ptr_eq(&effect.album_cover, &artwork)
        });
        if needs_refresh {
            app.effects_viewer = Some(EffectsViewer {
                track_id,
                album_cover: artwork.clone(),
                started_at: Instant::now(),
            });
        }
        let effect = app
            .effects_viewer
            .as_ref()
            .expect("effects viewer initialized");
        (
            effect.album_cover.clone(),
            effect.started_at.elapsed().as_secs_f32(),
        )
    };
    let title = playback.track.title.clone();
    let artist = playback.track.artist.clone();
    let subtitle = artist.to_string();
    let (show_lyrics, panel_opacity) = app
        .now_playing_panel_transition
        .sample(app.now_playing_lyrics, Instant::now());
    let cover_size = (window_height - 310.)
        .min(window_width * 0.30)
        .clamp(280., 430.);
    let favorite = current_track.as_ref().is_some_and(|track| track.favorite);
    let side_panel = if show_lyrics {
        div()
            .h_full()
            .flex_1()
            .min_w(px(390.))
            .max_w(px(650.))
            .opacity(panel_opacity)
            .child(lyrics_panel::render_fullscreen(
                &playback,
                &mut app.lyrics_panel,
                app.playback.clone(),
            ))
            .into_any_element()
    } else {
        div()
            .w(px(650.))
            .h(gpui::relative(0.92))
            .flex_none()
            .opacity(panel_opacity)
            .flex()
            .justify_center()
            .overflow_hidden()
            .child(
                div()
                    .w(px(520.))
                    .h_full()
                    .child(queue_panel::render_now_playing(app, window, cx)),
            )
            .into_any_element()
    };

    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .when(cfg!(not(target_os = "windows")), |root| {
            root.rounded(px(22.))
        })
        .text_color(rgba(0xffffffff))
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .overflow_hidden()
                .child(
                    color_flow(effect_artwork)
                        .options(
                            ColorFlowOptions::default()
                                .brightness(0.85)
                                .cohesion(0.85)
                                .drift(0.45), // .diffusion(0.22)
                                              // .saturation(0.9)
                                              // .motion(1.0)
                                              // .dither(1.0),
                        )
                        .time(background_time)
                        .size_full(),
                ),
        )
        .child(
            div()
                .relative()
                .size_full()
                .px(px(if show_lyrics { 72. } else { 48. }))
                .pt(px(82.))
                .pb(px(62.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(92.))
                .child(
                    div()
                        .w(px(cover_size))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .size(px(cover_size))
                                .flex_none()
                                .overflow_hidden()
                                .rounded_3xl()
                                .shadow(vec![
                                    gpui::BoxShadow::new(px(0.), px(24.), rgba(0x0000005e).into())
                                        .blur_radius(px(52.))
                                        .spread_radius(px(-15.)),
                                ])
                                .child(
                                    img(artwork)
                                        .size_full()
                                        .rounded_3xl()
                                        .object_fit(ObjectFit::Cover),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .mt(px(18.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(14.))
                                                .font_weight(FontWeight::BOLD)
                                                .child(title),
                                        )
                                        .child(
                                            div()
                                                .mt(px(3.))
                                                .truncate()
                                                .text_size(px(11.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgba(0xffffffe6))
                                                .child(subtitle),
                                        ),
                                )
                                .child(
                                    div()
                                        .ml(px(16.))
                                        .flex()
                                        .items_center()
                                        .gap(px(9.))
                                        .child(
                                            round_icon_button(
                                                "favorite-track",
                                                if favorite {
                                                    RomusIcons::FillStar.path()
                                                } else {
                                                    LucideIcons::Star.path()
                                                },
                                                20.,
                                                favorite,
                                                current_track.is_some(),
                                            )
                                            .when(
                                                current_track.is_some(),
                                                |button| {
                                                    button.on_click(cx.listener(|app, _, _, cx| {
                                                        app.toggle_current_favorite(cx);
                                                    }))
                                                },
                                            ),
                                        )
                                        .child(
                                            round_icon_button(
                                                "track-actions",
                                                LucideIcons::Ellipsis,
                                                22.,
                                                false,
                                                current_track.is_some(),
                                            )
                                            .when_some(current_track.clone(), |button, track| {
                                                let entity = cx.entity();
                                                button.on_mouse_down(
                                                    MouseButton::Left,
                                                    move |event, window, cx| {
                                                        let menu = now_playing_track_menu(
                                                            entity.clone(),
                                                            track.clone(),
                                                        );
                                                        let _ = context_menu::show(
                                                            menu,
                                                            event.position,
                                                            window,
                                                            cx,
                                                        );
                                                        cx.stop_propagation();
                                                    },
                                                )
                                            }),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .mt(px(18.))
                                .child(player_bar::render_now_playing(app, cx, rgba(0xffffffff)))
                                .child(
                                    toast::cover_layer(cx)
                                        .bottom(gpui::Length::Auto)
                                        .top(gpui::relative(1.))
                                        .mb(px(0.))
                                        .mt(px(14.)),
                                ),
                        ),
                )
                .child(side_panel),
        )
        .child(render_window_controls(app, window, cx))
        .child(render_view_switcher(app, cx))
        .into_any_element()
}

// Native traffic lights and the adjacent controls share this top-row anchor.
pub(crate) const WINDOW_CONTROLS_TOP: f32 = if cfg!(target_os = "macos") { 8. } else { 18. };

fn render_window_controls(
    _app: &mut MusicShell,
    window: &Window,
    cx: &mut gpui::Context<MusicShell>,
) -> AnyElement {
    div()
        .absolute()
        .top(px(WINDOW_CONTROLS_TOP))
        .left(px(18.))
        .right(px(18.))
        .h(px(34.))
        .flex()
        .items_center()
        .justify_between()
        .child(
            controls_capsule(true)
                .when(cfg!(target_os = "macos"), |this| this.ml(px(68.)))
                .child(
                    top_icon_button("close-now-playing", LucideIcons::X).on_click(cx.listener(
                        |app, _, _, cx| {
                            app.now_playing = false;
                            cx.notify();
                        },
                    )),
                )
                .child(
                    top_icon_button("mini-now-playing", LucideIcons::PictureInPicture2).on_click(
                        cx.listener(|app, _, window, cx| {
                            app.enter_mini_player(window, cx);
                        }),
                    ),
                ),
        )
        .when(cfg!(not(target_os = "macos")), |bar| {
            bar.child(super::window_controls::drag_region().h_full().flex_1())
        })
        .when(super::window_controls::visible(cx), |bar| {
            bar.pr(px(super::window_controls::width())).child(
                div()
                    .absolute()
                    .top(px(-WINDOW_CONTROLS_TOP))
                    .right(px(-18.))
                    .w(px(super::window_controls::width()))
                    .h(px(super::window_controls::HEIGHT))
                    .group("now-playing-window-controls")
                    .child(
                        super::window_controls::render(window)
                            .invisible()
                            .group_hover("now-playing-window-controls", |controls| {
                                controls.visible()
                            }),
                    ),
            )
        })
        .into_any_element()
}

fn controls_glass_appearance() -> LiquidGlassAppearance {
    LiquidGlassAppearance::clear()
        .blur_radius(px(10.))
        .clarity(0.35)
        .refraction(px(4.))
        .thickness(px(10.))
        .dispersion(0.005)
        .tint(rgba(0xffffff30).into())
        .brightness(1.0)
        .highlight(0.32)
        .edge_shadow(0.14)
        .rim_width(px(0.7))
}

fn controls_capsule(edge_tint: bool) -> LiquidGlass {
    LiquidGlass::with_appearance(if edge_tint {
        controls_glass_appearance()
            .edge_tint_width(px(2.))
            .edge_tint_strength(0.4)
    } else {
        controls_glass_appearance()
    })
    .h(px(36.))
    .px(px(5.))
    .flex()
    .items_center()
    .gap(px(12.))
    .rounded_full()
    .border_1()
    .border_color(rgba(0xffffff1a))
}

fn render_view_switcher(app: &mut MusicShell, cx: &mut gpui::Context<MusicShell>) -> AnyElement {
    controls_capsule(true)
        .absolute()
        .right(px(18.))
        .bottom(px(16.))
        .child(
            div()
                .id("toggle-now-playing-lyrics")
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .when(app.now_playing_lyrics, |button| button.bg(rgba(0xffffff24)))
                .hover(|style| style.bg(rgba(0xffffff20)))
                .on_click(cx.listener(|app, _, _, cx| {
                    app.now_playing_lyrics = true;
                    cx.notify();
                }))
                .child(
                    svg()
                        .path(LucideIcons::MicVocal)
                        .size(px(20.))
                        .text_color(rgba(0xffffffff)),
                ),
        )
        .child(
            div()
                .id("now-playing-queue")
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .when(!app.now_playing_lyrics, |button| {
                    button.bg(rgba(0xffffff24))
                })
                .hover(|style| style.bg(rgba(0xffffff20)))
                .on_click(cx.listener(|app, _, _, cx| {
                    app.now_playing_lyrics = false;
                    cx.notify();
                }))
                .child(
                    svg()
                        .path(LucideIcons::ListMusic)
                        .size(px(20.))
                        .text_color(rgba(0xffffffe3)),
                ),
        )
        .into_any_element()
}

fn round_icon_button(
    id: &'static str,
    icon: impl Into<SharedString>,
    icon_size: f32,
    active: bool,
    enabled: bool,
) -> Stateful<Div> {
    div()
        .on_paint_before_children(|bounds, style, window, _| {
            let corners = style.corner_radii.to_pixels(window.rem_size());
            paint_liquid_glass(bounds, corners, controls_glass_appearance(), window);
        })
        .id(id)
        .size(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0xffffff22)))
        })
        .when(!enabled, |button| button.opacity(0.42))
        .when(active, |button| button.bg(rgba(0xffffff22)))
        .child(svg().path(icon).size(px(icon_size)).text_color(if active {
            rgb(0xffffffff)
        } else {
            rgba(0xffffffe5)
        }))
}

fn top_icon_button(id: &'static str, icon: LucideIcons) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(27.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|button| button.bg(rgba(0xffffff20)))
        .child(svg().path(icon).size(px(20.)).text_color(rgba(0xffffffff)))
}

fn now_playing_track_menu(entity: gpui::Entity<MusicShell>, track: TrackResponse) -> ContextMenu {
    let info_track = track.clone();
    let next_track = track.clone();
    let enqueue_track = track.clone();
    let copy_track = track.clone();
    let artist_id = track.artist_id.clone();
    let album_id = track.album_id.clone();
    let like_label = if track.favorite { "Unlike" } else { "Like" };
    let favorite_entity = entity.clone();
    let next_entity = entity.clone();
    let enqueue_entity = entity.clone();

    let mut menu = ContextMenu::new()
        .action_with(
            |_, _| menu_label(LucideIcons::BadgeInfo, "Get Info"),
            move |window, cx| songs::show_track_info(info_track.clone(), window, cx),
        )
        .action_with(
            move |_, _| menu_label(LucideIcons::Heart, like_label),
            move |_, cx| {
                favorite_entity.update(cx, |app, cx| app.toggle_current_favorite(cx));
            },
        )
        .separator()
        .action_with(
            |_, _| menu_label(LucideIcons::ListPlus, strings::play_next()),
            move |_, cx| {
                next_entity.update(cx, |app, cx| {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_next(next_track.clone(), cx)
                    });
                });
                toast::success(strings::play_next(), cx);
            },
        )
        .action_with(
            |_, _| menu_label(LucideIcons::ListEnd, strings::add_to_queue()),
            move |_, cx| {
                enqueue_entity.update(cx, |app, cx| {
                    app.playback.update(cx, |playback, cx| {
                        playback.enqueue(enqueue_track.clone(), cx)
                    });
                });
                toast::success(strings::add_to_queue(), cx);
            },
        )
        .separator()
        .action_with(
            |_, _| menu_label(LucideIcons::Copy, "Copy"),
            move |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(songs::song_clipboard_text(
                    &copy_track,
                )));
                toast::success("Song info copied", cx);
            },
        );
    if let Some(artist_id) = artist_id {
        let artist_entity = entity.clone();
        menu = menu.action_with(
            |_, _| menu_label(LucideIcons::MicVocal, "Go to Artist"),
            move |_, cx| {
                artist_entity.update(cx, |app, cx| {
                    app.now_playing = false;
                    app.navigate(Page::Artist(artist_id.clone().into()), cx);
                });
            },
        );
    }
    if let Some(album_id) = album_id {
        menu = menu.action_with(
            |_, _| menu_label(LucideIcons::DiscAlbum, "Go to Album"),
            move |_, cx| {
                entity.update(cx, |app, cx| {
                    app.now_playing = false;
                    app.navigate(Page::Album(album_id.clone().into()), cx);
                });
            },
        );
    }
    theme::style_fullscreen_context_menu(menu)
}

fn menu_label(icon: LucideIcons, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(svg().path(icon).size(px(16.)).text_color(rgba(0xf2f0f6f2)))
        .child(label)
}

pub(crate) fn playback_artwork(playback: &crate::domain::PlaybackState) -> Arc<Image> {
    playback
        .track
        .artwork
        .as_ref()
        .map(|artwork| artwork.image.clone())
        .unwrap_or_else(fallback_cover)
}

fn fallback_cover() -> Arc<Image> {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        FALLBACK_COVER.as_bytes().to_vec(),
    ))
}
