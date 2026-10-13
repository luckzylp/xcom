//! The receive log: byte stream in, display rows out.
//!
//! Design notes, because the constraints here are not obvious:
//!
//! * Everything received is kept in a bounded `raw` byte buffer.  That buffer is
//!   the single source of truth, so switching between text and hex, changing the
//!   encoding, or changing the wrap width can re-render history instead of only
//!   affecting new data.
//! * Rows are produced *incrementally* as bytes arrive (O(1) amortised per
//!   byte), never by re-rendering the whole buffer per chunk.
//! * Rows are stored pre-wrapped and pre-formatted, so the UI can render a
//!   window of them with uniform height and stay O(visible) no matter how much
//!   data has scrolled by.
//! * Arrival times are recorded as marks alongside the raw bytes, so timestamps
//!   survive a re-render.

use std::collections::VecDeque;

use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};

use crate::encoding::{decode, StreamDecoder};
use crate::hex::{to_dump, to_hex_string};

/// How received bytes are presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewMode {
    /// Decode to text using the selected encoding.
    Text,
    /// Show raw bytes as hexadecimal.
    Hex,
}

/// Layout of the hexadecimal view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HexStyle {
    /// A continuous run of hex bytes, optionally wrapped every N bytes.
    Stream,
    /// Classic `offset  bytes  |ascii|` dump.
    Dump,
}

/// Everything that controls how the log is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayOptions {
    pub mode: ViewMode,
    pub hex_style: HexStyle,
    /// Bytes per row for hex views (16 is the classic dump width).
    pub hex_bytes_per_row: usize,
    /// Render hex digits in upper case.
    pub upper_hex: bool,
    /// Prefix each frame with its arrival time, framed on CR/LF.
    pub timestamp: bool,
    /// Soft-wrap long lines instead of scrolling horizontally.
    pub wrap: bool,
}

impl Default for DisplayOptions {
    fn default() -> Self {
        Self {
            mode: ViewMode::Text,
            hex_style: HexStyle::Stream,
            hex_bytes_per_row: 16,
            upper_hex: true,
            timestamp: false,
            wrap: false,
        }
    }
}

/// How many rows of scrollback to retain before dropping the oldest.
const DEFAULT_MAX_ROWS: usize = 200_000;
/// How many raw bytes to retain for re-rendering (32 MiB ≈ 45 min at 115200).
const DEFAULT_MAX_RAW: usize = 32 * 1024 * 1024;
/// A live line longer than this is committed early so the incremental
/// re-render stays cheap for devices that never send a newline.
const MAX_LIVE_LINE: usize = 16 * 1024;
/// Upper bound on recorded arrival marks.
const MAX_MARKS: usize = 8192;

/// Arrival time of a run of bytes, at an absolute stream offset.
#[derive(Debug, Clone, Copy)]
struct Mark {
    offset: u64,
    ms: i64,
}

/// The receive log.
pub struct LogStore {
    opts: DisplayOptions,
    encoding: &'static Encoding,
    /// Wrap width in display columns; `0` disables wrapping entirely.
    max_cols: usize,

    /// All retained bytes; the source of truth for re-rendering.
    raw: Vec<u8>,
    /// Absolute stream offset of `raw[0]`.
    raw_base: u64,
    marks: Vec<Mark>,

    /// Committed display rows.
    rows: VecDeque<String>,
    /// Absolute id of `rows[0]`, so selections survive trimming.
    first_row_id: u64,
    /// Widest row seen, in display columns; drives the horizontal scrollbar.
    max_columns: usize,

    /// Decoded text of the line currently being received.
    cur_line: String,
    /// `cur_line` broken into display rows.
    cur_rows: Vec<String>,
    /// Streaming decoder for the live line, tolerant of split sequences.
    tail_decoder: StreamDecoder,
    /// Index into `raw` of the next byte not yet consumed into rows.
    cursor: usize,

    // --- hex dump mode bookkeeping -----------------------------------------
    dump_row_start: usize,
    dump_filled: usize,

    max_rows: usize,
    max_raw: usize,
}

impl LogStore {
    pub fn new(encoding: &'static Encoding) -> Self {
        Self {
            opts: DisplayOptions::default(),
            encoding,
            max_cols: 0,
            raw: Vec::new(),
            raw_base: 0,
            marks: Vec::new(),
            rows: VecDeque::new(),
            first_row_id: 0,
            max_columns: 0,
            cur_line: String::new(),
            cur_rows: Vec::new(),
            tail_decoder: StreamDecoder::new(encoding),
            cursor: 0,
            dump_row_start: 0,
            dump_filled: 0,
            max_rows: DEFAULT_MAX_ROWS,
            max_raw: DEFAULT_MAX_RAW,
        }
    }

    // -- configuration ------------------------------------------------------

    /// Update display options, re-rendering history when that is required.
    pub fn set_options(&mut self, opts: DisplayOptions) {
        if opts == self.opts {
            return;
        }
        let layout_changed = opts.mode != self.opts.mode
            || opts.hex_style != self.opts.hex_style
            || opts.hex_bytes_per_row != self.opts.hex_bytes_per_row
            || opts.upper_hex != self.opts.upper_hex
            || opts.timestamp != self.opts.timestamp
            || opts.wrap != self.opts.wrap;
        self.opts = opts;
        if layout_changed {
            self.rebuild();
        }
    }

    /// Switch encoding, re-rendering the retained history.
    pub fn set_encoding(&mut self, encoding: &'static Encoding) {
        if encoding == self.encoding {
            return;
        }
        self.encoding = encoding;
        self.rebuild();
    }

    /// The wrap width currently in effect, in display columns.
    pub fn width_columns(&self) -> usize {
        self.max_cols
    }

    /// Set the wrap width in display columns (0 = never wrap).
    ///
    /// Only text mode consults this: the hex views wrap on byte boundaries so
    /// that rows stay aligned regardless of the window width.  A change
    /// therefore re-renders history only when text wrapping is actually on.
    pub fn set_width_columns(&mut self, cols: usize) {
        if cols == self.max_cols {
            return;
        }
        self.max_cols = cols;
        if self.opts.wrap && self.opts.mode == ViewMode::Text {
            self.rebuild();
        }
    }

    pub fn set_limits(&mut self, max_rows: usize, max_raw: usize) {
        self.max_rows = max_rows.max(1024);
        self.max_raw = max_raw.max(64 * 1024);
        self.trim_raw();
        self.trim_rows();
    }

    // -- ingest -------------------------------------------------------------

    /// Append freshly received bytes.
    ///
    /// `now_ms` is the arrival time of the *last* byte in `bytes`; it is used
    /// for the timestamps of any frame completed by this chunk.
    pub fn append(&mut self, bytes: &[u8], now_ms: i64) {
        if bytes.is_empty() {
            return;
        }
        let abs_start = self.raw_base + self.raw.len() as u64;
        self.raw.extend_from_slice(bytes);

        self.marks.push(Mark {
            offset: abs_start,
            ms: now_ms,
        });
        if self.marks.len() > MAX_MARKS {
            let keep = MAX_MARKS / 2;
            self.marks.drain(..self.marks.len() - keep);
        }

        self.pump();
        self.trim_raw();
        self.refresh_cur_rows();
    }

    /// Bytes currently retained for re-rendering.
    pub fn retained_bytes(&self) -> usize {
        self.raw.len()
    }

    /// Read access to the retained raw stream (used by "save").
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// Drop everything.
    pub fn clear(&mut self) {
        self.raw.clear();
        self.raw_base = 0;
        self.marks.clear();
        self.rows.clear();
        self.first_row_id = 0;
        self.max_columns = 0;
        self.cur_line.clear();
        self.cur_rows.clear();
        self.tail_decoder = StreamDecoder::new(self.encoding);
        self.cursor = 0;
        self.dump_row_start = 0;
        self.dump_filled = 0;
    }

    // -- display access -----------------------------------------------------

    /// Number of rows available for display.
    pub fn row_count(&self) -> usize {
        self.rows.len() + self.cur_rows.len()
    }

    /// The text of display row `index`.
    pub fn row(&self, index: usize) -> &str {
        let committed = self.rows.len();
        if index < committed {
            &self.rows[index]
        } else {
            self.cur_rows
                .get(index - committed)
                .map(String::as_str)
                .unwrap_or("")
        }
    }

    /// A stable identifier for a row, unaffected by trimming.
    pub fn row_id(&self, index: usize) -> u64 {
        self.first_row_id + index as u64
    }

    /// Id of the first (oldest) retained row.
    pub fn first_row_id(&self) -> u64 {
        self.first_row_id
    }

    /// Widest row seen so far, in monospace display columns.
    ///
    /// Tracked incrementally so the view can size its horizontal scroll extent
    /// without measuring every row on every frame.
    pub fn max_columns(&self) -> usize {
        self.max_columns
    }

    /// The line currently being received, for a live preview.
    pub fn pending_line(&self) -> &str {
        &self.cur_line
    }

    /// Render the whole retained buffer as text (used when saving a log).
    pub fn render_all(&self) -> String {
        let mut out = String::new();
        match (self.opts.mode, self.opts.hex_style) {
            (ViewMode::Text, _) => {
                out.push_str(&decode(&self.raw, self.encoding));
            }
            (ViewMode::Hex, HexStyle::Stream) => {
                out.push_str(&to_hex_string(&self.raw, self.opts.upper_hex));
            }
            (ViewMode::Hex, HexStyle::Dump) => {
                for row in to_dump(
                    &self.raw,
                    self.opts.hex_bytes_per_row.max(1),
                    self.raw_base,
                    self.opts.upper_hex,
                ) {
                    out.push_str(&format!(
                        "{:08X}  {}  |{}|\n",
                        row.offset, row.hex, row.ascii
                    ));
                }
            }
        }
        out
    }

    // -- internals ----------------------------------------------------------

    /// Re-render every retained byte.  Used when the layout or encoding
    /// changes; the cost is bounded by `max_raw`.
    fn rebuild(&mut self) {
        self.rows.clear();
        self.first_row_id += self.cur_rows.len() as u64;
        self.max_columns = 0;
        self.cur_line.clear();
        self.cur_rows.clear();
        self.cursor = 0;
        self.dump_row_start = 0;
        self.dump_filled = 0;
        self.tail_decoder = StreamDecoder::new(self.encoding);
        self.pump();
        self.refresh_cur_rows();
    }

    /// Consume as much of `raw` as possible into rows.
    fn pump(&mut self) {
        match (self.opts.mode, self.opts.hex_style) {
            (ViewMode::Hex, HexStyle::Dump) => self.pump_dump(),
            _ => self.pump_framed(),
        }
    }

    /// Text and hex-stream modes: both frame on CR/LF.
    fn pump_framed(&mut self) {
        loop {
            let rest = &self.raw[self.cursor..];
            if rest.is_empty() {
                break;
            }
            if let Some((idx, term_len)) = find_terminator(rest) {
                let body_end = self.cursor + idx;
                self.feed_tail(self.cursor, body_end);
                // Timestamp comes from the chunk that carried the terminator.
                let stamp = self.time_at(self.raw_base + body_end as u64);
                self.commit_line(self.opts.timestamp.then_some(stamp));
                self.cursor = body_end + term_len;
                self.tail_decoder = StreamDecoder::new(self.encoding);
            } else {
                // Everything left is part of the live line.  Hold back a
                // trailing CR: it might be the first half of CRLF.
                let mut end = self.raw.len();
                if self.raw[end - 1] == b'\r' {
                    end -= 1;
                }
                self.feed_tail(self.cursor, end);
                self.cursor = end;
                if self.cur_line.len() >= MAX_LIVE_LINE {
                    self.commit_line(None);
                    self.tail_decoder = StreamDecoder::new(self.encoding);
                    continue;
                }
                break;
            }
            if self.cur_line.len() >= MAX_LIVE_LINE {
                self.commit_line(None);
                self.tail_decoder = StreamDecoder::new(self.encoding);
            }
        }
    }

    /// Hex dump mode: fixed `bytes_per_row` rows with offset and ASCII gutter.
    fn pump_dump(&mut self) {
        let per_row = self.opts.hex_bytes_per_row.max(1);
        loop {
            if self.dump_filled == 0 {
                if self.cursor >= self.raw.len() {
                    break;
                }
                self.dump_row_start = self.cursor;
                self.push_row(String::new());
            }
            let remaining = self.raw.len() - self.cursor;
            if remaining == 0 {
                break;
            }
            let take = (per_row - self.dump_filled).min(remaining);
            self.cursor += take;
            self.dump_filled += take;

            let bytes = &self.raw[self.dump_row_start..self.cursor];
            let offset = self.raw_base + self.dump_row_start as u64;
            let text = render_dump_row(bytes, per_row, offset, self.opts.upper_hex);
            if let Some(last) = self.rows.back_mut() {
                *last = text;
            }
            if self.dump_filled >= per_row {
                self.dump_filled = 0;
            }
        }
    }

    /// Decode `raw[start..end]` and append it to the live line.
    ///
    /// In text mode this runs through the streaming decoder so multi-byte
    /// sequences split across reads survive; in hex mode the bytes are simply
    /// formatted, which is why the two paths look so different.
    fn feed_tail(&mut self, start: usize, end: usize) {
        if end <= start {
            return;
        }
        match self.opts.mode {
            ViewMode::Hex => {
                let hex = to_hex_string(&self.raw[start..end], self.opts.upper_hex);
                if !hex.is_empty() {
                    if !self.cur_line.is_empty() {
                        self.cur_line.push(' ');
                    }
                    self.cur_line.push_str(&hex);
                }
            }
            ViewMode::Text => {
                // Split the borrow so the decoder can write into `cur_line`
                // while reading from `raw`.
                let Self {
                    raw,
                    tail_decoder,
                    cur_line,
                    ..
                } = self;
                tail_decoder.decode_into(&raw[start..end], cur_line);
            }
        }
    }

    /// Turn the live line into one or more committed rows.
    fn commit_line(&mut self, stamp: Option<i64>) {
        let text = std::mem::take(&mut self.cur_line);
        let prefix = stamp.map(format_timestamp);
        for row in wrap_text(&text, prefix.as_deref(), self.effective_cols()) {
            self.push_row(row);
        }
    }

    fn effective_cols(&self) -> usize {
        if !self.opts.wrap {
            return 0;
        }
        let per_row = self.opts.hex_bytes_per_row.max(1);
        match self.opts.mode {
            // Hex wraps on byte boundaries so rows stay aligned.
            ViewMode::Hex => per_row * 3 - 1,
            ViewMode::Text => self.max_cols,
        }
    }

    /// Recompute the wrapped rows of the live line.
    fn refresh_cur_rows(&mut self) {
        if self.cur_line.is_empty() {
            self.cur_rows.clear();
            return;
        }
        self.cur_rows = wrap_text(&self.cur_line, None, self.effective_cols());
        let widest = self
            .cur_rows
            .iter()
            .map(|row| display_width(row))
            .max()
            .unwrap_or(0);
        self.max_columns = self.max_columns.max(widest);
    }

    fn push_row(&mut self, row: String) {
        self.max_columns = self.max_columns.max(display_width(&row));
        self.rows.push_back(row);
        self.trim_rows();
    }

    fn trim_rows(&mut self) {
        while self.rows.len() > self.max_rows {
            self.rows.pop_front();
            self.first_row_id += 1;
        }
    }

    /// The earliest raw index still needed by in-progress state.
    fn live_start(&self) -> usize {
        match (self.opts.mode, self.opts.hex_style) {
            (ViewMode::Hex, HexStyle::Dump) => self.dump_row_start.min(self.cursor),
            _ => self.cursor,
        }
    }

    fn trim_raw(&mut self) {
        if self.raw.len() <= self.max_raw {
            return;
        }
        let excess = self.raw.len() - self.max_raw;
        let drop = excess.min(self.live_start());
        if drop == 0 {
            return;
        }
        self.raw.drain(..drop);
        self.raw_base += drop as u64;
        self.cursor -= drop;
        self.dump_row_start = self.dump_row_start.saturating_sub(drop);
        while let Some(first) = self.marks.first() {
            if first.offset < self.raw_base {
                self.marks.remove(0);
            } else {
                break;
            }
        }
    }

    /// Arrival time associated with an absolute stream offset.
    fn time_at(&self, offset: u64) -> i64 {
        match self.marks.binary_search_by(|m| m.offset.cmp(&offset)) {
            Ok(i) => self.marks[i].ms,
            Err(0) => self.marks.first().map(|m| m.ms).unwrap_or(0),
            Err(i) => self.marks[i - 1].ms,
        }
    }
}

/// Format epoch milliseconds as the `[HH:MM:SS.mmm]` prefix the tool uses.
fn format_timestamp(ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_millis_opt(ms).single() {
        Some(dt) => format!("[{}] ", dt.format("%H:%M:%S%.3f")),
        None => String::new(),
    }
}

/// Locate the next line terminator, returning `(offset, length)`.
///
/// Both CR and LF terminate a line; CRLF counts as a single terminator, which
/// is what makes the timestamp framing match the original tool.
fn find_terminator(bytes: &[u8]) -> Option<(usize, usize)> {
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'\n' => return Some((i, 1)),
            b'\r' => {
                if bytes.get(i + 1) == Some(&b'\n') {
                    return Some((i, 2));
                }
                // A CR sitting at the very end might be the first half of a
                // CRLF that got split across two reads.  Hold it back until the
                // next byte arrives rather than emitting a spurious empty line.
                if i + 1 == bytes.len() {
                    return None;
                }
                return Some((i, 1));
            }
            _ => {}
        }
    }
    None
}

/// Render one hex-dump row.
fn render_dump_row(bytes: &[u8], per_row: usize, offset: u64, upper: bool) -> String {
    let mut hex = String::with_capacity(per_row * 3);
    for i in 0..per_row {
        if i > 0 {
            hex.push(' ');
        }
        match bytes.get(i) {
            Some(b) if upper => hex.push_str(&format!("{b:02X}")),
            Some(b) => hex.push_str(&format!("{b:02x}")),
            None => hex.push_str("  "),
        }
    }
    let ascii: String = bytes
        .iter()
        .map(|b| {
            if (0x20..0x7f).contains(b) {
                *b as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{offset:08X}  {hex}  |{ascii}|")
}

/// Break text into display rows of at most `cols` columns.
///
/// `cols == 0` means "never wrap", which yields exactly one row.  Column counts
/// treat East Asian wide characters as two columns so CJK text aligns.
fn wrap_text(text: &str, prefix: Option<&str>, cols: usize) -> Vec<String> {
    if cols == 0 {
        return vec![match prefix {
            Some(p) => format!("{p}{text}"),
            None => text.to_owned(),
        }];
    }

    let mut rows: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut width = 0usize;

    if let Some(p) = prefix {
        current.push_str(p);
        width = display_width(p);
        // A prefix alone can exceed the width on a very narrow window; that is
        // fine, the text simply starts on the next row.
        if width >= cols {
            rows.push(std::mem::take(&mut current));
            width = 0;
        }
    }

    for ch in text.chars() {
        if ch == '\t' {
            // Expand tabs to the next 4-column stop so alignment is stable.
            let pad = 4 - (width % 4);
            for _ in 0..pad {
                if width + 1 > cols {
                    rows.push(std::mem::take(&mut current));
                    width = 0;
                }
                current.push(' ');
                width += 1;
            }
            continue;
        }
        let w = char_width(ch);
        if width + w > cols && width > 0 {
            rows.push(std::mem::take(&mut current));
            width = 0;
        }
        current.push(ch);
        width += w;
    }

    // Always emit the trailing row, even when empty, so an empty line still
    // occupies one row in the view.
    rows.push(current);
    rows
}

/// Display width of a string in monospace columns.
fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Monospace column width of a character (2 for East Asian wide/fullwidth).
pub fn char_width(ch: char) -> usize {
    let cp = ch as u32;
    // Zero-width and combining marks.
    if ch == '\u{200B}' || (0x0300..=0x036F).contains(&cp) || (0xFE00..=0xFE0F).contains(&cp) {
        return 0;
    }
    let wide = (0x1100..=0x115F).contains(&cp)      // Hangul Jamo
        || (0x2E80..=0x303E).contains(&cp)          // CJK radicals, Kangxi
        || (0x3041..=0x33FF).contains(&cp)          // Kana, CJK symbols
        || (0x3400..=0x4DBF).contains(&cp)          // CJK ext A
        || (0x4E00..=0x9FFF).contains(&cp)          // CJK unified
        || (0xA000..=0xA4CF).contains(&cp)          // Yi
        || (0xAC00..=0xD7A3).contains(&cp)          // Hangul syllables
        || (0xF900..=0xFAFF).contains(&cp)          // CJK compatibility
        || (0xFE30..=0xFE6F).contains(&cp)          // CJK compatibility forms
        || (0xFF00..=0xFF60).contains(&cp)          // Fullwidth forms
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x1F300..=0x1F64F).contains(&cp)        // Emoji
        || (0x1F900..=0x1F9FF).contains(&cp)
        || (0x20000..=0x3FFFD).contains(&cp); // CJK ext B and beyond
    if wide {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::{GBK, UTF_8};

    fn store() -> LogStore {
        LogStore::new(UTF_8)
    }

    fn rows(store: &LogStore) -> Vec<String> {
        (0..store.row_count())
            .map(|i| store.row(i).to_owned())
            .collect()
    }

    #[test]
    fn splits_lines_on_crlf() {
        let mut s = store();
        s.append(b"one\r\ntwo\r\nthree", 0);
        assert_eq!(rows(&s), vec!["one", "two", "three"]);
    }

    #[test]
    fn lone_cr_and_lone_lf_both_terminate() {
        let mut s = store();
        s.append(b"a\rb\nc", 0);
        assert_eq!(rows(&s), vec!["a", "b", "c"]);
    }

    #[test]
    fn crlf_split_across_chunks_is_one_terminator() {
        let mut s = store();
        s.append(b"hello\r", 0);
        assert_eq!(rows(&s), vec!["hello"]);
        s.append(b"\nworld", 0);
        assert_eq!(rows(&s), vec!["hello", "world"]);
    }

    #[test]
    fn multibyte_split_across_chunks_decodes() {
        let mut s = store();
        let (bytes, _, _) = GBK.encode("中文");
        s.set_encoding(GBK);
        for b in bytes.iter() {
            s.append(&[*b], 0);
        }
        assert_eq!(s.pending_line(), "中文");
    }

    #[test]
    fn hex_stream_mode_renders_bytes() {
        let mut s = store();
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            ..Default::default()
        });
        s.append(&[0x01, 0xab, 0xff], 0);
        assert_eq!(s.pending_line(), "01 AB FF");
        assert_eq!(rows(&s), vec!["01 AB FF"]);
    }

    #[test]
    fn hex_stream_continues_across_chunks_without_doubling_spaces() {
        let mut s = store();
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            ..Default::default()
        });
        s.append(&[0x01], 0);
        s.append(&[0x02], 0);
        assert_eq!(s.pending_line(), "01 02");
    }

    #[test]
    fn hex_stream_frames_per_line_when_timestamped() {
        let mut s = store();
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            timestamp: true,
            ..Default::default()
        });
        s.append(&[0x01, 0x02, b'\r', b'\n', 0x03, 0x04], 0);
        // One committed frame plus the live (unterminated) line.
        assert_eq!(s.row_count(), 2);
        assert!(s.row(0).ends_with("01 02"), "{}", s.row(0));
        assert_eq!(s.row(1), "03 04");
        assert_eq!(s.pending_line(), "03 04");
    }

    #[test]
    fn hex_dump_mode_frames_at_sixteen_bytes() {
        let mut s = store();
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            hex_style: HexStyle::Dump,
            hex_bytes_per_row: 16,
            ..Default::default()
        });
        let data: Vec<u8> = (0u8..20).collect();
        s.append(&data, 0);
        assert_eq!(s.row_count(), 2);
        assert!(s.row(0).starts_with("00000000  00 01"), "{}", s.row(0));
        assert!(s.row(1).starts_with("00000010  "), "{}", s.row(1));
        // A partial row must still render its ASCII gutter.
        assert!(s.row(1).ends_with("|....|"), "{}", s.row(1));
    }

    #[test]
    fn dump_partial_row_updates_in_place() {
        let mut s = store();
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            hex_style: HexStyle::Dump,
            ..Default::default()
        });
        s.append(&[0x41, 0x42], 0);
        assert_eq!(s.row_count(), 1);
        let first = s.row(0).to_owned();
        s.append(&[0x43], 0);
        // Still one row, now with three bytes.
        assert_eq!(s.row_count(), 1);
        assert_ne!(first, s.row(0));
        assert!(s.row(0).contains("41 42 43"), "{}", s.row(0));
    }

    #[test]
    fn toggling_hex_re_renders_history() {
        let mut s = store();
        s.append(b"AB", 0);
        assert_eq!(rows(&s), vec!["AB"]);
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            ..Default::default()
        });
        assert_eq!(rows(&s), vec!["41 42"]);
        s.set_options(DisplayOptions::default());
        assert_eq!(rows(&s), vec!["AB"]);
    }

    #[test]
    fn changing_encoding_re_renders_history() {
        let mut s = store();
        let (bytes, _, _) = GBK.encode("你好\r\n");
        s.append(&bytes, 0);
        s.set_encoding(GBK);
        assert_eq!(rows(&s), vec!["你好"]);
    }

    #[test]
    fn timestamp_prefixes_completed_frames() {
        let mut s = store();
        s.set_options(DisplayOptions {
            timestamp: true,
            ..Default::default()
        });
        s.append(b"frame1\r\n", 0);
        assert_eq!(s.row_count(), 1);
        let row = s.row(0);
        assert!(row.starts_with('['), "{row}");
        assert!(row.ends_with("frame1"), "{row}");
    }

    #[test]
    fn wrapping_respects_columns() {
        let mut s = store();
        s.set_options(DisplayOptions {
            wrap: true,
            ..Default::default()
        });
        s.set_width_columns(5);
        s.append(b"abcdefghij\r\n", 0);
        assert_eq!(rows(&s), vec!["abcde", "fghij"]);
    }

    #[test]
    fn wrap_counts_wide_chars_as_two_columns() {
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('中'), 2);
        assert_eq!(char_width('\u{0301}'), 0);

        let rows = wrap_text("中文字", None, 4);
        assert_eq!(rows, vec!["中文", "字"]);
    }

    #[test]
    fn wrap_disabled_keeps_single_row() {
        let rows = wrap_text("a".repeat(500).as_str(), None, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 500);
    }

    #[test]
    fn live_line_preview_tracks_input_without_terminator() {
        let mut s = store();
        s.append(b"partial", 0);
        assert_eq!(s.pending_line(), "partial");
        s.append(b"-more", 0);
        assert_eq!(s.pending_line(), "partial-more");
        assert_eq!(s.row_count(), 1);
    }

    #[test]
    fn clear_resets_everything() {
        let mut s = store();
        s.append(b"data\r\n", 0);
        s.clear();
        assert_eq!(s.row_count(), 0);
        assert_eq!(s.retained_bytes(), 0);
        assert!(s.raw().is_empty());
    }

    #[test]
    fn row_ids_are_stable_across_trimming() {
        let mut s = store();
        s.set_limits(1024, 64 * 1024);
        for i in 0..5000 {
            s.append(format!("line{i}\r\n").as_bytes(), 0);
        }
        assert!(s.row_count() <= 1024);
        let last_id = s.row_id(s.row_count() - 1);
        // Appending more must not change the id of an existing row's position.
        let first_id = s.row_id(0);
        s.append(b"more\r\n", 0);
        assert!(s.row_id(0) >= first_id);
        assert_eq!(s.row_id(s.row_count() - 1), last_id + 1);
    }

    #[test]
    fn raw_buffer_is_bounded() {
        let mut s = store();
        // `set_limits` clamps to a sane minimum, so use a value above it.
        s.set_limits(1024, 64 * 1024);
        for _ in 0..1000 {
            s.append(&[b'x'; 256], 0);
        }
        assert!(
            s.retained_bytes() <= 64 * 1024,
            "retained {} bytes",
            s.retained_bytes()
        );
    }

    #[test]
    fn render_all_matches_mode() {
        let mut s = store();
        s.append(b"AB\r\n", 0);
        assert_eq!(s.render_all(), "AB\r\n");
        s.set_options(DisplayOptions {
            mode: ViewMode::Hex,
            ..Default::default()
        });
        assert_eq!(s.render_all(), "41 42 0D 0A");
    }

    #[test]
    fn find_terminator_variants() {
        assert_eq!(find_terminator(b"abc"), None);
        assert_eq!(find_terminator(b"a\nb"), Some((1, 1)));
        assert_eq!(find_terminator(b"a\r\nb"), Some((1, 2)));
        assert_eq!(find_terminator(b"a\rb"), Some((1, 1)));
        // A trailing CR is held back: it may be the start of a CRLF.
        assert_eq!(find_terminator(b"\r"), None);
        assert_eq!(find_terminator(b"ab\r"), None);
    }
}
