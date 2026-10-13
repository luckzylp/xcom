//! Wire format for the protocol-transfer feature.
//!
//! ```text
//! +---------+---------+---------+---------+-----------+----------------+
//! | 设备地址 | 帧功能   | 帧序列   | 数据长度 | 数据域     | 校验            |
//! | 1 byte  | 1 byte  | 1 byte  | 1 byte  | 0..255 B  | 1 or 2 bytes   |
//! +---------+---------+---------+---------+-----------+----------------+
//! ```
//!
//! The length field counts **only** the data domain.  The checksum covers every
//! preceding byte, the length field included.  There are no delimiter bytes:
//! a frame ends when the line goes idle, which is why
//! [`idle_timeout_ms`] exists.

use super::checksum::ChecksumKind;

/// Smallest legal frame: header (4) + one checksum byte.
pub const MIN_FRAME: usize = 5;
/// Largest legal frame: header (4) + 255 data + two checksum bytes.
pub const MAX_FRAME: usize = 4 + 255 + 2;
/// Largest data domain a single frame may carry.
pub const MAX_DATA: usize = 255;

/// Inter-byte idle threshold used to delimit a response frame.
///
/// Recovered from the reference implementation: `320000 / baud + 2` ms, which
/// is roughly the time to shift 32 characters plus a small margin.
pub fn idle_timeout_ms(baud: u32) -> u64 {
    if baud == 0 {
        return 50;
    }
    (320_000u64 / baud as u64) + 2
}

/// An outgoing frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub address: u8,
    pub function: u8,
    pub sequence: u8,
    pub data: Vec<u8>,
    pub checksum: ChecksumKind,
}

impl Frame {
    /// Serialise to bytes.
    pub fn build(&self) -> Vec<u8> {
        let data_len = self.data.len().min(MAX_DATA);
        let mut out = Vec::with_capacity(4 + data_len + self.checksum.width());
        out.push(self.address);
        out.push(self.function);
        out.push(self.sequence);
        out.push(data_len as u8);
        out.extend_from_slice(&self.data[..data_len]);
        // Compute the check into a scratch buffer: `out` is borrowed immutably
        // as the input and mutably as the destination otherwise.
        let mut check = Vec::with_capacity(self.checksum.width());
        self.checksum.append(&out, &mut check);
        out.extend_from_slice(&check);
        out
    }
}

/// A frame received from the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedFrame {
    pub address: u8,
    pub function: u8,
    pub sequence: u8,
    pub data: Vec<u8>,
    /// The check bytes exactly as they arrived.
    pub check: Vec<u8>,
    /// The complete frame as received.
    pub raw: Vec<u8>,
}

/// Why a received frame was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Shorter than [`MIN_FRAME`].
    TooShort(usize),
    /// Longer than [`MAX_FRAME`].
    TooLong(usize),
    /// The declared data length does not fit the bytes received.
    Truncated { declared: usize, available: usize },
    /// The trailing check field is neither 1 nor 2 bytes.
    BadCheckLength(usize),
    /// The check field width does not match the selected method.
    CheckWidthMismatch { expected: usize, found: usize },
    /// The check value itself did not match.
    ChecksumMismatch { expected: Vec<u8>, found: Vec<u8> },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooShort(n) => write!(
                f,
                "{}",
                crate::i18n::tr2("帧长度 {} 小于最小长度 {}", n, MIN_FRAME)
            ),
            FrameError::TooLong(n) => write!(
                f,
                "{}",
                crate::i18n::tr2("帧长度 {} 超过最大长度 {}", n, MAX_FRAME)
            ),
            FrameError::Truncated {
                declared,
                available,
            } => write!(
                f,
                "{}",
                crate::i18n::tr2("数据长度字段为 {}，但只剩 {} 字节", declared, available)
            ),
            FrameError::BadCheckLength(n) => write!(
                f,
                "{}",
                crate::i18n::tr1("校验字段长度 {} 非法（应为 1 或 2）", n)
            ),
            FrameError::CheckWidthMismatch { expected, found } => {
                write!(
                    f,
                    "{}",
                    crate::i18n::tr2("校验字段长度 {} 与所选校验方式 ({}) 不符", found, expected)
                )
            }
            FrameError::ChecksumMismatch { expected, found } => write!(
                f,
                "{}",
                crate::i18n::tr2(
                    "校验错误：期望 {}，实际 {}",
                    hex_join(expected),
                    hex_join(found)
                )
            ),
        }
    }
}

impl std::error::Error for FrameError {}

fn hex_join(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse and verify a complete frame.
pub fn parse(bytes: &[u8], checksum: ChecksumKind) -> Result<ReceivedFrame, FrameError> {
    if bytes.len() < MIN_FRAME {
        return Err(FrameError::TooShort(bytes.len()));
    }
    if bytes.len() > MAX_FRAME {
        return Err(FrameError::TooLong(bytes.len()));
    }

    let declared = bytes[3] as usize;
    let available = bytes.len() - 4;
    if declared > available {
        return Err(FrameError::Truncated {
            declared,
            available,
        });
    }

    let check_len = available - declared;
    if check_len == 0 || check_len > 2 {
        return Err(FrameError::BadCheckLength(check_len));
    }
    if check_len != checksum.width() {
        return Err(FrameError::CheckWidthMismatch {
            expected: checksum.width(),
            found: check_len,
        });
    }

    let body_end = 4 + declared;
    let body = &bytes[..body_end];
    let check = &bytes[body_end..];

    if !checksum.verify(body, check) {
        // Only build the expected value when reporting, so the happy path does
        // not allocate.
        let mut expected = Vec::with_capacity(checksum.width());
        checksum.append(body, &mut expected);
        return Err(FrameError::ChecksumMismatch {
            expected,
            found: check.to_vec(),
        });
    }

    Ok(ReceivedFrame {
        address: bytes[0],
        function: bytes[1],
        sequence: bytes[2],
        data: bytes[4..body_end].to_vec(),
        check: check.to_vec(),
        raw: bytes.to_vec(),
    })
}

/// Check whether a received frame acknowledges a given sequence number.
///
/// Only the sequence number is validated.  The reference tool explicitly does
/// *not* check the address or function code of a response — its own
/// documentation labels those fields 随意 ("arbitrary") — so neither do we.
pub fn is_ack_for(frame: &ReceivedFrame, sequence: u8) -> bool {
    frame.sequence == sequence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_documented_examples() {
        let make = |checksum| Frame {
            address: 0x01,
            function: 0x03,
            sequence: 0x0A,
            data: vec![0x00, 0x00, 0x00, 0x0A],
            checksum,
        };
        assert_eq!(
            make(ChecksumKind::Sum).build(),
            vec![0x01, 0x03, 0x0A, 0x04, 0x00, 0x00, 0x00, 0x0A, 0xE4]
        );
        assert_eq!(
            make(ChecksumKind::Xor).build(),
            vec![0x01, 0x03, 0x0A, 0x04, 0x00, 0x00, 0x00, 0x0A, 0x06]
        );
        assert_eq!(
            make(ChecksumKind::Crc8).build(),
            vec![0x01, 0x03, 0x0A, 0x04, 0x00, 0x00, 0x00, 0x0A, 0xAA]
        );
        assert_eq!(
            make(ChecksumKind::Crc16).build(),
            vec![0x01, 0x03, 0x0A, 0x04, 0x00, 0x00, 0x00, 0x0A, 0xC3, 0xAA]
        );
    }

    #[test]
    fn builds_the_documented_empty_ack_examples() {
        let make = |checksum| Frame {
            address: 0x01,
            function: 0x03,
            sequence: 0x0A,
            data: vec![],
            checksum,
        };
        assert_eq!(
            make(ChecksumKind::Sum).build(),
            vec![0x01, 0x03, 0x0A, 0x00, 0xF2]
        );
        assert_eq!(
            make(ChecksumKind::Crc8).build(),
            vec![0x01, 0x03, 0x0A, 0x00, 0x8C]
        );
        assert_eq!(
            make(ChecksumKind::Crc16).build(),
            vec![0x01, 0x03, 0x0A, 0x00, 0xF7, 0x78]
        );
    }

    #[test]
    fn round_trips_every_checksum() {
        for checksum in ChecksumKind::ALL {
            let frame = Frame {
                address: 0x7F,
                function: 0xA5,
                sequence: 0x33,
                data: vec![1, 2, 3, 4, 5],
                checksum,
            };
            let bytes = frame.build();
            let parsed = parse(&bytes, checksum).unwrap();
            assert_eq!(parsed.address, 0x7F);
            assert_eq!(parsed.function, 0xA5);
            assert_eq!(parsed.sequence, 0x33);
            assert_eq!(parsed.data, vec![1, 2, 3, 4, 5]);
            assert_eq!(parsed.raw, bytes);
        }
    }

    #[test]
    fn round_trips_zero_and_max_data() {
        for len in [0usize, 1, 254, 255] {
            let frame = Frame {
                address: 1,
                function: 2,
                sequence: 3,
                data: vec![0x5A; len],
                checksum: ChecksumKind::Crc16,
            };
            let bytes = frame.build();
            assert_eq!(bytes.len(), 4 + len + 2);
            let parsed = parse(&bytes, ChecksumKind::Crc16).unwrap();
            assert_eq!(parsed.data.len(), len);
        }
    }

    #[test]
    fn rejects_corrupted_checksum() {
        let mut bytes = Frame {
            address: 1,
            function: 2,
            sequence: 3,
            data: vec![9, 9],
            checksum: ChecksumKind::Crc16,
        }
        .build();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            parse(&bytes, ChecksumKind::Crc16),
            Err(FrameError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn rejects_short_and_long_frames() {
        assert!(matches!(
            parse(&[0x01, 0x02, 0x03], ChecksumKind::Sum),
            Err(FrameError::TooShort(3))
        ));
        let long = vec![0u8; MAX_FRAME + 1];
        assert!(matches!(
            parse(&long, ChecksumKind::Sum),
            Err(FrameError::TooLong(_))
        ));
    }

    #[test]
    fn rejects_truncated_data_field() {
        // Declares 10 data bytes but supplies none.
        let bytes = [0x01u8, 0x03, 0x0A, 0x0A, 0xE4];
        assert!(matches!(
            parse(&bytes, ChecksumKind::Sum),
            Err(FrameError::Truncated { declared: 10, .. })
        ));
    }

    #[test]
    fn rejects_wrong_check_width_for_method() {
        let bytes = Frame {
            address: 1,
            function: 2,
            sequence: 3,
            data: vec![],
            checksum: ChecksumKind::Sum,
        }
        .build();
        assert!(matches!(
            parse(&bytes, ChecksumKind::Crc16),
            Err(FrameError::CheckWidthMismatch { .. })
        ));
    }

    #[test]
    fn ack_matches_on_sequence_only() {
        let ack = ReceivedFrame {
            address: 0xFF,
            function: 0xFF,
            sequence: 5,
            data: vec![],
            check: vec![0],
            raw: vec![],
        };
        assert!(is_ack_for(&ack, 5));
        assert!(!is_ack_for(&ack, 6));
    }

    #[test]
    fn idle_timeout_scales_with_baud() {
        assert_eq!(idle_timeout_ms(9600), 35);
        assert_eq!(idle_timeout_ms(115200), 4);
        assert!(idle_timeout_ms(0) > 0);
    }
}
