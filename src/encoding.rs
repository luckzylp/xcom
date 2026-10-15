//! Character encoding support: manual selection plus automatic detection.
//!
//! Serial devices are wildly inconsistent about text encoding: 8051/STM32
//! firmware most often emits GBK, ESP-IDF emits UTF-8, Japanese modules emit
//! Shift_JIS.  The original XCOM tool made the user pick, which is easy to get
//! wrong.  This module keeps that manual control but adds an adaptive detector
//! that locks on to the most likely encoding as bytes arrive.

use encoding_rs::{
    BIG5, EUC_JP, EUC_KR, Encoding, GB18030, GBK, ISO_8859_2, ISO_8859_5, KOI8_R, SHIFT_JIS, UTF_8,
    WINDOWS_1251, WINDOWS_1252,
};
use serde::{Deserialize, Serialize};

/// A text encoding the user can select explicitly.
///
/// Deliberately excludes UTF-16: the log view frames lines on single `\n`/`\r`
/// bytes, and UTF-16 would break that (not to mention that no serial device
/// speaks UTF-16 in practice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EncodingId {
    Utf8,
    Gbk,
    Gb18030,
    Big5,
    ShiftJis,
    EucJp,
    EucKr,
    Windows1252,
    Windows1251,
    Iso8859_2,
    Iso8859_5,
    Koi8R,
}

impl EncodingId {
    /// Every selectable encoding, in menu order.
    pub const ALL: [EncodingId; 12] = [
        EncodingId::Utf8,
        EncodingId::Gbk,
        EncodingId::Gb18030,
        EncodingId::Big5,
        EncodingId::ShiftJis,
        EncodingId::EucJp,
        EncodingId::EucKr,
        EncodingId::Windows1252,
        EncodingId::Windows1251,
        EncodingId::Iso8859_2,
        EncodingId::Iso8859_5,
        EncodingId::Koi8R,
    ];

    /// The backing `encoding_rs` implementation.
    pub fn encoding(self) -> &'static Encoding {
        match self {
            EncodingId::Utf8 => UTF_8,
            EncodingId::Gbk => GBK,
            EncodingId::Gb18030 => GB18030,
            EncodingId::Big5 => BIG5,
            EncodingId::ShiftJis => SHIFT_JIS,
            EncodingId::EucJp => EUC_JP,
            EncodingId::EucKr => EUC_KR,
            EncodingId::Windows1252 => WINDOWS_1252,
            EncodingId::Windows1251 => WINDOWS_1251,
            EncodingId::Iso8859_2 => ISO_8859_2,
            EncodingId::Iso8859_5 => ISO_8859_5,
            EncodingId::Koi8R => KOI8_R,
        }
    }

    /// Human readable label used in the UI and in saved logs.
    pub fn label(self) -> &'static str {
        match self {
            EncodingId::Utf8 => "UTF-8",
            EncodingId::Gbk => "GBK (简体中文)",
            EncodingId::Gb18030 => "GB18030 (简体中文)",
            EncodingId::Big5 => "Big5 (繁体中文)",
            EncodingId::ShiftJis => "Shift_JIS (日文)",
            EncodingId::EucJp => "EUC-JP (日文)",
            EncodingId::EucKr => "EUC-KR (韩文)",
            EncodingId::Windows1252 => "Windows-1252 (西欧)",
            EncodingId::Windows1251 => "Windows-1251 (西里尔)",
            EncodingId::Iso8859_2 => "ISO-8859-2 (中欧)",
            EncodingId::Iso8859_5 => "ISO-8859-5 (西里尔)",
            EncodingId::Koi8R => "KOI8-R (俄文)",
        }
    }

    /// Short label, for the status bar.
    pub fn short_label(self) -> &'static str {
        match self {
            EncodingId::Utf8 => "UTF-8",
            EncodingId::Gbk => "GBK",
            EncodingId::Gb18030 => "GB18030",
            EncodingId::Big5 => "Big5",
            EncodingId::ShiftJis => "Shift_JIS",
            EncodingId::EucJp => "EUC-JP",
            EncodingId::EucKr => "EUC-KR",
            EncodingId::Windows1252 => "CP1252",
            EncodingId::Windows1251 => "CP1251",
            EncodingId::Iso8859_2 => "8859-2",
            EncodingId::Iso8859_5 => "8859-5",
            EncodingId::Koi8R => "KOI8-R",
        }
    }

    /// Map a `chardetng` result back onto our menu, when it is one of ours.
    pub fn from_encoding(enc: &'static Encoding) -> Option<EncodingId> {
        EncodingId::ALL
            .into_iter()
            .find(|id| id.encoding().name() == enc.name())
    }
}

/// How the receive path turns bytes into text.
///
/// `None` means "detect automatically"; `Some(id)` is an explicit override.
pub type EncodingChoice = Option<EncodingId>;

/// Automatically detect the encoding of an incoming byte stream.
///
/// The detector is intentionally conservative: once it has seen enough data and
/// the current guess decodes cleanly it stops reconsidering, so the display does
/// not flip between encodings while data is streaming in.
pub struct AutoDetector {
    detector: chardetng::EncodingDetector,
    /// Recent bytes, used for the UTF-8 fast path and mojibake scoring.
    window: Vec<u8>,
    /// Total bytes ever fed (monotonic, even after the window is trimmed).
    total: usize,
    active: &'static Encoding,
}

/// Bytes of evidence after which the detector stops flip-flopping.
const LOCK_BYTES: usize = 16 * 1024;
/// Below this much data we distrust `chardetng`'s Western default.
const MIN_BYTES: usize = 256;
/// Upper bound on the retained sample window.
const WINDOW_BYTES: usize = 64 * 1024;
/// Fraction of replacement characters tolerated before re-detecting.
const MOJIBAKE_TOLERANCE: f64 = 0.01;

impl Default for AutoDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoDetector {
    pub fn new() -> Self {
        Self {
            // ISO-2022-JP is a stateful 7-bit encoding that no serial device
            // emits, and we do not offer it as a manual choice, so exclude it.
            detector: chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny),
            window: Vec::new(),
            total: 0,
            // ASCII-only traffic (the common case for AT command sets) is valid
            // in every encoding we support, so UTF-8 is a safe initial guess.
            active: UTF_8,
        }
    }

    /// Forget everything and start over (used when the port is reopened).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// The encoding currently believed to be correct.
    pub fn active(&self) -> &'static Encoding {
        self.active
    }

    /// Total number of bytes analysed so far.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Feed newly received bytes; returns the encoding to decode them with.
    pub fn feed(&mut self, chunk: &[u8]) -> &'static Encoding {
        if chunk.is_empty() {
            return self.active;
        }
        self.total += chunk.len();
        self.detector.feed(chunk, false);

        self.window.extend_from_slice(chunk);
        if self.window.len() > WINDOW_BYTES {
            let mut cut = self.window.len() - WINDOW_BYTES;
            // Never cut a UTF-8 sequence in half: a truncated window would fail
            // the validity check below and trigger a bogus re-detection.
            while cut < self.window.len() && (self.window[cut] & 0xC0) == 0x80 {
                cut += 1;
            }
            self.window.drain(..cut);
        }

        // 1. Pure ASCII is valid everywhere: never churn on it.
        if self.window.iter().all(|b| b.is_ascii()) {
            return self.active;
        }

        // 2. Well-formed multi-byte UTF-8 is, in practice, always UTF-8.  This
        //    check is what prevents chardetng from mislabelling short Chinese
        //    UTF-8 fragments as GBK.
        if self.window.len() >= 8 {
            if let Ok(text) = std::str::from_utf8(&self.window) {
                if !text.is_ascii() {
                    return self.set(UTF_8);
                }
            }
        }

        // 3. Once locked in, only reconsider when the active encoding is
        //    visibly producing garbage.
        if self.total >= LOCK_BYTES && self.mojibake_ratio(self.active) <= MOJIBAKE_TOLERANCE {
            return self.active;
        }

        // We run our own UTF-8 fast path above, so anything reaching this point
        // is *not* well-formed UTF-8 and must not be guessed as such.
        let guess = self.detector.guess(None, chardetng::Utf8Detection::Deny);

        // 4. Short samples fall back to windows-1252 too eagerly.
        if self.total < MIN_BYTES && guess == WINDOWS_1252 && self.active != WINDOWS_1252 {
            return self.active;
        }

        // 5. Do not swap to a guess that decodes worse than what we have.
        if guess != self.active && self.mojibake_ratio(guess) > self.mojibake_ratio(self.active) {
            return self.active;
        }

        self.set(guess)
    }

    fn set(&mut self, enc: &'static Encoding) -> &'static Encoding {
        self.active = enc;
        enc
    }

    /// Share of characters that failed to decode, in `0.0..=1.0`.
    fn mojibake_ratio(&self, enc: &'static Encoding) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        let (text, _, had_errors) = enc.decode(&self.window);
        if !had_errors {
            return 0.0;
        }
        let bad = text.chars().filter(|c| *c == '\u{FFFD}').count();
        bad as f64 / text.chars().count().max(1) as f64
    }
}

/// Decode a full byte slice, substituting U+FFFD for malformed input.
pub fn decode(input: &[u8], enc: &'static Encoding) -> String {
    if input.is_empty() {
        return String::new();
    }
    let (text, _, _) = enc.decode(input);
    text.into_owned()
}

/// Encode text for transmission, substituting `?` for unmappable characters.
pub fn encode(input: &str, enc: &'static Encoding) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }
    let (bytes, _, _) = enc.encode(input);
    bytes.into_owned()
}

/// Incremental decoder that survives multi-byte sequences split across reads.
pub struct StreamDecoder {
    decoder: encoding_rs::Decoder,
    /// Encoding the decoder was built for; a change requires a rebuild.
    encoding: &'static Encoding,
}

impl StreamDecoder {
    pub fn new(encoding: &'static Encoding) -> Self {
        Self {
            decoder: encoding.new_decoder(),
            encoding,
        }
    }

    /// Decode `input`, appending to `out`.
    pub fn decode_into(&mut self, input: &[u8], out: &mut String) {
        if input.is_empty() {
            return;
        }
        // `decode_to_string_without_replacement` never grows the destination
        // beyond what is needed, but it requires the capacity up front.
        let needed = self
            .decoder
            .max_utf8_buffer_length_without_replacement(input.len());
        let needed = needed.unwrap_or(input.len() * 3 + 16);
        out.reserve(needed);
        // The destination must report the right remaining capacity, otherwise
        // the decoder stops early.  The returned tuple's second value is the
        // number of bytes read; a short read means the buffer was too small.
        let (_result, read) = self
            .decoder
            .decode_to_string_without_replacement(input, out, false);
        if read < input.len() {
            // Should not happen given the capacity reservation above; fall back
            // to a direct decode so no bytes are silently lost.
            let (text, _, _) = self.encoding.decode(&input[read..]);
            out.push_str(&text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_stable_under_auto_detect() {
        let mut d = AutoDetector::new();
        for _ in 0..100 {
            d.feed(b"AT+GMR\r\nOK\r\n");
        }
        assert_eq!(d.active(), UTF_8);
    }

    #[test]
    fn detects_utf8_chinese() {
        let text = "温度：25.6℃ 湿度：60%\r\n".repeat(20);
        let mut d = AutoDetector::new();
        // Feed in small chunks to mimic a real serial stream.
        for chunk in text.as_bytes().chunks(7) {
            d.feed(chunk);
        }
        assert_eq!(d.active(), UTF_8, "utf-8 chinese must stay utf-8");
    }

    #[test]
    fn detects_gbk_chinese() {
        let text = "温度传感器读取失败，请检查接线！\r\n".repeat(40);
        let (gbk_bytes, _, _) = GBK.encode(&text);
        let mut d = AutoDetector::new();
        for chunk in gbk_bytes.chunks(11) {
            d.feed(chunk);
        }
        assert_eq!(
            d.active(),
            GBK,
            "gbk payload should be detected (got {})",
            d.active().name()
        );
    }

    #[test]
    fn stream_decoder_handles_split_multibyte() {
        let text = "中文测试";
        let (bytes, _, _) = GBK.encode(text);
        let mut dec = StreamDecoder::new(GBK);
        let mut out = String::new();
        for b in bytes.iter() {
            dec.decode_into(&[*b], &mut out);
        }
        assert_eq!(out, text);
    }

    #[test]
    fn stream_decoder_handles_split_utf8() {
        let text = "你好世界";
        let bytes = text.as_bytes();
        let mut dec = StreamDecoder::new(UTF_8);
        let mut out = String::new();
        for b in bytes {
            dec.decode_into(&[*b], &mut out);
        }
        assert_eq!(out, text);
    }

    #[test]
    fn encode_round_trip_gbk() {
        let text = "正点原子";
        let bytes = encode(text, GBK);
        assert_eq!(decode(&bytes, GBK), text);
    }

    #[test]
    fn every_encoding_has_a_label() {
        for id in EncodingId::ALL {
            assert!(!id.label().is_empty());
            assert!(!id.short_label().is_empty());
            assert_eq!(EncodingId::from_encoding(id.encoding()), Some(id));
        }
    }
}
