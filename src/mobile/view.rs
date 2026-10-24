mod player;

use super::*;
use gpui_effects::{LiquidGlass, LiquidGlassAppearance};
use uic::components::{
    glass::{GlassSegmentedAppearance, GlassSegmentedControl},
    input::Input,
    reorderable_list::{ReorderEvent, ReorderHandle, ReorderableList},
    slider::{Slider, SliderAppearance, SliderThumbVisibility},
    swipe_actions::{SwipeActions, SwipeDirection},
};

// Coordinates are taken from the 392 x 852 Rovar frames. Android owns the
// status/navigation bars; content starts at design y=36 and ends at y=830.
#[derive(Clone, Copy)]
struct Layout {
    s: f32,
    h: f32,
}
impl Layout {
    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Div {
        div()
            .absolute()
            .left(px(x * self.s))
            .top(px(y * self.s))
            .w(px(w * self.s))
            .h(px(h * self.s))
    }
    fn top(self, y: f32) -> f32 {
        y - 36.
    }
    fn bottom(self, y: f32) -> f32 {
        self.h / self.s - (830. - y)
    }
    fn text(
        self,
        x: f32,
        y: f32,
        w: f32,
        size: f32,
        color: u32,
        text: impl Into<SharedString>,
    ) -> Div {
        self.rect(x, y, w, size * 1.6)
            .text_size(px(size * self.s))
            .line_height(px(size * 1.5 * self.s))
            .text_color(rgb(color))
            .overflow_hidden()
            .child(text.into())
    }
    fn button(
        self,
        id: impl Into<ElementId>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        icon: &str,
    ) -> Stateful<Div> {
        self.rect(x, y, w, h)
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .role(Role::Button)
            .active(|style| style.opacity(0.55))
            .child(icons::icon(icon, self.s))
    }
}
fn chrome_glass(scale: f32) -> LiquidGlassAppearance {
    let mut surface = GlassSegmentedAppearance::dark()
        .surface
        .tint(hsla(0., 0., 0., 0.))
        .blur_radius(px(0.))
        .clarity(1.)
        .brightness(1.5)
        .compress_highlights(true)
        .refraction(px(7.))
        .full_refraction(true);
    surface.blur_radius *= scale;
    surface.refraction *= scale;
    surface.thickness *= scale;
    surface.rim_width *= scale;
    surface.edge_tint_width *= scale;
    surface.edge_sample_distance *= scale;
    surface
}

fn cover(track: Option<&Track>, size: f32) -> Div {
    artwork_cover(
        track.and_then(|t| t.artwork.clone()).map(Into::into),
        size,
        size * 0.06,
    )
}

fn artwork_cover(source: Option<ImageSource>, size: f32, radius: f32) -> Div {
    let radius = px(radius);
    div()
        .size(px(size))
        .rounded(radius)
        .overflow_hidden()
        .bg(rgb(SURFACE))
        .when_some(source, |d, source| {
            d.child(
                img(source)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(radius),
            )
        })
}
fn time(d: Duration) -> String {
    format!("{}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
}

impl MobileApp {
    fn mini(&self, l: Layout, nav: bool, cx: &mut Context<Self>) -> Div {
        if self.page == Page::NowPlaying && self.player_dismiss.active() {
            return div();
        }
        let Some(track) = self.track() else {
            return div();
        };
        let y = l.bottom(if nav { 690. } else { 760. });
        l.rect(16., y, 360., 58.).child(
            LiquidGlass::with_appearance(chrome_glass(l.s))
                .size_full()
                .rounded_full()
                .occlude()
                .child(l.rect(16., 11., 36., 36.).child(artwork_cover(
                    track.artwork.clone().map(Into::into),
                    36. * l.s,
                    6. * l.s,
                )))
                .child(
                    l.text(62., 6., 180., 14., TEXT, track.title.clone())
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    l.text(62., 29., 180., 10., SECONDARY, track.artist.clone())
                        .truncate(),
                )
                .child(
                    l.rect(0., 0., 246., 58.)
                        .id("open-player")
                        .on_click(cx.listener(|this, _, w, cx| {
                            this.show_lyrics = false;
                            this.navigate(Page::NowPlaying, w, cx);
                        })),
                )
                .child(
                    l.button(
                        "mini-toggle",
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
                    .on_click(cx.listener(|this, _, _, cx| this.toggle(cx))),
                )
                .child(
                    l.button("mini-next", 294., 5., 44., 48., "mini-next")
                        .on_click(cx.listener(|this, _, _, cx| this.skip(1, cx))),
                ),
        )
    }
    fn nav(&self, page: Page, l: Layout, cx: &mut Context<Self>) -> Div {
        let mut appearance = GlassSegmentedAppearance::dark();
        appearance.surface = chrome_glass(l.s);
        let selection = &mut appearance.selection;
        selection.tint.a = 0.12;
        selection.blur_radius *= l.s;
        selection.refraction *= l.s;
        selection.thickness *= l.s;
        selection.edge_refraction *= l.s;
        selection.edge_width *= l.s;
        selection.rim_width *= l.s;
        appearance.selected_text = rgb(ACCENT).into();
        let entity = cx.entity();
        let mut control = GlassSegmentedControl::new("mobile-navigation", page)
            .label("Main navigation")
            .appearance(appearance)
            .selection_press_scale(1.35)
            .selection_horizontal_outset(px(4. * l.s))
            .surface_press_scale(1.05)
            .size_full()
            .p(px(4. * l.s))
            .gap_0()
            .justify_between()
            .border_0()
            .rounded_full()
            .text_color(rgb(0xf5f5f7))
            .on_change(move |destination, window, cx| {
                entity.update(cx, |this, cx| this.navigate(destination, window, cx));
            });
        for (destination, label, icon) in [
            (Page::Home, "Home", "nav-home"),
            (Page::Discover, "Discover", "nav-discover"),
            (Page::Radio, "Radio", "nav-radio"),
            (Page::Library, "Library", "nav-library"),
            (Page::Search, "Search", "nav-search"),
        ] {
            control = control.option(
                destination,
                div()
                    .w(px(344. * l.s / 5. - 36.))
                    .h(px(36. * l.s))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(l.s))
                    .child(
                        svg()
                            .path(format!("icons/android/{icon}.svg"))
                            .size(px(25. * l.s))
                            .text_color(rgb(0xf5f5f7))
                            .flex_none(),
                    )
                    .child(
                        div()
                            .text_size(px(10. * l.s))
                            .line_height(px(10. * l.s))
                            .whitespace_nowrap()
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    ),
            );
        }
        l.rect(20., l.bottom(758.), 352., 60.).child(control)
    }
    fn row(&self, track: &Track, index: usize, l: Layout, cx: &mut Context<Self>) -> Div {
        let id = track.id.clone();
        let menu = id.clone();
        div()
            .relative()
            .w_full()
            .h(px(64. * l.s))
            .child(
                l.rect(0., 0., 48., 48.)
                    .child(cover(Some(track), 48. * l.s)),
            )
            .child(l.text(
                62.,
                -1.,
                222.,
                15.,
                if self.current.as_ref() == Some(&id) {
                    ACCENT
                } else {
                    TEXT
                },
                track.title.clone(),
            ))
            .child(l.text(62., 26., 222., 11., SECONDARY, track.artist.clone()))
            .child(
                l.rect(0., 0., 288., 55.)
                    .id(("track", index))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if modal::is_open(cx) {
                            modal::dismiss(w, cx);
                        }
                        this.play_track(&id, cx);
                        this.show_lyrics = false;
                        this.navigate(Page::NowPlaying, w, cx);
                    })),
            )
            .child(
                l.button(("more", index), 296., 0., 48., 48., "more")
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.open_sheet(Sheet::Song(menu.clone()), w, cx)
                    })),
            )
    }
    fn header(&self, l: Layout, title: &str, back: bool, cx: &mut Context<Self>) -> Div {
        let d = div().child(
            l.text(
                if back { 58. } else { 24. },
                l.top(if back { 51. } else { 45. }),
                280.,
                if back { 24. } else { 30. },
                TEXT,
                title.to_owned(),
            )
            .font_weight(FontWeight::BOLD),
        );
        if back {
            d.child(
                l.button("back", 8., l.top(44.), 40., 44., "back")
                    .on_click(cx.listener(|this, _, w, cx| this.back(w, cx))),
            )
        } else {
            d
        }
    }
    fn browse(&self, page: Page, l: Layout, window: &Window, cx: &mut Context<Self>) -> Div {
        let tracks = self
            .library
            .as_ref()
            .map_or(&[][..], |lib| lib.tracks.as_slice());
        let home = page == Page::Home;
        let search = page == Page::Search;
        let collection = page == Page::Collection;
        let searching_with_keyboard =
            matches!(page, Page::Search | Page::Library) && window.insets().ime.bottom > px(0.);
        let list_bottom = l.h / l.s;
        let bottom_padding = px(l.s
            * if searching_with_keyboard {
                12.
            } else if collection {
                80.
            } else if self.track().is_some() {
                150.
            } else {
                82.
            });
        let mut d = div().relative().size_full().child(self.header(
            l,
            if home {
                "Home"
            } else if search {
                "Search"
            } else if collection {
                &self.collection_title
            } else {
                "Library"
            },
            collection,
            cx,
        ));
        if home {
            d = d
                .child(
                    l.button("search", 324., l.top(44.), 48., 48., "search")
                        .on_click(cx.listener(|this, _, w, cx| this.navigate(Page::Search, w, cx))),
                )
                .child(
                    l.text(24., l.top(112.), 240., 20., TEXT, "Recently Added")
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    l.rect(300., l.top(108.), 76., 44.)
                        .id("see-all")
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12. * l.s))
                        .text_color(rgb(ACCENT))
                        .child("See All")
                        .on_click(
                            cx.listener(|this, _, w, cx| this.navigate(Page::Library, w, cx)),
                        ),
                );
            for (i, track) in tracks.iter().rev().take(2).enumerate() {
                let id = track.id.clone();
                let x = 24. + i as f32 * 180.;
                d = d
                    .child(
                        l.rect(x, l.top(158.), 164., 164.)
                            .id(("album", i))
                            .child(cover(Some(track), 164. * l.s))
                            .on_click(
                                cx.listener(move |this, _, w, cx| this.open_album(&id, w, cx)),
                            ),
                    )
                    .child(
                        l.text(x, l.top(334.), 164., 16., TEXT, track.title.clone())
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(l.text(x, l.top(363.), 164., 12., SECONDARY, track.artist.clone()));
            }
            d = d.child(
                l.text(24., l.top(417.), 344., 20., TEXT, "Recently Played")
                    .font_weight(FontWeight::SEMIBOLD),
            );
            let recent = self
                .recent
                .iter()
                .filter_map(|id| tracks.iter().find(|t| &t.id == id))
                .collect::<Vec<_>>();
            d = d.child(
                l.rect(24., l.top(464.), 344., (list_bottom - l.top(464.)).max(0.))
                    .id("recent")
                    .overflow_y_scroll()
                    .pb(bottom_padding)
                    .children(
                        recent
                            .iter()
                            .enumerate()
                            .map(|(i, t)| self.row(t, i, l, cx)),
                    ),
            );
            if tracks.is_empty() {
                d = d.child(
                    l.rect(24., l.top(175.), 344., 48.)
                        .id("import-empty")
                        .text_color(rgb(ACCENT))
                        .child("Add Music")
                        .on_click(
                            cx.listener(|this, _, w, cx| this.open_sheet(Sheet::Import, w, cx)),
                        ),
                );
            }
        } else if collection {
            let selected = tracks
                .iter()
                .filter(|t| self.collection.contains(&t.id))
                .collect::<Vec<_>>();
            d = d
                .child(
                    l.rect(104., l.top(116.), 184., 184.)
                        .child(cover(selected.first().copied(), 184. * l.s)),
                )
                .child(
                    l.text(
                        24.,
                        l.top(316.),
                        344.,
                        12.,
                        SECONDARY,
                        format!("{} songs", selected.len()),
                    )
                    .text_center(),
                )
                .child(self.play_buttons(l, l.top(368.), cx))
                .child(
                    l.rect(24., l.top(446.), 344., (list_bottom - l.top(446.)).max(0.))
                        .id("collection-list")
                        .overflow_y_scroll()
                        .pb(bottom_padding)
                        .children(
                            selected
                                .into_iter()
                                .enumerate()
                                .map(|(i, t)| self.row(t, i, l, cx)),
                        ),
                );
        } else {
            if !search {
                d = d.child(
                    l.button("import", 324., l.top(44.), 48., 48., "plus")
                        .on_click(
                            cx.listener(|this, _, w, cx| this.open_sheet(Sheet::Import, w, cx)),
                        ),
                );
            }
            d = d.child(
                l.rect(24., l.top(108.), 344., 44.)
                    .bg(rgb(SURFACE))
                    .rounded(px(12. * l.s))
                    .child(
                        l.rect(16., 11., 24., 24.)
                            .child(icons::icon("search-muted", l.s)),
                    )
                    .child(
                        l.rect(46., 0., 282., 44.).child(
                            Input::new(&self.search)
                                .appearance(crate::ui::theme::input_appearance())
                                .bg(rgb(SURFACE))
                                .border_0()
                                .text_color(rgb(TEXT))
                                .text_size(px(14. * l.s))
                                .size_full(),
                        ),
                    ),
            );
            let query = self.search.read(cx).value().to_lowercase();
            let filtered = tracks
                .iter()
                .filter(|t| {
                    query.is_empty()
                        || format!("{} {} {}", t.title, t.artist, t.album)
                            .to_lowercase()
                            .contains(&query)
                })
                .collect::<Vec<_>>();
            if search {
                d = d
                    .child(
                        l.text(24., l.top(190.), 344., 18., TEXT, "Songs")
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(
                        l.rect(24., l.top(234.), 344., (list_bottom - l.top(234.)).max(0.))
                            .id("results")
                            .overflow_y_scroll()
                            .pb(bottom_padding)
                            .children(
                                filtered
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, t)| self.row(t, i, l, cx)),
                            ),
                    );
            } else {
                for (i, (tab, label)) in [
                    (LibraryTab::Songs, "Songs"),
                    (LibraryTab::Albums, "Albums"),
                    (LibraryTab::Playlists, "Playlists"),
                ]
                .into_iter()
                .enumerate()
                {
                    d = d.child(
                        l.rect(24. + i as f32 * 112., l.top(174.), 100., 34.)
                            .id(label)
                            .rounded_full()
                            .when(self.library_tab == tab, |d| d.bg(rgb(ACCENT)))
                            .text_size(px(13. * l.s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.library_tab = tab;
                                cx.notify();
                            })),
                    );
                }
                match self.library_tab {
                    LibraryTab::Songs => {
                        d = d.child(self.play_buttons(l, l.top(234.), cx)).child(
                            l.rect(24., l.top(300.), 344., (list_bottom - l.top(300.)).max(0.))
                                .id("songs")
                                .overflow_y_scroll()
                                .pb(bottom_padding)
                                .children(
                                    filtered
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, t)| self.row(t, i, l, cx)),
                                ),
                        );
                    }
                    LibraryTab::Albums => {
                        let mut seen = std::collections::HashSet::new();
                        let albums = filtered
                            .into_iter()
                            .filter(|t| seen.insert((t.album.clone(), t.artist.clone())));
                        d = d.child(
                            l.rect(24., l.top(234.), 344., (list_bottom - l.top(234.)).max(0.))
                                .id("albums")
                                .overflow_y_scroll()
                                .pb(bottom_padding)
                                .child(div().flex().flex_wrap().gap(px(16. * l.s)).children(
                                    albums.enumerate().map(|(i, t)| {
                                        let id = t.id.clone();
                                        div()
                                            .w(px(164. * l.s))
                                            .id(("album-card", i))
                                            .on_click(cx.listener(move |this, _, w, cx| {
                                                this.open_album(&id, w, cx)
                                            }))
                                            .child(cover(Some(t), 164. * l.s))
                                            .child(
                                                div()
                                                    .mt(px(8. * l.s))
                                                    .text_size(px(16. * l.s))
                                                    .child(t.album.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_color(rgb(SECONDARY))
                                                    .text_size(px(12. * l.s))
                                                    .child(t.artist.clone()),
                                            )
                                    }),
                                )),
                        );
                    }
                    LibraryTab::Playlists => {
                        d = d.child(
                            l.rect(24., l.top(258.), 344., 60.)
                                .id("favorites")
                                .flex()
                                .items_center()
                                .gap(px(16. * l.s))
                                .child(cover(tracks.iter().find(|t| t.favorite), 48. * l.s))
                                .child("Favorites")
                                .on_click(cx.listener(|this, _, w, cx| {
                                    this.collection_title = "Favorites".into();
                                    this.collection =
                                        this.library.as_ref().map_or(Vec::new(), |lib| {
                                            lib.tracks
                                                .iter()
                                                .filter(|t| t.favorite)
                                                .map(|t| t.id.clone())
                                                .collect()
                                        });
                                    this.navigate(Page::Collection, w, cx);
                                })),
                        );
                    }
                }
            }
        }
        if !searching_with_keyboard {
            d = d.child(self.mini(l, !collection, cx));
        }
        d
    }
    fn play_buttons(&self, l: Layout, y: f32, cx: &mut Context<Self>) -> Div {
        div().children(
            [
                (false, "Play", "play-accent"),
                (true, "Shuffle", "shuffle-accent"),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (shuffle, label, icon))| {
                l.rect(24. + i as f32 * 180., y, 164., 42.)
                    .id(label)
                    .bg(rgb(SURFACE))
                    .rounded(px(10. * l.s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8. * l.s))
                    .text_color(rgb(ACCENT))
                    .text_size(px(14. * l.s))
                    .child(icons::icon(icon, l.s))
                    .child(label)
                    .on_click(cx.listener(move |this, _, w, cx| this.play_all(shuffle, w, cx)))
            }),
        )
    }
    fn open_album(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(lib) = &self.library {
            if let Some(t) = lib.tracks.iter().find(|t| t.id == id) {
                self.collection_title = t.album.clone();
                self.collection = lib
                    .tracks
                    .iter()
                    .filter(|item| item.album == t.album && item.artist == t.artist)
                    .map(|t| t.id.clone())
                    .collect();
            }
        }
        self.navigate(Page::Collection, w, cx);
    }
    fn editor(&self, l: Layout, cx: &mut Context<Self>) -> Div {
        div()
            .size_full()
            .relative()
            .child(self.header(l, "Edit Lyrics", true, cx))
            .child(l.text(24., l.top(120.), 344., 14., SECONDARY, "Lyrics"))
            .child(
                l.rect(
                    24.,
                    l.top(160.),
                    344.,
                    (l.bottom(730.) - l.top(160.)).clamp(100., 470.),
                )
                .bg(rgb(SURFACE))
                .rounded(px(10. * l.s))
                .child(
                    Input::new(&self.lyrics_editor)
                        .appearance(crate::ui::theme::input_appearance())
                        .bg(rgb(SURFACE))
                        .border_0()
                        .text_color(rgb(TEXT))
                        .text_size(px(14. * l.s))
                        .rows(15)
                        .scrollbar(|bar| {
                            bar.appearance(
                                uic::components::scrollbar::ScrollbarAppearance::default()
                                    .thumb(rgba(0x9295a266).into())
                                    .hover_thumb(rgba(0x9295a299).into())
                                    .dragging_thumb(rgb(SECONDARY).into()),
                            )
                            .w(px(4.))
                        })
                        .size_full(),
                ),
            )
            .child(
                l.rect(24., l.bottom(756.), 344., 46.)
                    .id("save-lyrics")
                    .bg(rgb(ACCENT))
                    .rounded(px(10. * l.s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("Save")
                    .on_click(cx.listener(|this, _, w, cx| {
                        if let Some(id) = this.current.clone() {
                            let text = this.lyrics_editor.read(cx).value().to_string().into_bytes();
                            this.persist(move |lib| lib.set_lyrics(&id, text), cx);
                            this.navigate(Page::NowPlaying, w, cx);
                        }
                    })),
            )
    }
    fn queue_list(&self, l: Layout, cx: &mut Context<Self>) -> AnyElement {
        let Some(ui) = &self.queue_ui else {
            return div().into_any_element();
        };
        let mut list = ReorderableList::new("playback-queue", &ui.order)
            .size_full()
            .enabled(!ui.rows.values().any(|row| row.removing))
            .on_reorder(cx.listener(move |this, event: &ReorderEvent, _, cx| {
                let mut entries: Vec<_> = this.upcoming_queue().into_iter().cloned().collect();
                if entries
                    .get(event.from)
                    .is_some_and(|entry| ElementId::from(entry.key.clone()) == event.id)
                    && event.to < entries.len()
                {
                    let entry = entries.remove(event.from);
                    entries.insert(event.to, entry);
                    let keys: std::collections::HashSet<_> =
                        entries.iter().map(|entry| entry.key.clone()).collect();
                    let mut entries = entries.into_iter();
                    for slot in &mut this.queue {
                        if keys.contains(&slot.key) {
                            *slot = entries.next().unwrap();
                        }
                    }
                    cx.notify();
                }
            }));
        let entries = self.upcoming_queue();
        for entry in entries {
            let Some(track) = self
                .library
                .as_ref()
                .and_then(|lib| lib.tracks.iter().find(|track| track.id == entry.track_id))
            else {
                continue;
            };
            let Some(state) = ui.rows.get(&entry.key) else {
                continue;
            };
            let dragging =
                ui.order.read(cx).dragged_item() == Some(&ElementId::from(entry.key.clone()));
            let play_entry = entry.clone();
            let menu_entry = entry.clone();
            let row = div()
                .relative()
                .w_full()
                .h(px(55. * l.s))
                .when(dragging, |row| row.bg(rgba(0xffffff18)))
                .child(
                    l.rect(0., 0., 48., 48.)
                        .child(cover(Some(track), 48. * l.s)),
                )
                .child(
                    l.text(62., -1., 224., 15., TEXT, track.title.clone())
                        .truncate(),
                )
                .child(
                    l.text(62., 26., 222., 12., TEXT, track.artist.clone())
                        .opacity(0.55)
                        .truncate(),
                )
                .child(
                    l.rect(0., 0., 288., 55.)
                        .id((ElementId::from("queue-play"), entry.key.clone()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.play_queue_entry(&play_entry, cx)
                        })),
                );
            let row = row
                .child(
                    l.rect(302., 12., 26., 28.).opacity(0.3).child(
                        ReorderHandle::new(
                            &ui.order,
                            entry.key.clone(),
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icons::icon("reorder", l.s)),
                        )
                        .size_full(),
                    ),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, _, w, cx| {
                        this.queue_menu_entry = Some(menu_entry.key.clone());
                        this.open_sheet(Sheet::Song(menu_entry.track_id.clone()), w, cx);
                    }),
                );
            let swipe = SwipeActions::new(
                (ElementId::from("queue-swipe"), entry.key.clone()),
                &state.swipe,
                row,
            )
            .right_enabled(false)
            .threshold(px(88. * l.s))
            .dismissed(state.removing.then_some(SwipeDirection::Left))
            .feedback(move |progress, _, _| {
                div().size_full().pb(px(7. * l.s)).child(
                    div()
                        .size_full()
                        .px(px(20. * l.s))
                        .flex()
                        .items_center()
                        .justify_end()
                        .bg(rgb(SURFACE))
                        .opacity(if progress.ready { 1. } else { 0.65 })
                        .child(icons::icon("trash", l.s)),
                )
            });
            list = list.item(entry.key.clone(), swipe);
        }
        list.into_any_element()
    }

    fn open_sheet(&mut self, sheet: Sheet, window: &mut Window, cx: &mut Context<Self>) {
        use uic::components::{
            bottom_sheet::BottomSheet,
            modal::{self, Modal, ModalAppearance},
        };
        if modal::is_open(cx) {
            self.sheet_stack.push(sheet);
            cx.notify();
            return;
        }
        self.sheet_stack = vec![sheet.clone()];
        self.queue_menu_entry = None;
        let view = cx.entity().downgrade();
        let sheet: Modal = BottomSheet::new(move |window, cx| {
            let safe = window.insets().effective();
            let l = Layout {
                s: (window.viewport_size().width - safe.left - safe.right)
                    .as_f32()
                    .min(640.)
                    / 392.,
                h: 0.,
            };
            view.update(cx, |this, cx| {
                let active = this
                    .sheet_stack
                    .last()
                    .cloned()
                    .unwrap_or_else(|| sheet.clone());
                this.sheet_content(active, l, cx).into_any_element()
            })
            .unwrap_or_else(|_| div().into_any_element())
        })
        .animation_duration(Duration::from_millis(260))
        .expandable(true)
        .avoid_safe_area(true)
        .drag_to_dismiss(true)
        .close_on_backdrop(true)
        .bg(rgb(BACKGROUND))
        .text_color(rgb(TEXT))
        .font_family("Noto Sans CJK SC")
        .border_0()
        .into();
        modal::show(
            sheet.appearance(
                ModalAppearance::default()
                    .body_padding_x(px(0.))
                    .body_padding_y(px(0.))
                    .section_borders(false)
                    .backdrop(rgba(0x00000066).into()),
            ),
            window,
            cx,
        );
        window.set_back_enabled(true);
        cx.notify();
    }

    fn sheet_content(&self, sheet: Sheet, l: Layout, cx: &mut Context<Self>) -> Div {
        let nested = self.sheet_stack.len() > 1;
        let offset = if nested && !matches!(sheet, Sheet::Song(_)) {
            48.
        } else {
            0.
        };
        let height = sheet_height(&sheet) + offset;
        let mut body = div()
            .absolute()
            .top(px((-36. + offset) * l.s))
            .left_0()
            .w_full()
            .h(px(height * l.s));
        match sheet {
            Sheet::Import => {
                body = body
                    .child(
                        l.text(24., 36., 344., 22., TEXT, "Add Music")
                            .font_weight(FontWeight::BOLD),
                    )
                    .child(l.button("choose-files", 12., 96., 48., 54., "import-files"))
                    .child(
                        l.rect(64., 96., 304., 54.)
                            .id("import-files-action")
                            .flex()
                            .items_center()
                            .text_size(px(15. * l.s))
                            .child("Choose files")
                            .on_click(cx.listener(|this, _, w, cx| {
                                modal::dismiss(w, cx);
                                this.import(false, cx);
                            })),
                    );
            }
            Sheet::Output => {
                body = body
                    .child(
                        l.text(24., 36., 344., 22., TEXT, "Audio Output")
                            .font_weight(FontWeight::BOLD),
                    )
                    .child(
                        l.rect(28., 104., 28., 28.)
                            .child(icons::icon("device", l.s)),
                    )
                    .child(l.text(70., 104., 270., 16., TEXT, "This phone"))
                    .child(
                        l.rect(64., 174., 268., 36.).child(
                            Slider::new(&self.volume_slider)
                                .label("Volume")
                                .appearance(
                                    SliderAppearance::default()
                                        .active_track(rgb(TEXT).into())
                                        .thumb(rgb(TEXT).into())
                                        .thumb_border(rgb(TEXT).into())
                                        .focus_ring(rgba(0xffffff00).into())
                                        .thumb_size(px(10. * l.s))
                                        .bg(rgb(SURFACE))
                                        .h(px(4. * l.s)),
                                )
                                .size_full(),
                        ),
                    );
            }
            Sheet::Song(id) => {
                if let Some(t) = self
                    .library
                    .as_ref()
                    .and_then(|lib| lib.tracks.iter().find(|t| t.id == id))
                {
                    let cover_x = if nested { 60. } else { 24. };
                    body = body
                        .child(
                            l.rect(cover_x, 36., 48., 48.)
                                .child(cover(Some(t), 48. * l.s)),
                        )
                        .child(
                            l.text(cover_x + 64., 34., 244., 17., TEXT, t.title.clone())
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(l.text(cover_x + 64., 63., 244., 12., SECONDARY, t.artist.clone()));
                }
                let favorite = self
                    .library
                    .as_ref()
                    .and_then(|lib| lib.tracks.iter().find(|t| t.id == id))
                    .is_some_and(|t| t.favorite);
                for (y, height) in [(112., 54.), (178., 108.), (298., 108.)] {
                    body = body.child(
                        l.rect(16., y, 360., height)
                            .bg(rgb(SURFACE))
                            .rounded(px(12. * l.s)),
                    );
                }
                for y in [232., 352.] {
                    body = body.child(l.rect(72., y, 288., 0.5).bg(rgba(0x9295a226)));
                }
                for (i, (label, icon)) in [
                    (
                        if favorite {
                            "Remove from Favorites"
                        } else {
                            "Add to Favorites"
                        },
                        if favorite {
                            "heart-filled"
                        } else {
                            "menu-heart"
                        },
                    ),
                    ("Play Next", "play-next"),
                    ("Add to Queue", "add-queue"),
                    ("Song Info", "info"),
                    ("Import Lyrics", "lyrics"),
                ]
                .into_iter()
                .enumerate()
                {
                    let id = id.clone();
                    body = body.child(
                        l.rect(16., [112., 178., 232., 298., 352.][i], 360., 54.)
                            .id(("song-action", i))
                            .on_click(cx.listener(move |this, _, w, cx| {
                                if i < 3 && this.sheet_stack.len() > 1 {
                                    this.sheet_stack.pop();
                                } else {
                                    modal::dismiss(w, cx);
                                }
                                match i {
                                    0 => this.favorite(id.clone(), cx),
                                    1 => {
                                        let at = this.queue_position().map_or(0, |i| i + 1);
                                        this.queue.insert(at, QueueEntry::new(id.clone()));
                                    }
                                    2 => this.queue.push(QueueEntry::new(id.clone())),
                                    3 => {
                                        this.info_track = Some(id.clone());
                                        this.navigate(Page::SongInfo, w, cx);
                                    }
                                    _ => {
                                        if this.current.as_ref() != Some(&id) {
                                            this.play_track(&id, cx);
                                        }
                                        this.import(true, cx);
                                    }
                                }
                                cx.notify();
                            }))
                            .child(
                                l.rect(14., 13., 28., 28.)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icons::icon(icon, l.s)),
                            )
                            .child(l.text(56., 15., 288., 15., TEXT, label)),
                    );
                }
                let queue_entry = self.queue_menu_entry.clone();
                let in_queue = queue_entry.is_some();
                body = body.child(
                    l.rect(16., 422., 360., 54.)
                        .id("remove-song")
                        .rounded(px(12. * l.s))
                        .bg(rgb(SURFACE))
                        .child(
                            l.rect(14., 13., 28., 28.)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icons::icon("trash", l.s)),
                        )
                        .child(l.text(
                            56.,
                            15.,
                            288.,
                            15.,
                            ACCENT,
                            if in_queue {
                                "Remove from Queue"
                            } else {
                                "Remove from Library"
                            },
                        ))
                        .on_click(cx.listener(move |this, _, w, cx| {
                            if let Some(key) = &queue_entry {
                                this.sheet_stack.pop();
                                this.dismiss_queue_entry(key, cx);
                            } else {
                                this.open_sheet(Sheet::Remove(id.clone()), w, cx);
                            }
                        })),
                );
            }
            Sheet::Remove(id) => {
                body = body
                    .child(
                        l.text(24., 54., 344., 22., TEXT, "Remove Song?")
                            .text_center()
                            .font_weight(FontWeight::BOLD),
                    )
                    .child(
                        l.text(
                            24.,
                            110.,
                            344.,
                            14.,
                            SECONDARY,
                            "The original audio file will be kept.",
                        )
                        .text_center(),
                    )
                    .child(
                        l.rect(204., 238., 164., 46.)
                            .id("confirm-remove")
                            .bg(rgb(ACCENT))
                            .rounded(px(10. * l.s))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("Remove")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                modal::dismiss(w, cx);
                                this.queue.retain(|q| q.track_id != id);
                                if this.current.as_ref() == Some(&id) {
                                    this.player_subscription = None;
                                    this.player = None;
                                    this.current = None;
                                    #[cfg(target_os = "android")]
                                    {
                                        this.system_media =
                                            super::system_media::SystemMedia::default();
                                    }
                                }
                                let id = id.clone();
                                this.persist(
                                    move |lib| {
                                        lib.tracks.retain(|t| t.id != id);
                                        lib.save()
                                    },
                                    cx,
                                );
                            })),
                    );
            }
        }
        div()
            .relative()
            .w_full()
            .h(px((height - 36.) * l.s))
            .child(body.with_animation(
                ("sheet-subpage", self.sheet_stack.len()),
                Animation::new(Duration::from_millis(180)).with_easing(|t| 1. - (1. - t).powi(3)),
                move |body, t| body.left(px((1. - t) * 18. * l.s)).opacity(t),
            ))
            .when(nested, |body| {
                body.child(
                    l.button("sheet-back", 12., 0., 32., 48., "back")
                        .on_click(cx.listener(|this, _, w, cx| this.back(w, cx))),
                )
            })
    }
}
fn sheet_height(sheet: &Sheet) -> f32 {
    match sheet {
        Sheet::Song(_) => 500.,
        Sheet::Output => 250.,
        Sheet::Remove(_) => 308.,
        Sheet::Import => 174.,
    }
}

impl MobileApp {
    fn page_scene(
        &mut self,
        page: Page,
        l: Layout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let insets = window.insets().effective();
        let collapse = if page == Page::NowPlaying {
            self.player_dismiss
                .progress(window.viewport_size().height.as_f32())
        } else {
            0.
        };
        let background = if page == Page::NowPlaying {
            self.playback_background(window, cx)
        } else {
            None
        };
        let content = match page {
            Page::Discover | Page::Radio => {
                div().relative().size_full().child(self.mini(l, true, cx))
            }
            Page::NowPlaying => self.now_playing(l, window, cx),
            Page::EditLyrics => self.editor(l, cx),
            Page::SongInfo => {
                let track = self.library.as_ref().and_then(|lib| {
                    lib.tracks
                        .iter()
                        .find(|t| Some(&t.id) == self.info_track.as_ref())
                });
                div()
                    .child(self.header(l, "Song Info", true, cx))
                    .child(l.text(
                        24.,
                        l.top(130.),
                        344.,
                        18.,
                        TEXT,
                        track.map_or(String::new(), |t| t.title.clone()),
                    ))
                    .child(l.text(
                        24.,
                        l.top(180.),
                        344.,
                        14.,
                        SECONDARY,
                        track.map_or(String::new(), |t| t.artist.clone()),
                    ))
                    .child(l.text(
                        24.,
                        l.top(220.),
                        344.,
                        14.,
                        SECONDARY,
                        track.map_or(String::new(), |t| t.album.clone()),
                    ))
            }
            _ => self.browse(page, l, window, cx),
        };
        let weak = cx.entity().downgrade();
        let drag_height = if self.show_lyrics || self.show_queue {
            l.top(180.)
        } else {
            l.bottom(580.)
        };
        div()
            .relative()
            .size_full()
            .bg(rgb(if collapse > 0. { SURFACE } else { BACKGROUND }))
            .rounded(px(14. * l.s * collapse))
            .when_some(background, |scene, background| {
                scene.child(
                    div()
                        .absolute()
                        .inset_0()
                        .overflow_hidden()
                        .opacity(1. - collapse)
                        .child(background),
                )
            })
            .when(page == Page::NowPlaying, |scene| {
                scene.child(
                    canvas(
                        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                        move |_, hitbox, window, _| {
                            window.on_mouse_event(move |event: &TouchEvent, phase, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.player_touch(
                                        event,
                                        phase,
                                        hitbox.is_hovered(window),
                                        window,
                                        cx,
                                    );
                                });
                            });
                        },
                    )
                    .absolute()
                    .top(insets.top * (1. - collapse))
                    .left_0()
                    .w_full()
                    .when(collapse == 0., |area| area.h(px(drag_height.max(0.) * l.s)))
                    .when(collapse > 0., |area| area.h_full()),
                )
            })
            .child(
                div()
                    .relative()
                    .size_full()
                    .pt(insets.top * (1. - collapse))
                    .pb(insets.bottom * (1. - collapse))
                    .pl(insets.left * (1. - collapse))
                    .pr(insets.right * (1. - collapse))
                    .child(div().relative().size_full().child(content)),
            )
    }
}

impl Render for MobileApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.window_subscription.is_none() {
            self.window_active = window.is_window_active();
            self.window_subscription =
                Some(cx.observe_window_activation(window, |this, window, cx| {
                    this.window_active = window.is_window_active();
                    this.backdrop_sampled_at = None;
                    this.backdrop_frame = None;
                    this.controls_idle_task = None;
                    this.reveal_player_controls(cx);
                    if this.window_active {
                        #[cfg(target_os = "android")]
                        {
                            this.retry_background_playback();
                            this.publish_system_media(true, cx);
                        }
                        cx.notify();
                    }
                }));
        }
        window.set_back_enabled(modal::is_open(cx) || !self.page.is_tab());
        if self.page != Page::NowPlaying {
            self.backdrop_sampled_at = None;
            self.backdrop_frame = None;
            self.controls_idle_task = None;
            self.reveal_player_controls(cx);
        }
        let insets = window.insets().effective();
        let size = window.viewport_size();
        let l = Layout {
            s: (size.width - insets.left - insets.right).as_f32() / 392.,
            h: (size.height - insets.top - insets.bottom).as_f32(),
        };
        let (player_offset, player_moving, player_closed) =
            self.player_dismiss.sample(Instant::now());
        if player_closed {
            self.player_dismiss = motion::PlayerDismiss::default();
            self.page = self.return_page;
            self.show_lyrics = false;
            window.set_back_enabled(!self.page.is_tab());
        }
        if player_moving {
            window.request_animation_frame();
        }
        let transition = self.page_motion.as_ref().and_then(|motion| {
            let (progress, moving) = motion.progress();
            moving.then_some((motion.from, motion.to, progress))
        });
        if transition.is_some() {
            window.request_animation_frame();
        } else {
            self.page_motion = None;
        }
        let mut pages = div().relative().size_full().overflow_hidden();
        if self.page == Page::NowPlaying && self.player_dismiss.active() {
            let below = self.page_scene(self.return_page, l, window, cx);
            let player = self.page_scene(Page::NowPlaying, l, window, cx);
            let progress = (player_offset / size.height.as_f32()).clamp(0., 1.);
            let nav = self.return_page != Page::Collection;
            let target_y = insets.top + px(l.bottom(if nav { 690. } else { 760. }) * l.s);
            pages = pages
                .child(
                    div()
                        .id(("page", self.return_page as usize))
                        .absolute()
                        .inset_0()
                        .child(below),
                )
                .child(
                    div()
                        .id("player-dismiss-backdrop")
                        .absolute()
                        .inset_0()
                        .bg(black().opacity(0.18 * (1. - progress)))
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .id(("page", Page::NowPlaying as usize))
                        .absolute()
                        .left((insets.left + px(16. * l.s)) * progress)
                        .top(target_y * progress)
                        .w(size.width + (px(360. * l.s) - size.width) * progress)
                        .h(size.height + (px(58. * l.s) - size.height) * progress)
                        .overflow_hidden()
                        .child(player),
                );
        } else if let Some((from, to, t)) = transition {
            let outgoing = self.page_scene(from, l, window, cx);
            let incoming = self.page_scene(to, l, window, cx);
            let layer = |id, content| {
                div()
                    .id(id)
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full()
                    .child(content)
            };
            if to == Page::NowPlaying {
                pages = pages
                    .child(layer(("page", from as usize), outgoing))
                    .child(layer(("page", to as usize), incoming).top(size.height * (1. - t)));
            } else if from == Page::NowPlaying {
                pages = pages
                    .child(layer(("page", to as usize), incoming))
                    .child(layer(("page", from as usize), outgoing).top(size.height * t));
            } else if from.is_tab() && to.is_tab() {
                pages = pages
                    .child(layer(("page", from as usize), outgoing))
                    .child(layer(("page", to as usize), incoming).opacity(t));
            } else {
                let back = to.is_tab();
                if back {
                    pages = pages
                        .child(
                            layer(("page", to as usize), incoming)
                                .left(-size.width * 0.22 * (1. - t)),
                        )
                        .child(layer(("page", from as usize), outgoing).left(size.width * t));
                } else {
                    pages = pages
                        .child(
                            layer(("page", from as usize), outgoing).left(-size.width * 0.22 * t),
                        )
                        .child(layer(("page", to as usize), incoming).left(size.width * (1. - t)));
                }
            }
            pages = pages.child(
                div()
                    .id("page-transition-input")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            );
        } else {
            pages = pages.child(
                div()
                    .id(("page", self.page as usize))
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full()
                    .child(self.page_scene(self.page, l, window, cx)),
            );
        }
        div()
            .relative()
            .size_full()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .font_family("Noto Sans CJK SC")
            .when(self.page == Page::NowPlaying, |root| {
                let view = cx.entity().downgrade();
                root.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let touch = view.clone();
                            window.on_mouse_event(move |_: &TouchEvent, phase, _, cx| {
                                if phase.capture() {
                                    let _ =
                                        touch.update(cx, |this, _| this.record_player_activity());
                                }
                            });
                            let mouse = view.clone();
                            window.on_mouse_event(move |_: &MouseDownEvent, phase, _, cx| {
                                if phase.capture() {
                                    let _ =
                                        mouse.update(cx, |this, _| this.record_player_activity());
                                }
                            });
                            window.on_mouse_event(move |_: &ScrollWheelEvent, phase, _, cx| {
                                if phase.capture() {
                                    let _ =
                                        view.update(cx, |this, _| this.record_player_activity());
                                }
                            });
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
            })
            .child(pages)
            .when(
                self.page.is_tab() && window.insets().ime.bottom == px(0.),
                |root| {
                    root.child(
                        div()
                            .id("mobile-navigation-layer")
                            .absolute()
                            .inset_0()
                            .pt(insets.top)
                            .pb(insets.bottom)
                            .pl(insets.left)
                            .pr(insets.right)
                            .child(
                                div()
                                    .relative()
                                    .size_full()
                                    .child(self.nav(self.page, l, cx)),
                            ),
                    )
                },
            )
            .when_some(self.error.clone(), |root, error| {
                root.child(
                    l.rect(24., l.bottom(540.), 344., 48.)
                        .text_size(px(11. * l.s))
                        .text_color(rgb(ACCENT))
                        .child(error),
                )
            })
            .child(modal::layer(cx))
    }
}
