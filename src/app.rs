pub mod music_shell;

use gpui::{
    AppContext, Entity, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder,
};
use uic::components::{context_menu, modal};

use crate::services::config::LoadedClientConfig;
use music_shell::MusicShell;

pub struct MusicApp {
    music_shell: Entity<MusicShell>,
}

impl MusicApp {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let music_shell = cx.new(|cx| MusicShell::new(cx, window));
        music_shell.update(cx, |shell, cx| shell.signed_in(cx));

        Self { music_shell }
    }
}

impl Render for MusicApp {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let config = &cx.global::<LoadedClientConfig>().config;
        let modal_layer = modal::layer(cx);
        div()
            .size_full()
            .when_some(config.fonts.font_family.clone(), |this, family_name| {
                this.font_family(family_name)
            })
            .child(self.music_shell.clone())
            .child(context_menu::layer(cx))
            .child(modal_layer)
    }
}
