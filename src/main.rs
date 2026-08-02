use gpui_apple_music_demo::*;
use std::{path::PathBuf, process::ExitCode};

use app::AppleMusicApp;
use audio_metadata::read_audio_metadata;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_media::MediaSource;
use gpui_platform::application;
use state::TrackInfo;
use uic::assets::LucideAssets;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: gpui-apple-music-demo <audio-file>");
        return ExitCode::from(2);
    };
    let source = match MediaSource::from_path(&path) {
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
                    titlebar: None,
                    ..Default::default()
                },
                move |w, cx| cx.new(|cx| AppleMusicApp::new(source, track, cx, w)),
            )
            .expect("failed to open Apple Music demo");
            cx.activate(true);
        });
    ExitCode::SUCCESS
}
