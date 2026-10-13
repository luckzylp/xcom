//! Checksums for the protocol-transfer feature.
//!
//! Four methods are offered, matching the original tool.  The exact algorithms
//! were recovered from the reference implementation rather than guessed:
//!
//! * `SUM` is *not* a plain additive checksum — it is the two's complement of
//!   the byte sum, so that the whole frame sums to zero modulo 256.
//! * `XOR` is the plain XOR of every preceding byte.
//! * `CRC8` is CRC-8/MAXIM (reflected polynomial 0x31, init 0x00), whose check
//!   value over `"123456789"` is `0xA1` — *not* the more common 0x07 CRC-8.
//! * `CRC16` is CRC-16/MODBUS (reflected 0xA001, init 0xFFFF) and is emitted
//!   low byte first, exactly like a Modbus RTU frame.
//!
//! Every method covers all preceding frame bytes, the length field included.

use serde::{Deserialize, Serialize};

/// Selectable frame check method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChecksumKind {
    /// 校验和: two's complement of the byte sum.
    Sum,
    /// 异或: XOR of all preceding bytes.
    Xor,
    /// CRC-8/MAXIM, polynomial 0x31 reflected, init 0x00.
    Crc8,
    /// CRC-16/MODBUS, polynomial 0xA001 reflected, init 0xFFFF, low byte first.
    Crc16,
}

impl ChecksumKind {
    pub const ALL: [ChecksumKind; 4] = [
        ChecksumKind::Sum,
        ChecksumKind::Xor,
        ChecksumKind::Crc8,
        ChecksumKind::Crc16,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ChecksumKind::Sum => "校验和",
            ChecksumKind::Xor => "异或",
            ChecksumKind::Crc8 => "CRC8",
            ChecksumKind::Crc16 => "CRC16",
        }
    }

    /// Number of bytes this method appends to a frame.
    pub fn width(self) -> usize {
        match self {
            ChecksumKind::Crc16 => 2,
            _ => 1,
        }
    }

    /// Compute the check value and append it to `out`.
    pub fn append(self, data: &[u8], out: &mut Vec<u8>) {
        match self {
            ChecksumKind::Sum => out.push(sum8(data)),
            ChecksumKind::Xor => out.push(xor8(data)),
            ChecksumKind::Crc8 => out.push(crc8(data)),
            ChecksumKind::Crc16 => out.extend_from_slice(&crc16(data).to_le_bytes()),
        }
    }

    /// Verify a frame whose trailing bytes are this checksum.
    ///
    /// `body` excludes the check bytes; `check` is what the peer sent.
    pub fn verify(self, body: &[u8], check: &[u8]) -> bool {
        if check.len() != self.width() {
            return false;
        }
        let mut expected = Vec::with_capacity(self.width());
        self.append(body, &mut expected);
        expected == check
    }
}

/// 校验和: the two's complement of the byte sum.
///
/// The reference implementation computes `256 - (sum % 256)`, which makes the
/// sum of the entire frame (data plus check byte) zero modulo 256.
pub fn sum8(data: &[u8]) -> u8 {
    0u8.wrapping_sub(data.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)))
}

/// 异或: XOR of every byte.
pub fn xor8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, b| acc ^ *b)
}

/// CRC-8/MAXIM table (reflected polynomial 0x31, represented as 0x8C).
fn crc8_table() -> &'static [u8; 256] {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[u8; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u8; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let mut crc = i as u8;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0x8C
                } else {
                    crc >> 1
                };
            }
            *slot = crc;
        }
        table
    })
}

/// CRC-16/MODBUS table (reflected polynomial 0xA001).
fn crc16_table() -> &'static [u16; 256] {
    use std::sync::OnceLock;
    static TABLE: OnceLock<[u16; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u16; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let mut crc = i as u16;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xA001
                } else {
                    crc >> 1
                };
            }
            *slot = crc;
        }
        table
    })
}

/// CRC-8 over `data`.
pub fn crc8(data: &[u8]) -> u8 {
    let table = crc8_table();
    data.iter().fold(0u8, |crc, b| table[(crc ^ *b) as usize])
}

/// CRC-16/MODBUS over `data`.
pub fn crc16(data: &[u8]) -> u16 {
    let table = crc16_table();
    data.iter().fold(0xFFFFu16, |crc, b| {
        let index = ((crc ^ *b as u16) & 0xFF) as usize;
        (crc >> 8) ^ table[index]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_is_twos_complement_so_the_frame_sums_to_zero() {
        assert_eq!(sum8(&[]), 0);
        // 256 - 6 == 250
        assert_eq!(sum8(&[1, 2, 3]), 0xFA);
        for data in [
            vec![0x01u8, 0x03],
            vec![0xFF, 0x02],
            vec![0x00, 0x00, 0x00, 0x0A],
            vec![0xAB; 37],
        ] {
            let check = sum8(&data);
            let total: u8 = data.iter().fold(check, |acc, b| acc.wrapping_add(*b));
            assert_eq!(total, 0, "frame {data:02X?} must sum to zero");
        }
    }

    #[test]
    fn xor_is_plain_xor() {
        assert_eq!(xor8(&[0x0F, 0xF0]), 0xFF);
        assert_eq!(xor8(&[0xAA, 0xAA]), 0x00);
        assert_eq!(xor8(&[]), 0x00);
    }

    #[test]
    fn crc8_is_crc8_maxim() {
        // The check value that distinguishes CRC-8/MAXIM (0x31) from the
        // far more common CRC-8 (0x07), which would give 0xF4 here.
        assert_eq!(crc8(b"123456789"), 0xA1);
    }

    #[test]
    fn crc16_is_modbus() {
        // Canonical CRC-16/MODBUS check value.
        assert_eq!(crc16(b"123456789"), 0x4B37);
    }

    #[test]
    fn crc16_matches_the_captured_reference_frame() {
        // addr=01 func=03 seq=0A len=04 data=00 00 00 0A  ->  CRC16 = C3 AA
        // on the wire (low byte first).  This frame was captured from real
        // hardware and is the strongest end-to-end vector we have.
        let frame = [0x01u8, 0x03, 0x0A, 0x04, 0x00, 0x00, 0x00, 0x0A];
        assert_eq!(crc16(&frame), 0xAAC3);
        let mut out = Vec::new();
        ChecksumKind::Crc16.append(&frame, &mut out);
        assert_eq!(out, vec![0xC3, 0xAA]);
    }

    #[test]
    fn widths_are_right() {
        assert_eq!(ChecksumKind::Sum.width(), 1);
        assert_eq!(ChecksumKind::Xor.width(), 1);
        assert_eq!(ChecksumKind::Crc8.width(), 1);
        assert_eq!(ChecksumKind::Crc16.width(), 2);
    }

    #[test]
    fn verify_round_trips_for_every_kind() {
        let body = b"\x01\x03\x00\x10\x00\x02";
        for kind in ChecksumKind::ALL {
            let mut frame = body.to_vec();
            kind.append(body, &mut frame);
            let (payload, check) = frame.split_at(body.len());
            assert!(kind.verify(payload, check), "{kind:?} should verify");
            // A corrupted check byte must fail.
            let mut bad = check.to_vec();
            bad[0] ^= 0xFF;
            assert!(!kind.verify(payload, &bad), "{kind:?} should reject");
        }
    }

    #[test]
    fn verify_rejects_wrong_length() {
        assert!(!ChecksumKind::Crc16.verify(b"abc", &[0x00]));
        assert!(!ChecksumKind::Sum.verify(b"abc", &[0x00, 0x00]));
    }

    #[test]
    fn tables_are_consistent_with_bitwise_reference() {
        // Recompute without tables to make sure the tables are not wrong.
        fn bitwise_crc8(data: &[u8]) -> u8 {
            data.iter().fold(0u8, |mut crc, b| {
                crc ^= *b;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0x8C
                    } else {
                        crc >> 1
                    };
                }
                crc
            })
        }
        fn bitwise_crc16(data: &[u8]) -> u16 {
            data.iter().fold(0xFFFFu16, |mut crc, b| {
                crc ^= *b as u16;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xA001
                    } else {
                        crc >> 1
                    };
                }
                crc
            })
        }
        for len in 0..64usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(crc8(&data), bitwise_crc8(&data), "crc8 len {len}");
            assert_eq!(crc16(&data), bitwise_crc16(&data), "crc16 len {len}");
        }
    }
}
