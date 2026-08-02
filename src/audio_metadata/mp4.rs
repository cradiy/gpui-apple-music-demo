use std::{fs::File, io::Read};

use super::{
    AudioMetadata, EmbeddedArtwork, clean_text, merge_metadata, read_u32_be_at, year_part,
};

pub(super) fn read_mp4_metadata(file: File) -> std::io::Result<AudioMetadata> {
    let mut data = Vec::new();
    file.take(8 * 1024 * 1024).read_to_end(&mut data)?;
    Ok(parse_mp4_atoms(&data, 0))
}

fn parse_mp4_atoms(data: &[u8], depth: usize) -> AudioMetadata {
    if depth > 8 {
        return AudioMetadata::default();
    }
    let mut metadata = AudioMetadata::default();
    let mut offset = 0;
    while offset + 8 <= data.len() {
        let size32 = read_u32_be_at(data, offset).unwrap_or(0) as usize;
        let atom_type = &data[offset + 4..offset + 8];
        let (header_size, atom_size) = if size32 == 1 && offset + 16 <= data.len() {
            let size = u64::from_be_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
                data[offset + 12],
                data[offset + 13],
                data[offset + 14],
                data[offset + 15],
            ]) as usize;
            (16, size)
        } else {
            (8, size32)
        };
        if atom_size < header_size {
            break;
        }
        let end = offset.saturating_add(atom_size);
        if end > data.len() {
            break;
        }
        let payload = &data[offset + header_size..end];
        if atom_type == b"covr" {
            metadata.artwork = parse_mp4_artwork(payload);
        } else if let Some(value) = parse_mp4_text_atom(atom_type, payload) {
            apply_mp4_value(&mut metadata, atom_type, value);
        } else if is_mp4_container(atom_type) {
            let nested = if atom_type == b"meta" && payload.len() > 4 {
                parse_mp4_atoms(&payload[4..], depth + 1)
            } else {
                parse_mp4_atoms(payload, depth + 1)
            };
            merge_metadata(&mut metadata, nested);
        }
        offset = end;
    }
    metadata
}

fn parse_mp4_artwork(payload: &[u8]) -> Option<EmbeddedArtwork> {
    let mut offset = 0;
    while offset + 16 <= payload.len() {
        let size = read_u32_be_at(payload, offset)? as usize;
        if size < 16 || offset + size > payload.len() {
            break;
        }
        if &payload[offset + 4..offset + 8] == b"data" {
            let data_type = read_u32_be_at(payload, offset + 8)? & 0x00ff_ffff;
            let image = payload[offset + 16..offset + size].to_vec();
            let mime_type = match data_type {
                13 => "image/jpeg",
                14 => "image/png",
                _ if image.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
                _ if image.starts_with(&[0xff, 0xd8, 0xff]) => "image/jpeg",
                _ => "application/octet-stream",
            };
            return (!image.is_empty()).then_some(EmbeddedArtwork {
                mime_type: mime_type.to_owned(),
                data: image,
            });
        }
        offset += size;
    }
    None
}

fn parse_mp4_text_atom(atom_type: &[u8], payload: &[u8]) -> Option<String> {
    if !is_mp4_text_atom(atom_type) {
        return None;
    }
    let mut offset = 0;
    while offset + 16 <= payload.len() {
        let size = read_u32_be_at(payload, offset)? as usize;
        if size < 16 || offset + size > payload.len() {
            break;
        }
        if &payload[offset + 4..offset + 8] == b"data" {
            return clean_text(
                String::from_utf8_lossy(&payload[offset + 16..offset + size]).into_owned(),
            );
        }
        offset += size;
    }
    None
}

fn apply_mp4_value(metadata: &mut AudioMetadata, atom_type: &[u8], value: String) {
    let value = clean_text(value);
    match atom_type {
        [0xa9, b'n', b'a', b'm'] => metadata.title = value,
        [0xa9, b'A', b'R', b'T'] | b"aART" => metadata.artist = value,
        [0xa9, b'a', b'l', b'b'] => metadata.album = value,
        [0xa9, b'd', b'a', b'y'] => metadata.year = value.map(|value| year_part(&value)),
        [0xa9, b'l', b'y', b'r'] => metadata.lyrics = value,
        _ => {}
    }
}

fn is_mp4_container(atom_type: &[u8]) -> bool {
    matches!(atom_type, b"moov" | b"udta" | b"meta" | b"ilst")
}

fn is_mp4_text_atom(atom_type: &[u8]) -> bool {
    matches!(
        atom_type,
        [0xa9, b'n', b'a', b'm']
            | [0xa9, b'A', b'R', b'T']
            | b"aART"
            | [0xa9, b'a', b'l', b'b']
            | [0xa9, b'd', b'a', b'y']
            | [0xa9, b'l', b'y', b'r']
    )
}
