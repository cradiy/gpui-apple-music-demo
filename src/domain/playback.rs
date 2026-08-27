use gpui::SharedString;
use gpui_media::{PlaybackState as MediaPlaybackState, PlaybackTimeline};

use super::track::TrackInfo;

#[derive(Clone)]
pub struct PlaybackState {
    pub status: MediaPlaybackState,
    pub timeline: PlaybackTimeline,
    pub muted: bool,
    pub volume: f64,
    pub track: TrackInfo,
    pub error: Option<SharedString>,
}

impl PlaybackState {
    pub fn new(track: TrackInfo) -> Self {
        Self {
            status: MediaPlaybackState::Loading,
            timeline: PlaybackTimeline::default(),
            muted: false,
            volume: 1.,
            track,
            error: None,
        }
    }

    pub fn empty() -> Self {
        Self {
            status: MediaPlaybackState::Paused,
            timeline: PlaybackTimeline::default(),
            muted: false,
            volume: 1.,
            track: TrackInfo::empty(),
            error: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        matches!(
            self.status,
            MediaPlaybackState::Loading | MediaPlaybackState::Playing
        )
    }
}
