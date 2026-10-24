use super::*;

impl MobileApp {
    fn controls(&self, l: Layout, cx: &mut Context<Self>) -> Div {
        let position = self
            .player
            .as_ref()
            .map_or(Duration::ZERO, |p| p.read(cx).position());
        let duration = self
            .player
            .as_ref()
            .and_then(|p| p.read(cx).duration())
            .unwrap_or_default();
        div()
            .absolute()
            .inset_0()
            .size_full()
            .child(
                l.rect(32., l.bottom(546.), 328., 32.).child(
                    Slider::new(&self.timeline_slider)
                        .label("Playback position")
                        .appearance(player_slider(l.s))
                        .thumb_visibility(SliderThumbVisibility::Hover)
                        .size_full(),
                ),
            )
            .child(
                l.text(32., l.bottom(574.), 100., 11., TEXT, time(position))
                    .opacity(0.45),
            )
            .child(
                l.text(
                    260.,
                    l.bottom(574.),
                    100.,
                    11.,
                    TEXT,
                    format!("-{}", time(duration.saturating_sub(position))),
                )
                .text_right()
                .opacity(0.45),
            )
            .child(
                l.button("previous", 60., l.bottom(616.), 64., 64., "player-previous")
                    .aria_label("Previous song")
                    .on_click(cx.listener(|this, _, _, cx| this.skip(-1, cx))),
            )
            .child(self.playback_toggle(l, "toggle", [164., l.bottom(616.), 64., 64.], false, cx))
            .child(
                l.button("next", 268., l.bottom(616.), 64., 64., "player-next")
                    .aria_label("Next song")
                    .on_click(cx.listener(|this, _, _, cx| this.skip(1, cx))),
            )
            .child(
                l.rect(32., l.bottom(725.), 16., 24.)
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(0.45)
                    .child(icons::icon("volume-low", l.s)),
            )
            .child(
                l.rect(56., l.bottom(721.), 272., 32.).child(
                    Slider::new(&self.volume_slider)
                        .label("Volume")
                        .appearance(player_slider(l.s))
                        .thumb_visibility(SliderThumbVisibility::Hover)
                        .size_full(),
                ),
            )
            .child(
                l.rect(340., l.bottom(725.), 20., 24.)
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(0.45)
                    .child(icons::icon("volume-high", l.s)),
            )
            .child(
                footer_button(
                    l,
                    "lyrics",
                    64.,
                    if self.show_lyrics {
                        "player-lyrics-active"
                    } else {
                        "player-lyrics"
                    },
                    self.show_lyrics,
                )
                .aria_label("Lyrics")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_lyrics = !this.show_lyrics;
                    this.show_queue = false;
                    cx.notify();
                })),
            )
            .child(
                footer_button(l, "output", 176., "airplay", false)
                    .aria_label("Audio output")
                    .on_click(cx.listener(|this, _, w, cx| this.open_sheet(Sheet::Output, w, cx))),
            )
            .child(
                footer_button(l, "queue", 288., "queue-list", self.show_queue)
                    .aria_label("Playing next")
                    .aria_selected(self.show_queue)
                    .when(self.show_queue, |button| button.bg(rgba(0xedf0edad)))
                    .when(self.repeat && !self.show_queue, |button| {
                        button.child(
                            l.rect(27., -9., 20., 20.)
                                .rounded_full()
                                .bg(rgba(0xedf0ed28))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    icons::icon("repeat-badge", 0.72 * l.s)
                                        .current_color(rgba(0xf5f7f5dd)),
                                ),
                        )
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_queue = !this.show_queue;
                        this.show_lyrics = false;
                        cx.notify();
                    })),
            )
    }

    fn playing_next(&self, l: Layout, cx: &mut Context<Self>) -> Div {
        let offset = self.queue_ui.as_ref().map_or(0., |ui| ui.header_offset);
        let start = l.top(274.) - offset;
        let end = l.bottom(546.) - 8.;
        let view = cx.entity().downgrade();
        div()
            .absolute()
            .inset_0()
            .size_full()
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
                        window.on_mouse_event(
                            move |event: &ScrollWheelEvent, phase, window, cx| {
                                if phase.capture()
                                    && hitbox.should_handle_scroll(window)
                                    && view
                                        .update(cx, |this, cx| this.scroll_queue(event, l.s, cx))
                                        .unwrap_or(false)
                                {
                                    cx.stop_propagation();
                                }
                            },
                        );
                    },
                )
                .absolute()
                .left_0()
                .top(px(l.top(84.) * l.s))
                .w_full()
                .h(px((end - l.top(84.)).max(0.) * l.s)),
            )
            .children(
                [
                    ("Shuffle", "player-shuffle", self.shuffle, true),
                    ("Repeat", "repeat-badge", self.repeat, true),
                    ("Autoplay unavailable", "autoplay", false, false),
                    ("Crossfade unavailable", "crossfade", false, false),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (label, icon, selected, enabled))| {
                    l.rect(32. + index as f32 * 85., l.top(178.) - offset, 73., 37.)
                        .id(("player-mode", index))
                        .rounded_full()
                        .bg(rgba(if selected || !enabled {
                            0xf1f3f280
                        } else {
                            0xe3e7e438
                        }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .aria_label(label)
                        .aria_disabled(!enabled)
                        .when(enabled, |button| {
                            button
                                .cursor_pointer()
                                .role(Role::Button)
                                .active(|style| style.opacity(0.55))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if index == 0 {
                                        this.shuffle_queue(cx);
                                    } else {
                                        this.repeat = !this.repeat;
                                        cx.notify();
                                    }
                                }))
                        })
                        .child(
                            icons::icon(icon, l.s).current_color(rgb(if selected || !enabled {
                                0x4d5450
                            } else {
                                0xf7f9f7
                            })),
                        )
                }),
            )
            .child(
                l.text(32., l.top(235.) - offset, 328., 16., TEXT, "Playing Next")
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(
                l.rect(32., start, 328., (end - start).max(0.))
                    .overflow_hidden()
                    .when(self.upcoming_queue().is_empty(), |area| {
                        area.child(
                            l.text(0., 12., 328., 15., TEXT, "No upcoming songs")
                                .opacity(0.5),
                        )
                    })
                    .child(self.queue_list(l, cx)),
            )
    }

    pub(super) fn playback_background(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let source = self
            .track()
            .and_then(|track| track.artwork.clone())
            .map(ImageSource::from)
            .unwrap_or_else(|| crate::ui::components::now_playing::fallback_cover().into());
        match source.use_data(None, window, cx) {
            Some(Ok(image)) if image.frame_count() > 0 => {
                self.visible_artwork = Some(ImageSource::Render(image))
            }
            Some(_) => {
                let fallback =
                    ImageSource::from(crate::ui::components::now_playing::fallback_cover());
                if let Some(Ok(image)) = fallback.use_data(None, window, cx) {
                    self.visible_artwork = Some(ImageSource::Render(image));
                }
            }
            None => {}
        }
        let animate = window.is_window_active() && !modal::is_open(cx) && self.playing(cx);
        let now = Instant::now();
        if animate {
            if let Some(previous) = self.backdrop_sampled_at.replace(now) {
                self.backdrop_elapsed += now.saturating_duration_since(previous).as_secs_f32();
            }
            if self.backdrop_frame.is_none() {
                self.backdrop_frame = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs_f64(1. / 15.))
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.backdrop_frame = None;
                        if window.is_window_active()
                            && this.page == Page::NowPlaying
                            && !modal::is_open(cx)
                            && this.playing(cx)
                        {
                            cx.notify();
                        } else {
                            this.backdrop_sampled_at = None;
                        }
                    });
                }));
            }
        } else {
            self.backdrop_sampled_at = None;
            self.backdrop_frame = None;
        }
        self.visible_artwork.clone().map(|artwork| {
            let elapsed = self.backdrop_elapsed;
            let progress = self
                .player_dismiss
                .progress(window.viewport_size().height.as_f32());
            let scale = window.viewport_size().width.as_f32() / 392.;
            crate::ui::components::now_playing::render_background_rounded(
                artwork,
                elapsed,
                px(14. * scale * progress),
            )
        })
    }

    pub(super) fn now_playing(
        &mut self,
        l: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        self.update_controls_idle(window, cx);
        let controls_position = self.controls_motion.sample(self.controls_hidden, window);
        let idle = controls_position.clamp(0., 1.);
        let controls_y = (l.h / l.s - l.bottom(490.) + 24.) * controls_position;
        let controls_opacity = ((1. - idle) * 3.).min(1.);
        let collapse = self
            .player_dismiss
            .progress(window.viewport_size().height.as_f32());
        let morph = |from: f32, to: f32| from + (to - from) * collapse;
        let full_opacity = (1. - collapse * 3.).max(0.);
        let mix = self
            .lyrics_motion
            .sample(f32::from(self.show_lyrics || self.show_queue), window);
        let lerp = |a: f32, b: f32| a + (b - a) * mix;
        let text_x =
            morph(lerp(32., 112.), 62.) + 170. * 4. * collapse * (1. - collapse) * (1. - mix);
        let text_right = morph(392., 360.) - 16. - 102. * collapse.powi(6);
        let text_width = morph(lerp(238., 166.), 180.).min((text_right - text_x).max(24.));
        let art_y = l.top(92.) + (l.bottom(92.) - l.top(92.)).max(0.) * 0.35;
        let art_size = 344_f32.min((l.bottom(466.) - art_y).max(160.));
        if self.show_queue {
            self.prepare_queue(window, cx);
        }
        let title = self
            .track()
            .map_or("Not Playing", |t| t.title.as_str())
            .to_owned();
        let artist = self.track().map_or("", |t| t.artist.as_str()).to_owned();
        let track = self.track().map(|track| (track.id.clone(), track.favorite));
        let mut d = div().size_full().relative().child(
            l.rect(148., l.top(56.), 96., 28.)
                .id("collapse")
                .cursor_pointer()
                .child(
                    l.rect(19., 11., 58., 4.)
                        .rounded_full()
                        .bg(rgba(0xffffff50)),
                )
                .opacity(full_opacity)
                .on_click(cx.listener(|this, _, w, cx| {
                    this.navigate(this.return_page, w, cx);
                })),
        );
        if controls_position < 1. {
            d = d.child(
                self.controls(l, cx)
                    .top(px(controls_y * l.s))
                    .opacity(full_opacity * controls_opacity),
            );
        }
        if self.show_queue {
            d = d.child(self.playing_next(l, cx).opacity(mix * full_opacity));
        }
        if self.show_lyrics && mix > 0.001 {
            if self.lyrics.lines.is_empty() {
                d = d
                    .child(
                        l.rect(0., l.top(298.), 392., 40.)
                            .opacity(full_opacity)
                            .flex()
                            .justify_center()
                            .child(icons::icon("lyrics-empty", l.s)),
                    )
                    .child(
                        l.text(24., l.top(355.), 344., 18., TEXT, "No lyrics available")
                            .opacity(full_opacity)
                            .text_center(),
                    )
                    .child(
                        l.rect(100., l.top(389.), 192., 48.)
                            .opacity(full_opacity)
                            .id("add-lyrics")
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(14. * l.s))
                            .text_color(rgb(ACCENT))
                            .child("Add Lyrics")
                            .on_click(cx.listener(|this, _, w, cx| this.edit_lyrics(w, cx))),
                    );
            } else {
                let mut playback = crate::domain::playback::PlaybackState::empty();
                playback.track.lyrics = self.lyrics.clone();
                if let Some(player) = &self.player {
                    playback.status = player.read(cx).state().clone();
                    playback.timeline = player.read(cx).timeline();
                }
                let entity = cx.entity().downgrade();
                let seek: lyrics_panel::LyricsSeek = Rc::new(move |position, cx| {
                    let _ = entity.update(cx, |this, cx| this.seek_to(position, cx));
                });
                let start = l.top(180.);
                let end = l.bottom(530.) + (l.bottom(810.) - l.bottom(530.)) * idle;
                let height = (end - start).max(100.);
                let lyrics = lyrics_panel::render_mobile(
                    &playback,
                    &mut self.lyric_state,
                    seek,
                    392. * l.s,
                    height * l.s,
                    self.show_translation,
                    window,
                );
                if window.is_window_active()
                    && !modal::is_open(cx)
                    && self.lyric_state.is_animating(&playback, true)
                {
                    window.request_animation_frame();
                }
                let view = cx.entity().downgrade();
                d = d.child(faded_lyrics(
                    l.rect(0., start, 392., height)
                        .id("player-lyrics-viewport")
                        .overflow_hidden()
                        .opacity(mix * full_opacity)
                        .child(
                            canvas(
                                |bounds, window, _| {
                                    window.insert_hitbox(bounds, HitboxBehavior::Normal)
                                },
                                move |_, hitbox, window, _| {
                                    window.on_mouse_event(
                                        move |event: &TouchEvent, phase, window, cx| {
                                            if phase.capture() {
                                                let _ = view.update(cx, |this, cx| {
                                                    let inside = hitbox.is_hovered(window);
                                                    this.touch_player_controls(event, inside, cx);
                                                    if this
                                                        .lyric_state
                                                        .handle_mobile_touch(event, inside)
                                                    {
                                                        cx.notify();
                                                    }
                                                });
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .child(lyrics)
                        .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                            if event.click_count() == 2 {
                                this.reveal_player_controls(cx);
                            }
                        }))
                        .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                            this.scroll_player_controls(event, cx);
                        })),
                    l.s,
                ));
            }
            if controls_position < 1.
                && self.lyrics.lines.iter().any(|line| {
                    line.translation
                        .as_ref()
                        .is_some_and(|text| !text.trim().is_empty())
                })
            {
                d = d.child(
                    l.rect(32., l.bottom(490.) + controls_y, 38., 38.)
                        .id("lyrics-translation")
                        .occlude()
                        .rounded_full()
                        .bg(rgb(if self.show_translation {
                            0xb9bdb9
                        } else {
                            0x90978e
                        }))
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(mix * full_opacity * controls_opacity)
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_selected(self.show_translation)
                        .aria_label(if self.show_translation {
                            "Hide translations"
                        } else {
                            "Show translations"
                        })
                        .active(|style| style.opacity(0.55))
                        .child(icons::icon("translation", l.s).current_color(
                            if self.show_translation {
                                rgba(0x293638ff)
                            } else {
                                rgba(0xf7f8f4ff)
                            },
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_translation = !this.show_translation;
                            this.lyric_state.resume_mobile_following();
                            cx.notify();
                        })),
                );
            }
        }
        let artwork = self.visible_artwork.clone();
        let cover_size = morph(lerp(art_size, 70.), 36.) * l.s;
        let cover_radius = morph(8., 6.) * l.s;
        let paused_scale = if self.playing(cx) { 1. } else { 0.82 };
        let cover =
            gpui_effects::animated_number("playback-cover-scale", paused_scale, move |scale| {
                let scale = 1. - (1. - scale as f32) * (1. - mix) * (1. - collapse);
                let inset = cover_size * (1. - scale) * 0.5;
                let size = cover_size * scale;
                div()
                    .relative()
                    .left(px(inset))
                    .top(px(inset))
                    .size(px(size))
                    .child(artwork_cover(artwork, size, cover_radius * scale))
            })
            .duration(Duration::from_millis(320));
        let mut header = div()
            .absolute()
            .left_0()
            .top_0()
            .w_full()
            .h(px(l.h))
            .child(
                l.rect(
                    morph(lerp((392. - art_size) / 2., 32.), 16.),
                    morph(lerp(art_y, l.top(92.)), 11.),
                    morph(lerp(art_size, 70.), 36.),
                    morph(lerp(art_size, 70.), 36.),
                )
                .child(cover)
                .id("player-cover")
                .when(
                    (self.show_lyrics || self.show_queue) && collapse == 0.,
                    |cover| {
                        cover
                            .cursor_pointer()
                            .role(Role::Button)
                            .aria_label("Show player")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_lyrics = false;
                                this.show_queue = false;
                                this.reveal_player_controls(cx);
                                cx.notify();
                            }))
                    },
                ),
            )
            .child(
                l.text(
                    text_x,
                    morph(lerp(l.bottom(480.), l.top(108.)), 6.),
                    text_width,
                    morph(lerp(18., 16.), 14.),
                    TEXT,
                    title,
                )
                .truncate()
                .font_weight(if collapse > 0.8 {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::BOLD
                }),
            )
            .child(
                l.text(
                    text_x,
                    morph(lerp(l.bottom(503.), l.top(130.)), 29.),
                    morph(lerp(238., 166.), 180.).min(text_width + 48. * (1. - collapse)),
                    morph(lerp(16., 14.), 10.),
                    TEXT,
                    artist,
                )
                .opacity(0.6)
                .truncate(),
            );
        if let Some((id, favorite)) = &track {
            let id = id.clone();
            header = header.child(
                l.button(
                    "favorite",
                    282.,
                    lerp(l.bottom(479.), l.top(106.)),
                    32.,
                    40.,
                    if *favorite { "star-filled" } else { "star" },
                )
                .opacity(full_opacity)
                .aria_label("Favorite")
                .on_click(cx.listener(move |this, _, _, cx| this.favorite(id.clone(), cx))),
            );
        }
        if let Some((id, _)) = track {
            header = header.child(
                l.button(
                    "song-menu",
                    324.,
                    lerp(l.bottom(479.), l.top(106.)),
                    40.,
                    40.,
                    "player-more",
                )
                .opacity(full_opacity)
                .aria_label("Song options")
                .on_click(cx.listener(move |this, _, w, cx| {
                    this.open_sheet(Sheet::Song(id.clone()), w, cx)
                })),
            );
        }
        if self.show_queue {
            let compact = mix * (1. - collapse);
            let offset = self.queue_ui.as_ref().map_or(0., |ui| ui.header_offset) * compact;
            let top = l.top(84.) * compact;
            let height = l.h / l.s + (88. - l.h / l.s) * compact;
            d = d.child(
                l.rect(0., top, 392., height)
                    .overflow_hidden()
                    .child(header.top(px((-top - offset) * l.s))),
            );
        } else {
            d = d.child(header);
        }
        if collapse > 0. {
            let opacity = ((collapse - 0.55) / 0.45).clamp(0., 1.);
            d = d
                .child(
                    l.button(
                        "morph-mini-toggle",
                        246.,
                        5.,
                        44.,
                        48.,
                        if self.playing(cx) {
                            "mini-pause"
                        } else {
                            "mini-play"
                        },
                    )
                    .opacity(opacity),
                )
                .child(
                    l.button("morph-mini-next", 294., 5., 44., 48., "mini-next")
                        .opacity(opacity),
                );
        }
        d
    }
}

fn faded_lyrics(content: Stateful<Div>, scale: f32) -> gpui_effects::SubtreeEffect<Stateful<Div>> {
    static SHADER: std::sync::OnceLock<EffectShader> = std::sync::OnceLock::new();
    let shader = SHADER.get_or_init(|| EffectShader::wgsl_image(include_str!("lyrics_fade.wgsl")));
    gpui_effects::subtree_effect(content, shader.clone())
        .uniform_pixels(0, [px(48. * scale), px(56. * scale), px(0.), px(0.)])
}

fn player_slider(scale: f32) -> SliderAppearance {
    SliderAppearance::default()
        .active_track(rgba(0xffffff66).into())
        .thumb(rgba(0xffffffaa).into())
        .thumb_border(rgba(0xffffff00).into())
        .focus_ring(rgba(0xffffff00).into())
        .thumb_size(px(7. * scale))
        .bg(rgba(0xffffff26))
        .h(px(6. * scale))
        .rounded_full()
}

fn footer_button(l: Layout, id: &'static str, x: f32, icon: &str, selected: bool) -> Stateful<Div> {
    l.rect(x, l.bottom(766.), 40., 40.)
        .id(id)
        .rounded_full()
        .cursor_pointer()
        .role(Role::Button)
        .active(|style| style.opacity(0.55))
        .when(selected, |button| button.bg(rgba(0xffffff70)))
        .flex()
        .items_center()
        .justify_center()
        .child(icons::icon(icon, l.s).current_color(if selected {
            rgba(0x293638ff)
        } else {
            rgba(0xffffffaa)
        }))
}
