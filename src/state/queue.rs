use crate::services::api::TrackResponse;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum QueueOrigin {
    Album { id: String, name: String },
    Playlist { id: String, name: String },
}

impl QueueOrigin {
    fn name(&self) -> &str {
        match self {
            Self::Album { name, .. } | Self::Playlist { name, .. } => name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub key: u64,
    pub track: TrackResponse,
}

#[derive(Clone, Debug, Default)]
pub struct PlayQueue {
    pub items: Vec<QueueItem>,
    pub index: Option<usize>,
    pub origin: Option<QueueOrigin>,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    next_key: u64,
}

impl PlayQueue {
    pub fn current(&self) -> Option<&TrackResponse> {
        Some(&self.items[self.index?].track)
    }

    pub fn current_key(&self) -> Option<u64> {
        Some(self.items[self.index?].key)
    }

    pub fn has_next(&self) -> bool {
        self.index.is_some_and(|index| index + 1 < self.items.len())
    }

    pub fn has_previous(&self) -> bool {
        self.index.is_some_and(|index| index > 0)
    }

    pub fn playing_from(&self) -> Option<&str> {
        if self.items.is_empty() {
            return None;
        }
        if let Some(origin) = &self.origin {
            return Some(origin.name());
        }
        self.unified_album_name()
    }

    pub fn play_from(
        &mut self,
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
        origin: Option<QueueOrigin>,
    ) {
        if tracks.is_empty() {
            self.clear();
            self.shuffle = shuffle;
            return;
        }
        let start = start.min(tracks.len() - 1);
        self.shuffle = shuffle;
        self.origin = origin;
        if shuffle {
            let current = tracks[start].clone();
            let mut rest = tracks
                .into_iter()
                .enumerate()
                .filter(|(index, _)| *index != start)
                .map(|(_, track)| track)
                .collect::<Vec<_>>();
            shuffle_tracks(&mut rest);
            let mut items = Vec::with_capacity(rest.len() + 1);
            items.push(self.alloc_item(current));
            items.extend(rest.into_iter().map(|track| self.alloc_item(track)));
            self.items = items;
            self.index = Some(0);
            return;
        }
        self.items = tracks
            .into_iter()
            .map(|track| self.alloc_item(track))
            .collect();
        self.index = Some(start);
    }

    pub fn restore(
        &mut self,
        tracks: Vec<TrackResponse>,
        index: usize,
        origin: Option<QueueOrigin>,
        repeat: RepeatMode,
        shuffle: bool,
    ) {
        self.clear();
        self.repeat = repeat;
        self.shuffle = shuffle;
        if tracks.is_empty() {
            return;
        }
        self.items = tracks
            .into_iter()
            .map(|track| self.alloc_item(track))
            .collect();
        self.index = Some(index.min(self.items.len() - 1));
        self.origin = origin;
    }

    pub fn play_next(&mut self, track: TrackResponse) {
        self.play_next_many(vec![track]);
    }

    pub fn play_next_many(&mut self, tracks: Vec<TrackResponse>) {
        if tracks.is_empty() {
            return;
        }
        if self.index.is_none() {
            self.play_from(tracks, 0, false, None);
            return;
        }
        for track in &tracks {
            self.note_inserted(track);
        }
        let mut items = Vec::with_capacity(tracks.len());
        for track in tracks {
            items.push(self.alloc_item(track));
        }
        let insert_at = self.index.map_or(0, |index| index + 1);
        self.items.splice(insert_at..insert_at, items);
    }

    pub fn enqueue(&mut self, track: TrackResponse) {
        self.enqueue_many(vec![track]);
    }

    pub fn enqueue_many(&mut self, tracks: Vec<TrackResponse>) {
        if tracks.is_empty() {
            return;
        }
        if self.index.is_none() {
            self.play_from(tracks, 0, false, None);
            return;
        }
        for track in &tracks {
            self.note_inserted(track);
        }
        for track in tracks {
            let item = self.alloc_item(track);
            self.items.push(item);
        }
    }

    pub fn advance(&mut self) -> Option<TrackResponse> {
        if self.repeat == RepeatMode::One {
            return self.current().cloned();
        }
        let index = self.index?;
        if index + 1 < self.items.len() {
            self.index = Some(index + 1);
            return self.current().cloned();
        }
        if self.repeat != RepeatMode::All || self.items.is_empty() {
            return None;
        }
        self.index = Some(0);
        self.current().cloned()
    }

    pub fn go_previous(&mut self) -> Option<TrackResponse> {
        let index = self.index?;
        if index > 0 {
            self.index = Some(index - 1);
        }
        self.current().cloned()
    }

    pub fn jump(&mut self, index: usize) -> Option<TrackResponse> {
        if index >= self.items.len() {
            return self.current().cloned();
        }
        self.index = Some(index);
        self.current().cloned()
    }

    pub fn jump_key(&mut self, key: u64) -> Option<TrackResponse> {
        let index = self.items.iter().position(|item| item.key == key)?;
        self.jump(index)
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.remove_at(index);
        }
    }

    pub fn remove_key(&mut self, key: u64) {
        if let Some(index) = self.items.iter().position(|item| item.key == key) {
            self.remove_at(index);
        }
    }

    pub fn play_next_key(&mut self, key: u64) {
        let Some(from) = self.items.iter().position(|item| item.key == key) else {
            return;
        };
        let Some(current) = self.index else {
            return;
        };
        if from == current || from == current + 1 {
            return;
        }
        let item = self.items.remove(from);
        let current = if from < current { current - 1 } else { current };
        self.items.insert(current + 1, item);
        self.index = Some(current);
    }

    pub fn move_key_to_end(&mut self, key: u64) {
        let Some(from) = self.items.iter().position(|item| item.key == key) else {
            return;
        };
        if from + 1 == self.items.len() {
            return;
        }
        let item = self.items.remove(from);
        if Some(from) == self.index {
            self.items.push(item);
            self.index = Some(self.items.len() - 1);
            return;
        }
        if self.index.is_some_and(|index| from < index) {
            self.index = self.index.map(|index| index - 1);
        }
        self.items.push(item);
    }

    pub fn move_by_key(&mut self, key: u64, to: usize) -> bool {
        let Some(from) = self.items.iter().position(|item| item.key == key) else {
            return false;
        };
        if self.items.is_empty() {
            return false;
        }
        let to = to.min(self.items.len() - 1);
        if from == to {
            return false;
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        self.index = Some(adjust_index_after_move(self.index, from, to));
        true
    }

    pub fn clear_up_next(&mut self) {
        if let Some(index) = self.index {
            self.items.truncate(index + 1);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.index = None;
        self.origin = None;
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if !self.shuffle {
            return;
        }
        let Some(index) = self.index else {
            shuffle_items(&mut self.items);
            return;
        };
        if index + 1 < self.items.len() {
            shuffle_items(&mut self.items[index + 1..]);
        }
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.next();
    }

    fn remove_at(&mut self, index: usize) {
        self.items.remove(index);
        let Some(current) = self.index else {
            return;
        };
        if self.items.is_empty() {
            self.index = None;
            return;
        }
        if index < current {
            self.index = Some(current - 1);
        } else if index == current {
            self.index = Some(index.min(self.items.len() - 1));
        }
    }

    fn note_inserted(&mut self, track: &TrackResponse) {
        match &self.origin {
            Some(QueueOrigin::Playlist { .. }) => self.origin = None,
            Some(QueueOrigin::Album { id, .. })
                if track.album_id.as_deref() != Some(id.as_str()) =>
            {
                self.origin = None;
            }
            _ => {}
        }
    }

    fn unified_album_name(&self) -> Option<&str> {
        let first = self.items.first()?;
        let album_id = first.track.album_id.as_ref().filter(|id| !id.is_empty())?;
        let name = first
            .track
            .album
            .as_deref()
            .filter(|name| !name.is_empty())?;
        self.items
            .iter()
            .all(|item| item.track.album_id.as_ref() == Some(album_id))
            .then_some(name)
    }

    fn alloc_item(&mut self, track: TrackResponse) -> QueueItem {
        self.next_key = self.next_key.saturating_add(1);
        QueueItem {
            key: self.next_key,
            track,
        }
    }
}

fn adjust_index_after_move(index: Option<usize>, from: usize, to: usize) -> usize {
    let Some(index) = index else {
        return to;
    };
    if from == index {
        to
    } else if from < index && to >= index {
        index - 1
    } else if from > index && to <= index {
        index + 1
    } else {
        index
    }
}

fn shuffle_tracks(tracks: &mut [TrackResponse]) {
    if tracks.len() < 2 {
        return;
    }
    let mut seed = mix_seed(tracks.len() as u64, tracks.first().map(|track| &track.id));
    fisher_yates(tracks.len(), &mut seed, |left, right| {
        tracks.swap(left, right)
    });
}

fn shuffle_items(items: &mut [QueueItem]) {
    if items.len() < 2 {
        return;
    }
    let mut seed = mix_seed(items.len() as u64, items.first().map(|item| &item.track.id));
    fisher_yates(items.len(), &mut seed, |left, right| {
        items.swap(left, right)
    });
}

fn mix_seed(len: u64, first_id: Option<&String>) -> u64 {
    len ^ first_id
        .map(|id| {
            id.bytes().fold(0_u64, |hash, byte| {
                hash.wrapping_mul(31).wrapping_add(u64::from(byte))
            })
        })
        .unwrap_or(1)
        ^ std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(1)
}

fn fisher_yates(len: usize, seed: &mut u64, mut swap: impl FnMut(usize, usize)) {
    for index in (1..len).rev() {
        *seed = seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xBF58_476D_1CE4_E5B9);
        let other = (*seed as usize) % (index + 1);
        swap(index, other);
    }
}
