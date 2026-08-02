use std::{ops::Range, sync::Arc, time::Duration};

use gpui::SharedString;

#[derive(Clone, Debug)]
pub struct LyricWord {
    /// UTF-8 byte range in [`LyricLine::text`] painted by this timing unit.
    pub range: Range<usize>,
    /// Absolute playback time for the beginning of the reveal.
    pub start: Duration,
    /// Absolute playback time for the end of the reveal.
    pub end: Duration,
    /// Units in the same group share the elastic word emphasis.
    pub group: usize,
}

impl LyricWord {
    pub fn new(range: Range<usize>, start: Duration, end: Duration) -> Self {
        let group = range.start;
        Self {
            range,
            start,
            end,
            group,
        }
    }

    pub fn group(mut self, group: usize) -> Self {
        self.group = group;
        self
    }
}

#[derive(Clone, Debug)]
pub struct LyricLine {
    pub start: Option<Duration>,
    pub end: Option<Duration>,
    pub text: SharedString,
    pub translation: Option<SharedString>,
    pub words: Arc<[LyricWord]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyricsMode {
    Empty,
    Plain,
    Synced,
}

#[derive(Clone, Debug)]
pub struct LyricsDocument {
    pub mode: LyricsMode,
    pub lines: Arc<[LyricLine]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LyricGap {
    pub previous: usize,
    pub next: usize,
    pub start: Duration,
    pub end: Duration,
}

impl LyricsDocument {
    pub fn parse(source: Option<&str>) -> Self {
        let Some(source) = source.map(str::trim).filter(|source| !source.is_empty()) else {
            return Self::empty();
        };

        let mut offset_ms = 0_i64;
        let mut timed = Vec::<ParsedLine>::new();
        let mut plain = Vec::<String>::new();

        for raw_line in source.lines() {
            let line = raw_line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() {
                continue;
            }
            if let Some(value) = metadata_value(line, "offset") {
                offset_ms = value.trim().parse().unwrap_or_default();
                continue;
            }

            let mut rest = line;
            let mut timestamps = Vec::new();
            while let Some(tag) = rest.strip_prefix('[').and_then(|rest| rest.split_once(']')) {
                let Some(timestamp) = parse_timestamp_ms(tag.0) else {
                    break;
                };
                timestamps.push(timestamp);
                rest = tag.1;
            }

            let text = rest.trim();
            if timestamps.is_empty() {
                if !is_metadata_line(line) {
                    plain.push(line.to_owned());
                }
            } else if !text.is_empty() {
                timed.extend(timestamps.into_iter().map(|timestamp| {
                    parse_timed_line(timestamp.saturating_add(offset_ms), text, offset_ms)
                }));
            }
        }

        if timed.is_empty() {
            let lines = plain
                .into_iter()
                .map(|text| LyricLine {
                    start: None,
                    end: None,
                    text: text.into(),
                    translation: None,
                    words: Arc::default(),
                })
                .collect::<Vec<_>>();
            return if lines.is_empty() {
                Self::empty()
            } else {
                Self {
                    mode: LyricsMode::Plain,
                    lines: lines.into(),
                }
            };
        }

        timed.sort_by_key(|line| line.start);
        let mut merged = Vec::<ParsedLine>::new();
        for mut line in timed {
            line.start = line.start.max(0);
            if let Some(last) = merged.last_mut()
                && last.start == line.start
            {
                merge_same_timestamp(last, line);
            } else {
                merged.push(line);
            }
        }

        let mut lines = Vec::with_capacity(merged.len());
        for (index, line) in merged.iter().enumerate() {
            let start_ms = line.start.max(0);
            let end_ms = line
                .end
                .map(|end| end.max(start_ms))
                .or_else(|| merged.get(index + 1).map(|next| next.start.max(start_ms)))
                .unwrap_or_else(|| start_ms.saturating_add(6_000));
            let words = line
                .words
                .iter()
                .map(|word| {
                    LyricWord::new(
                        word.range.clone(),
                        Duration::from_millis(word.start.max(0) as u64),
                        Duration::from_millis(word.end.unwrap_or(end_ms).max(word.start) as u64),
                    )
                    .group(word.group)
                })
                .collect::<Vec<_>>();
            lines.push(LyricLine {
                start: Some(Duration::from_millis(start_ms as u64)),
                end: Some(Duration::from_millis(end_ms as u64)),
                text: line.text.clone().into(),
                translation: (!line.translations.is_empty())
                    .then(|| SharedString::from(line.translations.join("\n"))),
                words: words.into(),
            });
        }

        Self {
            mode: LyricsMode::Synced,
            lines: lines.into(),
        }
    }

    pub fn empty() -> Self {
        Self {
            mode: LyricsMode::Empty,
            lines: Arc::default(),
        }
    }

    pub fn active_index(&self, position: Duration) -> Option<usize> {
        if self.mode != LyricsMode::Synced {
            return None;
        }
        let next = self
            .lines
            .partition_point(|line| line.start.is_some_and(|start| start <= position));
        let index = next.checked_sub(1)?;
        self.lines[index]
            .end
            .is_none_or(|end| position < end)
            .then_some(index)
    }

    pub fn gap_at(&self, position: Duration, minimum: Duration) -> Option<LyricGap> {
        if self.mode != LyricsMode::Synced {
            return None;
        }
        let next = self
            .lines
            .partition_point(|line| line.start.is_some_and(|start| start <= position));
        let previous = next.checked_sub(1)?;
        let gap_start = self.lines.get(previous)?.end?;
        let gap_end = self.lines.get(next)?.start?;
        (gap_end.saturating_sub(gap_start) >= minimum
            && position >= gap_start
            && position < gap_end)
            .then_some(LyricGap {
                previous,
                next,
                start: gap_start,
                end: gap_end,
            })
    }
}

#[derive(Debug)]
struct ParsedWord {
    range: Range<usize>,
    start: i64,
    end: Option<i64>,
    group: usize,
}

#[derive(Debug)]
struct ParsedLine {
    start: i64,
    end: Option<i64>,
    text: String,
    translations: Vec<String>,
    words: Vec<ParsedWord>,
}

fn parse_timed_line(start: i64, source: &str, offset_ms: i64) -> ParsedLine {
    if next_timestamp(source, 0).is_none() {
        return ParsedLine {
            start,
            end: None,
            text: source.to_owned(),
            translations: Vec::new(),
            words: Vec::new(),
        };
    }

    let mut text = String::new();
    let mut words = Vec::new();
    let mut cursor = 0;
    let mut unit_start = start;
    let mut trailing_marker = None;

    while let Some((tag_start, tag_end, timestamp)) = next_timestamp(source, cursor) {
        let unit_end = timestamp.saturating_add(offset_ms);
        append_timed_segment(
            &mut text,
            &mut words,
            &source[cursor..tag_start],
            unit_start,
            Some(unit_end),
        );
        unit_start = unit_end;
        trailing_marker = Some(unit_end);
        cursor = tag_end;
    }

    let tail = &source[cursor..];
    append_timed_segment(&mut text, &mut words, tail, unit_start, None);
    if words.len() == 1 {
        words.clear();
    }

    ParsedLine {
        start,
        end: tail.is_empty().then_some(trailing_marker).flatten(),
        text,
        translations: Vec::new(),
        words,
    }
}

fn append_timed_segment(
    text: &mut String,
    words: &mut Vec<ParsedWord>,
    segment: &str,
    start: i64,
    end: Option<i64>,
) {
    if segment.is_empty() {
        return;
    }
    let range_start = text.len();
    text.push_str(segment);
    words.push(ParsedWord {
        range: range_start..text.len(),
        start,
        end,
        group: words.len(),
    });
}

fn next_timestamp(source: &str, from: usize) -> Option<(usize, usize, i64)> {
    let mut search_from = from;
    while let Some(relative_start) = source.get(search_from..)?.find('[') {
        let tag_start = search_from + relative_start;
        let contents_start = tag_start + 1;
        let relative_end = source.get(contents_start..)?.find(']')?;
        let tag_end = contents_start + relative_end + 1;
        if let Some(timestamp) = parse_timestamp_ms(&source[contents_start..tag_end - 1]) {
            return Some((tag_start, tag_end, timestamp));
        }
        search_from = tag_end;
    }
    None
}

fn merge_same_timestamp(target: &mut ParsedLine, mut incoming: ParsedLine) {
    if incoming.words.len() > target.words.len() {
        std::mem::swap(&mut target.text, &mut incoming.text);
        std::mem::swap(&mut target.end, &mut incoming.end);
        std::mem::swap(&mut target.words, &mut incoming.words);
    }
    if !incoming.text.is_empty() && incoming.text != target.text {
        target.translations.push(incoming.text);
    }
    target.translations.append(&mut incoming.translations);
}

fn metadata_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let contents = line.strip_prefix('[')?.strip_suffix(']')?;
    let (name, value) = contents.split_once(':')?;
    name.eq_ignore_ascii_case(key).then_some(value)
}

fn is_metadata_line(line: &str) -> bool {
    ["ar", "al", "ti", "by", "re", "ve", "offset"]
        .iter()
        .any(|key| metadata_value(line, key).is_some())
}

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    let (minutes, seconds) = value.trim().split_once(':')?;
    let minutes = minutes.parse::<u64>().ok()?;
    let seconds = seconds.parse::<f64>().ok()?;
    if !seconds.is_finite() || !(0.0..60.0).contains(&seconds) {
        return None;
    }
    let millis = minutes
        .checked_mul(60_000)?
        .checked_add((seconds * 1000.).round() as u64)?;
    i64::try_from(millis).ok()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{LyricsDocument, LyricsMode};

    #[test]
    fn parses_line_timed_embedded_lrc() {
        let lyrics = LyricsDocument::parse(Some(
            "[ti:Demo]\n[ar:Artist]\n[00:01.20]First\n[00:03.40]Second",
        ));
        assert_eq!(lyrics.mode, LyricsMode::Synced);
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(lyrics.active_index(Duration::from_millis(1300)), Some(0));
        assert_eq!(lyrics.active_index(Duration::from_millis(3500)), Some(1));
    }

    #[test]
    fn applies_offset_and_merges_translations() {
        let lyrics = LyricsDocument::parse(Some(
            "[offset:100]\n[00:01.00]Original\n[00:01.00]Translation",
        ));
        assert_eq!(lyrics.lines[0].start, Some(Duration::from_millis(1100)));
        assert_eq!(lyrics.lines[0].text.as_ref(), "Original");
        assert_eq!(lyrics.lines[0].translation.as_deref(), Some("Translation"));
    }

    #[test]
    fn parses_lddc_word_timing_and_translation() {
        let lyrics = LyricsDocument::parse(Some(
            "[00:29.654]明[00:29.966]け[00:30.238]\n\
             [00:29.654]在黎明[00:30.238]",
        ));
        let line = &lyrics.lines[0];

        assert_eq!(line.text.as_ref(), "明け");
        assert_eq!(line.translation.as_deref(), Some("在黎明"));
        assert_eq!(line.end, Some(Duration::from_millis(30_238)));
        assert_eq!(line.words.len(), 2);
        assert_eq!(line.words[0].range, 0..3);
        assert_eq!(line.words[0].start, Duration::from_millis(29_654));
        assert_eq!(line.words[0].end, Duration::from_millis(29_966));
        assert_eq!(line.words[1].range, 3..6);
        assert_eq!(line.words[1].end, Duration::from_millis(30_238));
    }

    #[test]
    fn keeps_unsynchronized_embedded_lyrics_as_plain_lines() {
        let lyrics = LyricsDocument::parse(Some("First line\nSecond line"));
        assert_eq!(lyrics.mode, LyricsMode::Plain);
        assert_eq!(lyrics.lines.len(), 2);
        assert!(lyrics.active_index(Duration::from_secs(1)).is_none());
    }

    #[test]
    fn parses_explicit_line_end_and_clears_highlight_during_gaps() {
        let lyrics = LyricsDocument::parse(Some(
            "[00:02.500]本家：av32647141[00:05.270]\n\
             [00:07.840]编曲：Demo[00:12.050]",
        ));

        assert_eq!(lyrics.lines[0].text.as_ref(), "本家：av32647141");
        assert_eq!(lyrics.lines[0].end, Some(Duration::from_millis(5_270)));
        assert!(lyrics.lines[0].words.is_empty());
        assert_eq!(lyrics.active_index(Duration::from_millis(4_000)), Some(0));
        assert_eq!(lyrics.active_index(Duration::from_millis(6_000)), None);
        let gap = lyrics
            .gap_at(Duration::from_millis(6_000), Duration::from_secs(1))
            .expect("visible interlude gap");
        assert_eq!(gap.previous, 0);
        assert_eq!(gap.next, 1);
        assert_eq!(gap.start, Duration::from_millis(5_270));
        assert_eq!(gap.end, Duration::from_millis(7_840));
        assert_eq!(lyrics.active_index(Duration::from_millis(8_000)), Some(1));
    }
}
