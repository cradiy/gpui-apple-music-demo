use flexi_logger::{
    Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming, colored_detailed_format,
    detailed_format,
};
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, px, size,
};
use gpui_apple_music_demo::{
    app::MusicApp, resources::assets::AppAssets, services::config::LoadedClientConfig,
    services::session::SessionStore,
};
use std::{fs, process::ExitCode};

fn titlebar() -> Option<TitlebarOptions> {
    #[cfg(target_os = "macos")]
    {
        Some(TitlebarOptions {
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(34.), px(34.))),
            ..Default::default()
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    configure_audio_identity();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start the application runtime")
        .block_on(run())
}

#[cfg(target_os = "linux")]
fn configure_audio_identity() {
    const PROPERTIES: &str = r#"application.name = "Romus demo"
application.id = "romus demo"
application.icon-name = "multimedia-player"
node.description = "Romus demo"
media.name = "Music"
media.role = "Music""#;

    let configured = std::env::var("PIPEWIRE_ALSA").unwrap_or_default();
    let configured = configured.trim();
    let configured = configured
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(configured);
    // SAFETY: Called from main before the runtime, logger, or audio threads are started.
    unsafe {
        std::env::set_var("PIPEWIRE_ALSA", format!("{PROPERTIES}\n{configured}"));
    }
}

async fn run() -> ExitCode {
    let loaded_config = match LoadedClientConfig::load_default() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Unable to load Romus Demo configuration: {error:#}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = initialize_logger(&loaded_config) {
        eprintln!("Unable to initialize Romus Demo logging: {error:#}");
        return ExitCode::FAILURE;
    }
    log::info!(
        "loaded local player config from {}",
        loaded_config.path.display()
    );
    let session_store = match SessionStore::load_default() {
        Ok(store) => store,
        Err(error) => {
            eprintln!("Unable to initialize Romus Demo session storage: {error:#}");
            return ExitCode::FAILURE;
        }
    };
    gpui_platform::application()
        .with_assets(AppAssets::new())
        .run(move |cx: &mut App| {
            gpui_tokio::init(cx);
            cx.set_global(loaded_config.clone());
            cx.set_global(session_store.clone());
            uic::init(cx);
            gpui_apple_music_demo::ui::components::feedback::init(cx);
            let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: titlebar(),
                    window_background: WindowBackgroundAppearance::Transparent,
                    app_id: Some("romus demo".to_owned()),
                    ..Default::default()
                },
                move |window, cx| {
                    window.set_window_title("Romus Demo");
                    gpui_apple_music_demo::window::remove_frame(window);
                    cx.new(|cx| MusicApp::new(window, cx))
                },
            )
            .expect("failed to open Romus Demo");
            cx.activate(true);
        });
    ExitCode::SUCCESS
}

fn initialize_logger(config: &LoadedClientConfig) -> anyhow::Result<()> {
    let log_directory = config.log_directory()?;
    fs::create_dir_all(&log_directory)?;
    let duplicate = if config.config.logging.console {
        Duplicate::All
    } else {
        Duplicate::None
    };
    Logger::try_with_env_or_str(&config.config.logging.filter)?
        .log_to_file(
            FileSpec::default()
                .directory(log_directory)
                .basename("romus-demo"),
        )
        .duplicate_to_stdout(duplicate)
        .format_for_stdout(colored_detailed_format)
        .format_for_files(detailed_format)
        .set_palette("196;208;40;39;244".to_string())
        .rotate(
            Criterion::Size(config.config.logging.max_file_size_bytes),
            Naming::Numbers,
            Cleanup::KeepLogFiles(config.config.logging.retained_files),
        )
        .start()?;
    Ok(())
}
