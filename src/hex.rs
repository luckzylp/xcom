//! Hexadecimal parsing and formatting helpers.
//!
//! The receive/send logic needs a *lenient* hex parser that matches what the
//! original XCOM tool accepts: the user may type `01 02 03`, `010203`,
//! `0x01,0x02` or `01-02-03` and all of them must yield the same three bytes.

use std::fmt;

/// Errors produced while parsing a hex string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HexError {
    /// A character that is neither a hex digit nor a permitted separator.
    InvalidChar { ch: char, index: usize },
    /// An odd number of hex digits: the trailing nibble is incomplete.
    OddDigits { index: usize, digit: char },
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexError::InvalidChar { ch, index } => {
                write!(
                    f,
                    "位置 {} 处的字符 {ch:?} 不是合法的十六进制字符",
                    index + 1
                )
            }
            HexError::OddDigits { index, digit } => {
                write!(
                    f,
                    "位置 {} 处剩余半个字节 {digit:?}，十六进制字符个数必须为偶数",
                    index + 1
                )
            }
        }
    }
}

impl std::error::Error for HexError {}

/// Characters that are silently skipped by [`parse`].
#[inline]
fn is_separator(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ',' | ';' | '-' | '_' | ':' | '|')
}

/// Parse a lenient hex string into raw bytes.
///
/// Separators (whitespace and most punctuation) are ignored and `0x` prefixes
/// are accepted, so byte auto-splitting works exactly like the original tool.
pub fn parse(input: &str) -> Result<Vec<u8>, HexError> {
    let mut out = Vec::with_capacity(input.len() / 2 + 1);
    // High nibble of the byte currently being assembled.
    let mut hi: Option<u8> = None;
    // True when the last hex digit consumed was a lone '0', which may be the
    // start of a "0x" prefix.
    let mut zero_pending = false;

    for (index, ch) in input.char_indices() {
        if is_separator(ch) {
            zero_pending = false;
            continue;
        }
        if ch == 'x' || ch == 'X' {
            if zero_pending && hi == Some(0) {
                hi = None;
                zero_pending = false;
                continue;
            }
            return Err(HexError::InvalidChar { ch, index });
        }
        let digit = match ch.to_digit(16) {
            Some(d) => d as u8,
            None => return Err(HexError::InvalidChar { ch, index }),
        };
        match hi.take() {
            None => {
                hi = Some(digit);
                zero_pending = digit == 0;
            }
            Some(high) => {
                out.push((high << 4) | digit);
                zero_pending = false;
            }
        }
    }

    if let Some(high) = hi {
        return Err(HexError::OddDigits {
            index: input.len(),
            digit: char::from_digit(high as u32, 16).unwrap_or('?'),
        });
    }
    Ok(out)
}

/// Render bytes as space separated hex, e.g. `01 0A FF`.
pub fn to_hex_string(bytes: &[u8], upper: bool) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        if upper {
            s.push_str(&format!("{b:02X}"));
        } else {
            s.push_str(&format!("{b:02x}"));
        }
    }
    s
}

/// One row of a classic 16-byte hex dump.
pub struct DumpRow {
    pub offset: usize,
    pub hex: String,
    pub ascii: String,
}

/// Produce a classic `offset  hex bytes  |ascii|` dump.
pub fn to_dump(bytes: &[u8], per_row: usize, base_offset: u64, upper: bool) -> Vec<DumpRow> {
    let per_row = per_row.max(1);
    let mut rows = Vec::with_capacity(bytes.len() / per_row + 1);
    for (chunk_index, chunk) in bytes.chunks(per_row).enumerate() {
        let offset = base_offset as usize + chunk_index * per_row;
        let mut hex = String::with_capacity(per_row * 3);
        for i in 0..per_row {
            if i > 0 {
                hex.push(' ');
            }
            match chunk.get(i) {
                Some(b) if upper => hex.push_str(&format!("{b:02X}")),
                Some(b) => hex.push_str(&format!("{b:02x}")),
                None => hex.push_str("  "),
            }
        }
        let ascii: String = chunk
            .iter()
            .map(|b| {
                if (0x20..0x7f).contains(b) {
                    *b as char
                } else {
                    '.'
                }
            })
            .collect();
        rows.push(DumpRow { offset, hex, ascii });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loose_forms() {
        let want = vec![0x01, 0x02, 0x03];
        for form in [
            "01 02 03",
            "010203",
            "0x01,0x02,0x03",
            "01-02-03",
            " 01\n02\t03 ",
        ] {
            assert_eq!(parse(form).unwrap(), want, "form {form:?}");
        }
    }

    #[test]
    fn parses_uppercase_and_mixed() {
        assert_eq!(parse("aAbB").unwrap(), vec![0xaa, 0xbb]);
        assert_eq!(parse("0XFF").unwrap(), vec![0xff]);
    }

    #[test]
    fn accepts_empty() {
        assert_eq!(parse("").unwrap(), Vec::<u8>::new());
        assert_eq!(parse("   ").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn rejects_odd_digit_count() {
        assert!(matches!(parse("010"), Err(HexError::OddDigits { .. })));
    }

    #[test]
    fn rejects_non_hex() {
        assert!(matches!(parse("01ZZ"), Err(HexError::InvalidChar { .. })));
        // A stray 'x' that is not part of a 0x prefix is invalid.
        assert!(matches!(parse("x1"), Err(HexError::InvalidChar { .. })));
    }

    #[test]
    fn hex_string_round_trip() {
        let bytes = vec![0x00, 0x0a, 0xff];
        assert_eq!(to_hex_string(&bytes, false), "00 0a ff");
        assert_eq!(to_hex_string(&bytes, true), "00 0A FF");
        assert_eq!(parse(&to_hex_string(&bytes, true)).unwrap(), bytes);
    }

    #[test]
    fn dump_rows_align() {
        let bytes: Vec<u8> = (0..20).collect();
        let rows = to_dump(&bytes, 16, 0, true);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].offset, 0);
        assert_eq!(rows[1].offset, 16);
        assert_eq!(rows[0].ascii, "................");
    }
}
