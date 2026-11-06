use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Note {
    pub mask: u32,
    pub time: f32,
}

#[derive(Clone, Debug)]
pub struct Chart {
    pub steps_type: String,
    pub description: String,
    pub difficulty: String,
    pub meter: String,
    pub key_count: u32,
    pub notes: Vec<Note>,
    pub note_count: u32,
    pub duration: f32,
    pub avg_nps: f32,
    pub peak_nps: f32,
    pub max_rows_half: u32,
    pub ratings: [[f32; 8]; 14],
}

#[derive(Clone, Debug)]
pub struct Song {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub charts: Vec<Chart>,
    pub warnings: Vec<String>,
}

pub fn skill_labels(key_count: u32) -> [&'static str; 8] {
    if key_count == 4 {
        [
            "综合",
            "Stream",
            "Jumpstream",
            "Handstream",
            "Stamina",
            "Jack",
            "Chordjack",
            "Tech",
        ]
    } else {
        [
            "综合",
            "Stream",
            "Chordstream",
            "Bracketing",
            "Stamina",
            "Jack",
            "Chordjack",
            "Tech",
        ]
    }
}

pub fn rate_label(index: usize) -> String {
    format!("{:.2}", (index as f32 + 7.0) / 10.0)
}

pub fn blurb(chart: &Chart) -> String {
    if chart.notes.len() <= 1 {
        return "有效音符不足两行，无法计算难度。".to_owned();
    }
    let at_song_rate = chart.ratings[3];
    if at_song_rate.iter().any(|value| !value.is_finite()) {
        return "计算器未能返回有效数值。".to_owned();
    }
    if at_song_rate.iter().all(|value| *value == 0.0) {
        return format!(
            "1.00x 速率结果为 0。峰值半秒行数为 {}，超过 MinaCalc 50行/0.5s（100 NPS）的硬性上限，该档已被跳过。",
            chart.max_rows_half
        );
    }

    let labels = skill_labels(chart.key_count);
    let mut top = 1usize;
    let mut low = 1usize;
    for index in 2..8 {
        if at_song_rate[index] > at_song_rate[top] {
            top = index;
        }
        if at_song_rate[index] < at_song_rate[low] {
            low = index;
        }
    }

    let mut text = format!(
        "1.00x 综合 MSD {:.2}。优势属性为 {} ({:.2})，弱势属性为 {} ({:.2})。有效音符跨度 {:.1} 秒，合计 {} 个音符，平均密度 {:.1} NPS，半秒峰值密度 {:.1} NPS。",
        at_song_rate[0],
        labels[top],
        at_song_rate[top],
        labels[low],
        at_song_rate[low],
        chart.duration,
        chart.note_count,
        chart.avg_nps,
        chart.peak_nps
    );
    if chart.ratings.iter().skip(4).any(|row| row[0] == 0.0) {
        text.push_str(" 注意：部分高倍速档位显示为 0，系时间压缩后局部密度超出了计算器上限。");
    }
    text
}

pub fn parse_sm(path: &Path, bytes: &[u8]) -> Result<Song, String> {
    let text = decode_simfile(bytes);
    let stripped = strip_line_comments(&text);
    let tags = scan_tags(&stripped);

    let title = tag_value(&tags, "TITLE")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未命名曲目".to_owned())
        });
    let artist = tag_value(&tags, "ARTIST").unwrap_or_else(|| "未知艺术家".to_owned());

    let mut warnings = Vec::new();
    let timing = match tags.iter().find(|(name, _)| name == "BPMS") {
        Some((_, value)) => Timing::from_tag_values(value, &tags, &mut warnings),
        None => {
            warnings.push("谱面未包含 #BPMS 标签，默认使用固定 BPM 60。".to_owned());
            Timing::default_bpm()
        }
    };

    let mut saw_notes = false;
    let mut charts = Vec::new();
    for (name, value) in &tags {
        if name != "NOTES" && name != "NOTES2" {
            continue;
        }
        saw_notes = true;
        if let Some(chart) = parse_chart(value, &timing) {
            charts.push(chart);
        }
    }
    if charts.is_empty() {
        if saw_notes {
            return Err("检测到 #NOTES 字段，但格式不完整或字段不足，未能解析出有效谱面。".to_owned());
        }
        return Err("文件中未检测到 #NOTES 谱面数据。".to_owned());
    }

    Ok(Song {
        path: path.to_path_buf(),
        title,
        artist,
        charts,
        warnings,
    })
}

fn decode_simfile(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        return String::from_utf16_lossy(&units).replace("\r\n", "\n").replace('\r', "\n");
    }
    let body = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    String::from_utf8_lossy(body)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn strip_line_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(index) = rest.find("//") {
        let preceded_by_space = index == 0 || rest.as_bytes()[index - 1].is_ascii_whitespace();
        if !preceded_by_space {
            out.push_str(&rest[..=index]);
            rest = &rest[index + 1..];
            continue;
        }
        out.push_str(&rest[..index]);
        rest = match rest[index..].find('\n') {
            Some(newline) => &rest[index + newline..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn scan_tags(src: &str) -> Vec<(String, String)> {
    let bytes = src.as_bytes();
    let mut index = 0;
    let mut tags = Vec::new();
    while index < bytes.len() {
        if bytes[index] != b'#' {
            index += 1;
            continue;
        }
        let name_start = index + 1;
        let mut cursor = name_start;
        while cursor < bytes.len()
            && bytes[cursor] != b':'
            && bytes[cursor] != b'\n'
            && bytes[cursor] != b';'
        {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            index += 1;
            continue;
        }
        let name = src[name_start..cursor].trim().to_ascii_uppercase();
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b';' {
            cursor += 1;
        }
        tags.push((name, src[value_start..cursor].to_owned()));
        index = if cursor < bytes.len() { cursor + 1 } else { cursor };
    }
    tags
}

fn tag_value(tags: &[(String, String)], name: &str) -> Option<String> {
    tags.iter()
        .find(|(tag, _)| tag == name)
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_chart(value: &str, timing: &Timing) -> Option<Chart> {
    let fields = split_note_fields(value)?;
    let steps_type = fields[0].trim().to_owned();
    let description = fields[1].trim().to_owned();
    let difficulty = {
        let text = fields[2].trim();
        if text.is_empty() {
            "未命名".to_owned()
        } else {
            text.to_owned()
        }
    };
    let meter = fields[3].trim().to_owned();
    let notedata = fields[5];
    let key_count = columns_for_steps(&steps_type).unwrap_or_else(|| infer_columns(notedata));
    if key_count == 0 {
        return None;
    }

    let mut placed = Vec::new();
    for (measure_index, measure) in split_measures(notedata).into_iter().enumerate() {
        let lines = note_lines(measure);
        let line_count = lines.len();
        for (line_index, line) in lines.iter().enumerate() {
            let fraction = line_index as f32 / line_count as f32;
            let beat = (measure_index as f32 + fraction) * 4.0;
            let row = beat_to_row(beat);
            if timing.is_warp_row(row) || timing.is_fake_row(row) {
                continue;
            }
            let mask = column_mask(line, key_count);
            if mask == 0 {
                continue;
            }
            if placed.last().map(|(existing, _)| *existing) == Some(row) {
                let last = placed.last_mut().unwrap();
                last.1 |= mask;
            } else {
                placed.push((row, mask));
            }
        }
    }

    let mut notes = Vec::with_capacity(placed.len());
    if !placed.is_empty() {
        let origin = timing.time_at_row(placed[0].0);
        for (row, mask) in placed {
            notes.push(Note {
                mask,
                time: timing.time_at_row(row) - origin,
            });
        }
    }

    let (note_count, duration, avg_nps, peak_nps, max_rows_half) = density(&notes);
    Some(Chart {
        steps_type,
        description,
        difficulty,
        meter,
        key_count,
        notes,
        note_count,
        duration,
        avg_nps,
        peak_nps,
        max_rows_half,
        ratings: [[0.0; 8]; 14],
    })
}

fn split_note_fields(value: &str) -> Option<Vec<&str>> {
    let bytes = value.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b':' {
            parts.push(&value[start..index]);
            start = index + 1;
            if parts.len() == 5 {
                parts.push(&value[start..]);
                break;
            }
        }
    }
    if parts.len() == 6 {
        Some(parts)
    } else {
        None
    }
}

fn columns_for_steps(steps_type: &str) -> Option<u32> {
    Some(
        match steps_type.trim().to_ascii_lowercase().as_str() {
            "dance-single" | "maniax-single" | "techno-single4" => 4,
            "dance-double" | "dance-couple" | "dance-routine" | "ds3ddx-single"
            | "techno-double4" => 8,
            "dance-solo" | "pump-halfdouble" => 6,
            "dance-threepanel" => 3,
            "pump-single" | "ez2-single" | "para-single" | "techno-single5" | "pnm-five" => 5,
            "pump-double" | "pump-couple" | "ez2-double" | "techno-double5" => 10,
            "kb7-single" | "ez2-real" => 7,
            "techno-single8" => 8,
            "pnm-nine" => 9,
            _ => return None,
        },
    )
}

fn infer_columns(notedata: &str) -> u32 {
    let mut widest = 0u32;
    for measure in split_measures(notedata) {
        for line in note_lines(measure) {
            widest = widest.max(count_columns(line));
        }
    }
    widest.min(16)
}

fn split_measures(data: &str) -> Vec<&str> {
    let bytes = data.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index] == b',' {
        index += 1;
    }
    let mut start = index;
    let mut measures = Vec::new();
    while index < bytes.len() {
        if bytes[index] == b',' {
            measures.push(&data[start..index]);
            index += 1;
            while index < bytes.len() && bytes[index] == b',' {
                index += 1;
            }
            start = index;
        } else {
            index += 1;
        }
    }
    if start < bytes.len() {
        measures.push(&data[start..]);
    }
    measures
}

fn note_lines(measure: &str) -> Vec<&str> {
    measure
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn column_mask(line: &str, key_count: u32) -> u32 {
    let mut mask = 0u32;
    let mut column = 0u32;
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            for skipped in chars.by_ref() {
                if skipped == ']' {
                    break;
                }
            }
            continue;
        }
        if column >= key_count {
            break;
        }
        if matches!(ch, '1' | '2' | '4') && column < 32 {
            mask |= 1 << column;
        }
        column += 1;
    }
    mask
}

fn count_columns(line: &str) -> u32 {
    let mut column = 0u32;
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            for skipped in chars.by_ref() {
                if skipped == ']' {
                    break;
                }
            }
            continue;
        }
        column += 1;
    }
    column
}

fn density(notes: &[Note]) -> (u32, f32, f32, f32, u32) {
    let mut note_count = 0u32;
    let mut rows_and_taps: Vec<(i32, u32, u32)> = Vec::new();
    for note in notes {
        let taps = note.mask.count_ones();
        note_count += taps;
        let bucket = (note.time / 0.5).floor() as i32;
        if let Some(slot) = rows_and_taps.iter_mut().find(|slot| slot.0 == bucket) {
            slot.1 += 1;
            slot.2 += taps;
        } else {
            rows_and_taps.push((bucket, 1, taps));
        }
    }
    let duration = notes.last().map(|note| note.time.max(0.0)).unwrap_or(0.0);
    let max_rows_half = rows_and_taps.iter().map(|slot| slot.1).max().unwrap_or(0);
    let peak_taps = rows_and_taps.iter().map(|slot| slot.2).max().unwrap_or(0);
    let peak_nps = peak_taps as f32 / 0.5;
    let avg_nps = if duration > 0.05 {
        note_count as f32 / duration
    } else {
        note_count as f32
    };
    (note_count, duration, avg_nps, peak_nps, max_rows_half)
}

fn beat_to_row(beat: f32) -> i32 {
    (beat * 48.0).round() as i32
}

fn row_to_beat(row: i32) -> f32 {
    row as f32 / 48.0
}

#[derive(Clone, Debug)]
struct Seg {
    row: i32,
    value: f32,
}

#[derive(Clone, Debug)]
struct Timing {
    bpms: Vec<Seg>,
    stops: Vec<Seg>,
    delays: Vec<Seg>,
    warps: Vec<Seg>,
    fakes: Vec<Seg>,
}

impl Timing {
    fn default_bpm() -> Self {
        Self {
            bpms: vec![Seg { row: 0, value: 60.0 }],
            stops: Vec::new(),
            delays: Vec::new(),
            warps: Vec::new(),
            fakes: Vec::new(),
        }
    }

    fn from_tag_values(bpms: &str, tags: &[(String, String)], warnings: &mut Vec<String>) -> Self {
        let mut timing = Self {
            bpms: prepare_bpms(parse_pairs(bpms)),
            stops: dedupe_last(parse_pairs(tag_raw(tags, "STOPS"))),
            delays: dedupe_last(parse_pairs(tag_raw(tags, "DELAYS"))),
            warps: dedupe_last(parse_pairs(tag_raw(tags, "WARPS"))),
            fakes: dedupe_last(parse_pairs(tag_raw(tags, "FAKES"))),
        };
        if timing.bpms.iter().any(|seg| seg.value == 0.0) {
            warnings.push("检测到 BPM 为 0 的异常片段，计算时间轴可能与游戏内存在偏差。".to_owned());
        }
        if timing.bpms.is_empty() {
            timing.bpms.push(Seg { row: 0, value: 60.0 });
        }
        timing
    }

    fn time_at_row(&self, target: i32) -> f32 {
        let mut bpm_index = 0usize;
        let mut stop_index = 0usize;
        let mut delay_index = 0usize;
        let mut warp_index = 0usize;
        let mut last_row = 0i32;
        let mut time = 0.0f32;
        let mut bps = self.bpms.first().map(|seg| seg.value / 60.0).unwrap_or(1.0);
        let mut is_warping = false;
        let mut warp_destination = 0.0f32;

        for _ in 0..100_000 {
            let mut event_row = i32::MAX;
            let mut kind = 0u8;

            if is_warping {
                let destination = beat_to_row(warp_destination);
                if destination < event_row {
                    event_row = destination;
                    kind = 1;
                }
            }
            if bpm_index < self.bpms.len() && self.bpms[bpm_index].row < event_row {
                event_row = self.bpms[bpm_index].row;
                kind = 2;
            }
            if delay_index < self.delays.len() && self.delays[delay_index].row < event_row {
                event_row = self.delays[delay_index].row;
                kind = 3;
            }
            if target < event_row {
                event_row = target;
                kind = 4;
            }
            if stop_index < self.stops.len() && self.stops[stop_index].row < event_row {
                event_row = self.stops[stop_index].row;
                kind = 5;
            }
            if warp_index < self.warps.len() && self.warps[warp_index].row < event_row {
                event_row = self.warps[warp_index].row;
                kind = 6;
            }
            if kind == 0 {
                return time;
            }

            if !is_warping && bps.abs() >= 1.0e-8 {
                time += (event_row - last_row) as f32 / 48.0 / bps;
            }

            match kind {
                1 => is_warping = false,
                2 => {
                    bps = self.bpms[bpm_index].value / 60.0;
                    bpm_index += 1;
                }
                3 => {
                    time += self.delays[delay_index].value;
                    delay_index += 1;
                }
                4 => return time,
                5 => {
                    time += self.stops[stop_index].value;
                    stop_index += 1;
                }
                6 => {
                    is_warping = true;
                    let start = row_to_beat(self.warps[warp_index].row);
                    let destination = start + self.warps[warp_index].value;
                    if destination > warp_destination {
                        warp_destination = destination;
                    }
                    warp_index += 1;
                }
                _ => return time,
            }
            last_row = event_row;
        }
        time
    }

    fn is_warp_row(&self, row: i32) -> bool {
        let Some(warp) = segment_covering(&self.warps, row) else {
            return false;
        };
        let beat = row_to_beat(row);
        let start = row_to_beat(warp.row);
        if start <= beat && beat < start + warp.value {
            if self.stops.is_empty() && self.delays.is_empty() {
                return true;
            }
            let stop_here = self.stops.iter().any(|seg| seg.row == row && seg.value != 0.0);
            let delay_here = self.delays.iter().any(|seg| seg.row == row && seg.value != 0.0);
            return !(stop_here || delay_here);
        }
        false
    }

    fn is_fake_row(&self, row: i32) -> bool {
        let Some(fake) = segment_covering(&self.fakes, row) else {
            return false;
        };
        let beat = row_to_beat(row);
        let start = row_to_beat(fake.row);
        start <= beat && beat < start + fake.value
    }
}

fn tag_raw<'a>(tags: &'a [(String, String)], name: &str) -> &'a str {
    tags.iter()
        .find(|(tag, _)| tag == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn parse_pairs(text: &str) -> Vec<Seg> {
    let mut pairs = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((beat, value)) = part.split_once('=') else {
            continue;
        };
        let Ok(beat) = beat.trim().parse::<f32>() else {
            continue;
        };
        let Ok(value) = value.trim().parse::<f32>() else {
            continue;
        };
        pairs.push(Seg {
            row: beat_to_row(beat),
            value,
        });
    }
    pairs
}

fn prepare_bpms(pairs: Vec<Seg>) -> Vec<Seg> {
    let mut bpms = dedupe_last(pairs);
    if bpms.is_empty() {
        bpms.push(Seg { row: 0, value: 60.0 });
    } else if bpms[0].row != 0 {
        bpms[0].row = 0;
    }
    bpms
}

fn dedupe_last(mut pairs: Vec<Seg>) -> Vec<Seg> {
    pairs.sort_by_key(|seg| seg.row);
    let mut kept = Vec::new();
    for seg in pairs {
        if kept.last().map(|existing: &Seg| existing.row) == Some(seg.row) {
            kept.pop();
        }
        kept.push(seg);
    }
    kept
}

fn segment_covering(segs: &[Seg], row: i32) -> Option<&Seg> {
    let mut found = None;
    for seg in segs {
        if seg.row <= row {
            found = Some(seg);
        } else {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(text: &str) -> Song {
        parse_sm(Path::new("demo.sm"), text.as_bytes()).unwrap()
    }

    #[test]
    fn quarter_notes_at_120_land_on_half_seconds() {
        let parsed = song(
            r#"
#TITLE:Hi;
#ARTIST:Me;
#BPMS:0.000=120.000;
#NOTES:
dance-single:
:
Hard:
12:
0:
1000
0100
0000
0000
,
0011
;
"#,
        );
        let chart = &parsed.charts[0];
        assert_eq!(chart.key_count, 4);
        assert_eq!(chart.notes.len(), 3);
        assert_eq!(chart.notes[0].mask, 0b0001);
        assert_eq!(chart.notes[1].mask, 0b0010);
        assert_eq!(chart.notes[2].mask, 0b1100);
        assert!((chart.notes[1].time - 0.5).abs() < 1e-3);
        assert!((chart.notes[2].time - 2.0).abs() < 1e-3);
        assert_eq!(chart.note_count, 4);
    }

    #[test]
    fn stop_waits_until_the_next_row_delay_lands_on_the_row() {
        let stopped = song(
            r#"
#BPMS:0=120;
#STOPS:4=1;
#NOTES:
dance-single:
:
Hard:
1:
0:
1000
0000
0000
0000
,
1000
0000
0000
0000
,
1000
;
"#,
        );
        let delayed = song(
            r#"
#BPMS:0=120;
#DELAYS:4=1;
#NOTES:
dance-single:
:
Hard:
1:
0:
1000
0000
0000
0000
,
1000
;
"#,
        );
        let notes = &stopped.charts[0].notes;
        assert!((notes[1].time - 2.0).abs() < 1e-3, "on stop {}", notes[1].time);
        assert!((notes[2].time - 5.0).abs() < 1e-3, "after stop {}", notes[2].time);
        let delay_time = delayed.charts[0].notes[1].time;
        assert!((delay_time - 3.0).abs() < 1e-3, "delay {delay_time}");
    }

    #[test]
    fn blank_lines_do_not_subdivide_and_empty_commas_collapse() {
        let parsed = song(
            r#"
#BPMS:0=120;
#NOTES:
dance-single:
:
Hard:
1:
0:
1000

0001
,,
0100
;
"#,
        );
        let notes = &parsed.charts[0].notes;
        assert_eq!(notes.len(), 3);
        assert!((notes[1].time - 1.0).abs() < 1e-3, "{}", notes[1].time);
        assert!((notes[2].time - 2.0).abs() < 1e-3, "{}", notes[2].time);
    }

    #[test]
    fn warp_drops_notes_inside_and_keeps_the_exit() {
        let parsed = song(
            r#"
#BPMS:0=120;
#WARPS:4=4;
#NOTES:
dance-single:
:
Hard:
1:
0:
1000
0000
0000
0000
,
1000
0000
0000
0000
,
1000
;
"#,
        );
        let notes = &parsed.charts[0].notes;
        assert_eq!(notes.len(), 2, "{notes:?}");
        assert!((notes[1].time - 2.0).abs() < 1e-3, "{}", notes[1].time);
    }

    #[test]
    fn etterna_sample_rates_are_finite() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../etterna/Docs/legacy/SimfileFormats/SDF/test.sm");
        if !path.is_file() {
            return;
        }
        let bytes = std::fs::read(&path).unwrap();
        let song = parse_sm(&path, &bytes).unwrap();
        assert!(song.charts.len() >= 2, "{}", song.charts.len());
        assert_eq!(crate::ffi::calc_version(), 527);
        let ratings =
            crate::ffi::rate_notes(&song.charts[0].notes, song.charts[0].key_count).unwrap();
        assert!(ratings[3][0].is_finite());
        assert!(
            ratings[3][0] > 0.0,
            "1.00x overall was {}",
            ratings[3][0]
        );
    }

    #[test]
    fn mines_and_tails_are_not_notes_holds_are() {
        let parsed = song(
            r#"
#BPMS:0=120;
#NOTES:
dance-single:
:
Hard:
1:
0:
1200
0030
0000
0000
,
M000
;
"#,
        );
        let notes = &parsed.charts[0].notes;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].mask, 0b0011);
    }
}
