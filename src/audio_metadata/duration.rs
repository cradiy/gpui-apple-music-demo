use std::{fs::File, path::Path};

use symphonia::core::{
    formats::{FormatOptions, TrackType, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    units::Timestamp,
};

/// Read the audio track's duration without decoding or opening an audio device.
/// Missing or unsupported duration information stays unknown instead of becoming zero.
pub fn read_audio_duration(path: &Path) -> Option<i64> {
    let file = File::open(path).ok()?;
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .ok()?;
    let track = format.default_track(TrackType::Audio)?;
    let ticks = i64::try_from(track.duration?.get()).ok()?;
    let time = track.time_base?.calc_time(Timestamp::new(ticks))?;
    let millis = i64::try_from(time.as_nanos() / 1_000_000).ok()?;
    (millis > 0).then_some(millis)
}
