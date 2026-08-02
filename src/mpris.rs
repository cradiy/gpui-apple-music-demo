use std::{
    collections::HashMap,
    process,
    sync::{Arc, RwLock, mpsc},
    thread,
    time::Duration,
};

use async_channel::{Receiver, Sender};
use zbus::{
    object_server::SignalEmitter,
    zvariant::{ObjectPath, OwnedValue, Str, Value},
};

use crate::state::PlaybackState;

const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const TRACK_PATH: &str = "/org/mpris/MediaPlayer2/track/1";

#[derive(Clone, Debug)]
pub enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Seek(i64),
    SetPosition(i64),
    SetVolume(f64),
    Next,
    Previous,
}

#[derive(Clone, Debug, PartialEq)]
struct MprisSnapshot {
    playback_status: String,
    title: String,
    artist: String,
    album: Option<String>,
    art_url: Option<String>,
    position_micros: i64,
    duration_micros: Option<i64>,
    volume: f64,
}

impl MprisSnapshot {
    fn from_playback(playback: &PlaybackState) -> Self {
        let playback_status = if playback.is_playing() {
            "Playing"
        } else if matches!(playback.status, gpui_media::PlaybackState::Ended) {
            "Stopped"
        } else {
            "Paused"
        };
        Self {
            playback_status: playback_status.to_owned(),
            title: playback.track.title.to_string(),
            artist: playback.track.artist.to_string(),
            album: playback.track.album.as_ref().map(ToString::to_string),
            art_url: playback
                .track
                .artwork
                .as_ref()
                .map(|artwork| artwork.url.to_string()),
            position_micros: duration_to_micros(playback.timeline.position()),
            duration_micros: playback.timeline.duration().map(duration_to_micros),
            volume: if playback.muted { 0. } else { playback.volume },
        }
    }
}

pub struct MprisBridge {
    commands: Receiver<MprisCommand>,
    updates: Sender<MprisSnapshot>,
}

impl MprisBridge {
    pub fn start(playback: &PlaybackState) -> Result<Self, String> {
        let snapshot = Arc::new(RwLock::new(MprisSnapshot::from_playback(playback)));
        let (commands_tx, commands_rx) = async_channel::unbounded();
        let (updates_tx, updates_rx) = async_channel::unbounded();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let service_name = format!(
            "org.mpris.MediaPlayer2.gpui_apple_music_demo.instance{}",
            process::id()
        );

        thread::Builder::new()
            .name("gpui-mpris".to_owned())
            .spawn(move || {
                zbus::block_on(run_service(
                    service_name,
                    snapshot,
                    commands_tx,
                    updates_rx,
                    ready_tx,
                ));
            })
            .map_err(|error| format!("failed to spawn MPRIS service: {error}"))?;

        ready_rx
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| "timed out while connecting MPRIS to the session bus".to_owned())??;

        Ok(Self {
            commands: commands_rx,
            updates: updates_tx,
        })
    }

    pub fn commands(&self) -> Receiver<MprisCommand> {
        self.commands.clone()
    }

    pub fn publish(&self, playback: &PlaybackState) {
        let _ = self
            .updates
            .try_send(MprisSnapshot::from_playback(playback));
    }
}

async fn run_service(
    service_name: String,
    snapshot: Arc<RwLock<MprisSnapshot>>,
    commands: Sender<MprisCommand>,
    updates: Receiver<MprisSnapshot>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let builder = match zbus::connection::Builder::session()
        .and_then(|builder| builder.name(service_name))
        .and_then(|builder| builder.serve_at(OBJECT_PATH, MprisRoot))
        .and_then(|builder| {
            builder.serve_at(
                OBJECT_PATH,
                MprisPlayer {
                    snapshot: snapshot.clone(),
                    commands,
                },
            )
        }) {
        Ok(builder) => builder,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let connection = match builder.build().await {
        Ok(connection) => connection,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    let _ = ready.send(Ok(()));

    while let Ok(next) = updates.recv().await {
        let previous = snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        *snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = next.clone();

        let Ok(interface) = connection
            .object_server()
            .interface::<_, MprisPlayer>(OBJECT_PATH)
            .await
        else {
            continue;
        };
        let player = interface.get().await;
        if previous.playback_status != next.playback_status {
            let _ = player
                .playback_status_changed(interface.signal_emitter())
                .await;
        }
        if previous.title != next.title
            || previous.artist != next.artist
            || previous.album != next.album
            || previous.art_url != next.art_url
            || previous.duration_micros != next.duration_micros
        {
            let _ = player.metadata_changed(interface.signal_emitter()).await;
        }
        if (previous.volume - next.volume).abs() > f64::EPSILON {
            let _ = player.volume_changed(interface.signal_emitter()).await;
        }
    }
}

struct MprisRoot;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    fn raise(&self) {}

    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn fullscreen(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_set_fullscreen(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        "GPUI Apple Music Demo"
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        "gpui-apple-music-demo"
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<&str> {
        vec!["file"]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<&str> {
        vec![
            "audio/mpeg",
            "audio/mp4",
            "audio/flac",
            "audio/ogg",
            "audio/wav",
        ]
    }
}

struct MprisPlayer {
    snapshot: Arc<RwLock<MprisSnapshot>>,
    commands: Sender<MprisCommand>,
}

impl MprisPlayer {
    fn send(&self, command: MprisCommand) {
        let _ = self.commands.try_send(command);
    }

    fn snapshot(&self) -> MprisSnapshot {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    fn next(&self) {
        self.send(MprisCommand::Next);
    }

    fn previous(&self) {
        self.send(MprisCommand::Previous);
    }

    fn pause(&self) {
        self.send(MprisCommand::Pause);
    }

    fn play_pause(&self) {
        self.send(MprisCommand::PlayPause);
    }

    fn stop(&self) {
        self.send(MprisCommand::Stop);
    }

    fn play(&self) {
        self.send(MprisCommand::Play);
    }

    fn seek(&self, offset: i64) {
        self.send(MprisCommand::Seek(offset));
    }

    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        if track_id.as_str() == TRACK_PATH {
            self.send(MprisCommand::SetPosition(position));
        }
    }

    fn open_uri(&self, _uri: &str) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.snapshot().playback_status
    }

    #[zbus(property)]
    fn loop_status(&self) -> &str {
        "None"
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let snapshot = self.snapshot();
        let mut metadata = HashMap::new();
        metadata.insert(
            "mpris:trackid".to_owned(),
            ObjectPath::try_from(TRACK_PATH)
                .expect("valid MPRIS track path")
                .into(),
        );
        metadata.insert("xesam:title".to_owned(), Str::from(snapshot.title).into());
        metadata.insert(
            "xesam:artist".to_owned(),
            owned_value(Value::from(vec![Str::from(snapshot.artist)])),
        );
        if let Some(album) = snapshot.album {
            metadata.insert("xesam:album".to_owned(), Str::from(album).into());
        }
        if let Some(art_url) = snapshot.art_url {
            metadata.insert("mpris:artUrl".to_owned(), Str::from(art_url).into());
        }
        if let Some(length) = snapshot.duration_micros {
            metadata.insert("mpris:length".to_owned(), length.into());
        }
        metadata
    }

    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        self.send(MprisCommand::SetVolume(volume.clamp(0., 1.)));
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.snapshot().volume
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.snapshot().position_micros
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;
}

fn owned_value(value: Value<'_>) -> OwnedValue {
    value.try_into().expect("static MPRIS metadata value")
}

fn duration_to_micros(duration: Duration) -> i64 {
    duration.as_micros().min(i64::MAX as u128) as i64
}
