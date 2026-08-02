mod id3;
mod mp4;

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use id3::read_id3v2_metadata;
use mp4::read_mp4_metadata;

#[derive(Clone, Debug, Default)]
pub struct AudioMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<String>,
    pub lyrics: Option<String>,
    pub artwork: Option<EmbeddedArtwork>,
}

#[derive(Clone, Debug)]
pub struct EmbeddedArtwork {
    pub mime_type: String,
    pub data: Vec<u8>,
}

pub fn read_audio_metadata(path: &Path) -> std::io::Result<AudioMetadata> {
    let mut file = File::open(path)?;
    let mut signature = [0_u8; 10];
    let read = file.read(&mut signature)?;
    file.seek(SeekFrom::Start(0))?;

    if read >= 4 && &signature[..4] == b"fLaC" {
        return read_flac_metadata(file);
    }
    if read >= 4 && &signature[..4] == b"OggS" {
        return read_ogg_metadata(file);
    }
    if read >= 4 && &signature[..4] == b"RIFF" {
        return read_wav_metadata(file);
    }
    if read >= 8 && &signature[4..8] == b"ftyp" {
        return read_mp4_metadata(file);
    }
    if read >= 3 && &signature[..3] == b"ID3" {
        return read_id3v2_metadata(file, signature);
    }

    Ok(AudioMetadata::default())
}

fn read_ogg_metadata(file: File) -> std::io::Result<AudioMetadata> {
    let mut data = Vec::new();
    file.take(2 * 1024 * 1024).read_to_end(&mut data)?;
    if let Some(index) = find_bytes(&data, b"\x03vorbis") {
        return Ok(parse_vorbis_comment(&data[index + 7..]));
    }
    if let Some(index) = find_bytes(&data, b"OpusTags") {
        return Ok(parse_vorbis_comment(&data[index + 8..]));
    }
    Ok(AudioMetadata::default())
}

fn read_wav_metadata(file: File) -> std::io::Result<AudioMetadata> {
    let mut data = Vec::new();
    file.take(2 * 1024 * 1024).read_to_end(&mut data)?;
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Ok(AudioMetadata::default());
    }

    let mut metadata = AudioMetadata::default();
    let mut offset = 12;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = read_u32_le_at(&data, offset + 4).unwrap_or(0) as usize;
        offset += 8;
        let end = offset.saturating_add(size);
        if end > data.len() {
            break;
        }
        if id == b"LIST" && size >= 4 && &data[offset..offset + 4] == b"INFO" {
            parse_wav_info(&mut metadata, &data[offset + 4..end]);
        }
        offset = end + (size % 2);
    }
    Ok(metadata)
}

fn parse_wav_info(metadata: &mut AudioMetadata, data: &[u8]) {
    let mut offset = 0;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = read_u32_le_at(data, offset + 4).unwrap_or(0) as usize;
        offset += 8;
        let end = offset.saturating_add(size);
        if end > data.len() {
            break;
        }
        let value = clean_text(String::from_utf8_lossy(&data[offset..end]).into_owned());
        match id {
            b"INAM" => metadata.title = value,
            b"IART" => metadata.artist = value,
            b"IPRD" => metadata.album = value,
            b"ICRD" | b"IDIT" => metadata.year = value.map(|value| year_part(&value)),
            b"ILYR" => metadata.lyrics = value,
            _ => {}
        }
        offset = end + (size % 2);
    }
}

fn read_flac_metadata(mut file: File) -> std::io::Result<AudioMetadata> {
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != b"fLaC" {
        return Ok(AudioMetadata::default());
    }

    let mut metadata = AudioMetadata::default();
    loop {
        let mut header = [0_u8; 4];
        if file.read_exact(&mut header).is_err() {
            return Ok(metadata);
        }
        let is_last = header[0] & 0x80 != 0;
        let block_type = header[0] & 0x7f;
        let length = u32::from_be_bytes([0, header[1], header[2], header[3]]).min(16 * 1024 * 1024);

        if block_type == 4 {
            let mut data = vec![0_u8; length as usize];
            file.read_exact(&mut data)?;
            merge_metadata(&mut metadata, parse_vorbis_comment(&data));
        } else if block_type == 6 {
            let mut data = vec![0_u8; length as usize];
            file.read_exact(&mut data)?;
            metadata.artwork = parse_flac_picture(&data);
        } else {
            file.seek(SeekFrom::Current(length as i64))?;
        }
        if is_last {
            return Ok(metadata);
        }
    }
}

fn parse_flac_picture(data: &[u8]) -> Option<EmbeddedArtwork> {
    let mut offset = 4; // Picture type.
    let mime_length = read_u32_be_at(data, offset)? as usize;
    offset += 4;
    let mime_end = offset.checked_add(mime_length)?;
    let mime_type = std::str::from_utf8(data.get(offset..mime_end)?)
        .ok()?
        .to_owned();
    offset = mime_end;
    let description_length = read_u32_be_at(data, offset)? as usize;
    offset = offset.checked_add(4 + description_length + 16)?; // Description + dimensions.
    let image_length = read_u32_be_at(data, offset)? as usize;
    offset += 4;
    let image_end = offset.checked_add(image_length)?;
    let image = data.get(offset..image_end)?.to_vec();
    (!image.is_empty()).then_some(EmbeddedArtwork {
        mime_type,
        data: image,
    })
}

fn parse_vorbis_comment(data: &[u8]) -> AudioMetadata {
    let mut offset = 0;
    let Some(vendor_len) = read_le_u32(data, &mut offset) else {
        return AudioMetadata::default();
    };
    offset = offset.saturating_add(vendor_len as usize);
    let Some(count) = read_le_u32(data, &mut offset) else {
        return AudioMetadata::default();
    };

    let mut metadata = AudioMetadata::default();
    for _ in 0..count.min(512) {
        let Some(length) = read_le_u32(data, &mut offset) else {
            break;
        };
        let end = offset.saturating_add(length as usize);
        if end > data.len() {
            break;
        }
        if let Ok(comment) = std::str::from_utf8(&data[offset..end]) {
            apply_comment(&mut metadata, comment);
        }
        offset = end;
    }
    metadata
}

fn apply_comment(metadata: &mut AudioMetadata, comment: &str) {
    let Some((key, value)) = comment.split_once('=') else {
        return;
    };
    let value = clean_text(value.to_owned());
    match key.to_ascii_uppercase().as_str() {
        "TITLE" => metadata.title = value,
        "ARTIST" | "ALBUMARTIST" => metadata.artist = value,
        "ALBUM" => metadata.album = value,
        "DATE" | "YEAR" => metadata.year = value.map(|value| year_part(&value)),
        "LYRICS" | "UNSYNCEDLYRICS" | "UNSYNCED LYRICS" => metadata.lyrics = value,
        _ => {}
    }
}

fn merge_metadata(target: &mut AudioMetadata, source: AudioMetadata) {
    if target.title.is_none() {
        target.title = source.title;
    }
    if target.artist.is_none() {
        target.artist = source.artist;
    }
    if target.album.is_none() {
        target.album = source.album;
    }
    if target.year.is_none() {
        target.year = source.year;
    }
    if target.lyrics.is_none() {
        target.lyrics = source.lyrics;
    }
    if target.artwork.is_none() {
        target.artwork = source.artwork;
    }
}

fn clean_text(value: String) -> Option<String> {
    let value = value
        .trim_matches(char::from(0))
        .trim()
        .replace('\u{0}', " / ");
    (!value.is_empty()).then_some(value)
}

fn year_part(value: &str) -> String {
    value.chars().take(4).collect()
}

fn read_le_u32(data: &[u8], offset: &mut usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes = data.get(*offset..end)?;
    *offset = end;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u32_le_at(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u32_be_at(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|part| part == needle)
}
