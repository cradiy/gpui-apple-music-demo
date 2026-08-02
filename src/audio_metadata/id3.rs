use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use super::{AudioMetadata, EmbeddedArtwork, clean_text, year_part};

pub(super) fn read_id3v2_metadata(
    mut file: File,
    header: [u8; 10],
) -> std::io::Result<AudioMetadata> {
    let version = header[3];
    if !(3..=4).contains(&version) {
        return Ok(AudioMetadata::default());
    }
    let tag_size = synchsafe_u32(&header[6..10]).min(16 * 1024 * 1024);
    let mut data = vec![0_u8; tag_size as usize];
    file.seek(SeekFrom::Start(10))?;
    file.read_exact(&mut data)?;

    let mut offset = 0;
    let mut metadata = AudioMetadata::default();
    while offset + 10 <= data.len() {
        let frame_id = &data[offset..offset + 4];
        if frame_id.iter().all(|byte| *byte == 0) {
            break;
        }
        let size = if version == 4 {
            synchsafe_u32(&data[offset + 4..offset + 8])
        } else {
            u32::from_be_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ])
        } as usize;
        offset += 10;
        if size == 0 || offset + size > data.len() {
            break;
        }
        match frame_id {
            b"TIT2" => metadata.title = decode_text_frame(&data[offset..offset + size]),
            b"TPE1" => metadata.artist = decode_text_frame(&data[offset..offset + size]),
            b"TALB" => metadata.album = decode_text_frame(&data[offset..offset + size]),
            b"TDRC" | b"TYER" => {
                metadata.year =
                    decode_text_frame(&data[offset..offset + size]).map(|value| year_part(&value));
            }
            b"USLT" => metadata.lyrics = decode_unsynchronized_lyrics(&data[offset..offset + size]),
            b"TXXX" => apply_id3_user_text(&mut metadata, &data[offset..offset + size]),
            b"APIC" => metadata.artwork = decode_attached_picture(&data[offset..offset + size]),
            _ => {}
        }
        offset += size;
    }

    Ok(metadata)
}

fn decode_attached_picture(data: &[u8]) -> Option<EmbeddedArtwork> {
    let (&encoding, content) = data.split_first()?;
    let mime_end = content.iter().position(|byte| *byte == 0)?;
    let mime_type = std::str::from_utf8(&content[..mime_end]).ok()?.to_owned();
    let description = content.get(mime_end + 2..)?; // NUL + picture type.
    let image_start = text_separator_end(description, encoding)?;
    let image = description.get(image_start..)?.to_vec();
    (!image.is_empty()).then_some(EmbeddedArtwork {
        mime_type,
        data: image,
    })
}

fn decode_unsynchronized_lyrics(data: &[u8]) -> Option<String> {
    let (&encoding, content) = data.split_first()?;
    if content.len() <= 3 {
        return None;
    }
    let content = &content[3..];
    let lyric_start = text_separator_end(content, encoding)?;
    decode_text_payload(encoding, &content[lyric_start..])
}

fn apply_id3_user_text(metadata: &mut AudioMetadata, data: &[u8]) {
    let (&encoding, content) = match data.split_first() {
        Some(value) => value,
        None => return,
    };
    let Some(separator) = text_separator_start(content, encoding) else {
        return;
    };
    let description = decode_text_payload(encoding, &content[..separator]).unwrap_or_default();
    let value_start = text_separator_end(content, encoding).unwrap_or(separator);
    let value = decode_text_payload(encoding, &content[value_start..]);
    if matches!(
        description.to_ascii_uppercase().as_str(),
        "LYRICS" | "UNSYNCEDLYRICS" | "UNSYNCED LYRICS"
    ) {
        metadata.lyrics = value;
    }
}

fn decode_text_frame(data: &[u8]) -> Option<String> {
    let (&encoding, content) = data.split_first()?;
    decode_text_payload(encoding, content)
}

fn decode_text_payload(encoding: u8, content: &[u8]) -> Option<String> {
    let value = match encoding {
        0 => content.iter().map(|byte| char::from(*byte)).collect(),
        1 => decode_utf16(content, true)?,
        2 => decode_utf16(content, false)?,
        3 => String::from_utf8_lossy(content).into_owned(),
        _ => return None,
    };
    clean_text(value)
}

fn decode_utf16(data: &[u8], allow_bom: bool) -> Option<String> {
    let (big_endian, data) = if allow_bom && data.len() >= 2 {
        match &data[..2] {
            [0xfe, 0xff] => (true, &data[2..]),
            [0xff, 0xfe] => (false, &data[2..]),
            _ => (false, data),
        }
    } else {
        (true, data)
    };
    let units = data
        .chunks_exact(2)
        .map(|chunk| {
            if big_endian {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_le_bytes([chunk[0], chunk[1]])
            }
        })
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    String::from_utf16(&units).ok()
}

fn text_separator_start(content: &[u8], encoding: u8) -> Option<usize> {
    if encoding == 1 || encoding == 2 {
        content.windows(2).position(|window| window == [0, 0])
    } else {
        content.iter().position(|byte| *byte == 0)
    }
}

fn text_separator_end(content: &[u8], encoding: u8) -> Option<usize> {
    text_separator_start(content, encoding)
        .map(|index| index + if encoding == 1 || encoding == 2 { 2 } else { 1 })
}

fn synchsafe_u32(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .take(4)
        .fold(0_u32, |value, byte| (value << 7) | u32::from(byte & 0x7f))
}
