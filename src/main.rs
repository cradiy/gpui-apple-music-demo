use gpui_apple_music_demo::*;
use std::{path::PathBuf, process::ExitCode};

use app::AppleMusicApp;
use audio_metadata::read_audio_metadata;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, point, px, size,
};
use gpui_media::{AudioPlayer, AudioPlayerOptions, AudioSource};
use gpui_platform::application;
use state::TrackInfo;
use std::time::Duration;
use uic::assets::LucideAssets;

fn titlebar() -> Option<TitlebarOptions> {
    #[cfg(target_os = "macos")]
    {
        Some(TitlebarOptions {
            appears_transparent: true,
            traffic_light_position: Some(point(px(34.), px(34.))),
            ..Default::default()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: gpui-apple-music-demo <audio-file>");
        return ExitCode::from(2);
    };
    let source = match AudioSource::from_path(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("Unable to open {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let metadata = match read_audio_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!(
                "Unable to read embedded metadata from {}: {error}",
                path.display()
            );
            Default::default()
        }
    };
    let track = TrackInfo::from_metadata(&path, metadata);

    if let Err(error) = gpui_media::init() {
        eprintln!("Unable to initialize audio playback: {error}");
        return ExitCode::FAILURE;
    }

    application()
        .with_assets(LucideAssets::new())
        .run(move |cx: &mut App| {
            uic::components::input::init(cx);
            let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
            let source = source.clone();
            let track = track.clone();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: titlebar(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                move |window, cx| {
                    gpui_apple_music_demo::window::remove_frame(window);
                    let player = cx.new(|cx| {
                        AudioPlayer::new(
                            source,
                            AudioPlayerOptions {
                                timeline_update_interval: Duration::from_millis(33),
                                ..AudioPlayerOptions::default()
                            },
                            cx,
                        )
                        .expect("failed to create audio playback session")
                    });
                    cx.new(|cx| AppleMusicApp::new(player, track, cx, window))
                },
            )
            .expect("failed to open Apple Music demo");
            cx.activate(true);
        });
    ExitCode::SUCCESS
}
