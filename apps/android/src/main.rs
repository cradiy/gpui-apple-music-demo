use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_apple_music_demo::{mobile::MobileApp, resources::assets::AppAssets};

#[gpui_platform::main(android_log_level = log::LevelFilter::Info)]
fn main() {
    #[cfg(not(target_os = "android"))]
    log::set_max_level(if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    });
    gpui_platform::application()
        .with_assets(AppAssets::new())
        .run(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(392.), px(852.)),
                    cx,
                ))),
                app_id: Some("dev.romus.demo".to_owned()),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                window.set_window_title("Romus demo");
                let view = cx.new(MobileApp::new);
                window.on_system_back(
                    cx,
                    window.handler_for(&view, |view, window, cx| view.back(window, cx)),
                );
                view
            })
            .expect("failed to open Romus demo");
        });
}
