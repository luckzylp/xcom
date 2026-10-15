//! Serial port configuration model and its mapping onto the OS driver.

use serde::{Deserialize, Serialize};
use serialport::{DataBits, FlowControl, Parity, StopBits};

/// Number of data bits per character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataBitsSel {
    Five,
    Six,
    Seven,
    Eight,
}

impl DataBitsSel {
    pub const ALL: [DataBitsSel; 4] = [
        DataBitsSel::Five,
        DataBitsSel::Six,
        DataBitsSel::Seven,
        DataBitsSel::Eight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DataBitsSel::Five => "5",
            DataBitsSel::Six => "6",
            DataBitsSel::Seven => "7",
            DataBitsSel::Eight => "8",
        }
    }

    pub fn bits(self) -> u8 {
        match self {
            DataBitsSel::Five => 5,
            DataBitsSel::Six => 6,
            DataBitsSel::Seven => 7,
            DataBitsSel::Eight => 8,
        }
    }

    fn to_driver(self) -> DataBits {
        match self {
            DataBitsSel::Five => DataBits::Five,
            DataBitsSel::Six => DataBits::Six,
            DataBitsSel::Seven => DataBits::Seven,
            DataBitsSel::Eight => DataBits::Eight,
        }
    }
}

/// Number of stop bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopBitsSel {
    One,
    OnePointFive,
    Two,
}

impl StopBitsSel {
    pub const ALL: [StopBitsSel; 3] = [
        StopBitsSel::One,
        StopBitsSel::OnePointFive,
        StopBitsSel::Two,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StopBitsSel::One => "1",
            StopBitsSel::OnePointFive => "1.5",
            StopBitsSel::Two => "2",
        }
    }
}

/// Parity checking mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParitySel {
    None,
    Odd,
    Even,
}

impl ParitySel {
    pub const ALL: [ParitySel; 3] = [ParitySel::None, ParitySel::Odd, ParitySel::Even];

    pub fn label(self) -> &'static str {
        match self {
            ParitySel::None => "无",
            ParitySel::Odd => "奇校验",
            ParitySel::Even => "偶校验",
        }
    }

    fn to_driver(self) -> Parity {
        match self {
            ParitySel::None => Parity::None,
            ParitySel::Odd => Parity::Odd,
            ParitySel::Even => Parity::Even,
        }
    }
}

/// Hardware/software flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowSel {
    None,
    Software,
    Hardware,
}

impl FlowSel {
    pub const ALL: [FlowSel; 3] = [FlowSel::None, FlowSel::Software, FlowSel::Hardware];

    pub fn label(self) -> &'static str {
        match self {
            FlowSel::None => "无",
            FlowSel::Software => "软件 (XON/XOFF)",
            FlowSel::Hardware => "硬件 (RTS/CTS)",
        }
    }

    fn to_driver(self) -> FlowControl {
        match self {
            FlowSel::None => FlowControl::None,
            FlowSel::Software => FlowControl::Software,
            FlowSel::Hardware => FlowControl::Hardware,
        }
    }
}

/// Standard baud rates offered in the drop-down, matching the original tool
/// plus a few modern additions.
pub const STANDARD_BAUDS: [u32; 20] = [
    1200, 2400, 4800, 9600, 14400, 19200, 38400, 56000, 57600, 115200, 128000, 230400, 256000,
    460800, 500000, 512000, 600000, 921600, 1000000, 1382400,
];

/// Lowest/highest baud rate accepted for a custom value.
pub const CUSTOM_BAUD_RANGE: (u32, u32) = (50, 12_000_000);

/// Upper bound on remembered custom baud rates.
pub const MAX_CUSTOM_BAUDS: usize = 16;

/// Everything needed to open a port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SerialConfig {
    /// Port device path, e.g. `/dev/ttyUSB0` or `COM3`.
    pub port: String,
    pub baud: u32,
    /// User-added baud rates shown alongside the standard rates.
    pub custom_bauds: Vec<u32>,
    pub data_bits: DataBitsSel,
    pub stop_bits: StopBitsSel,
    pub parity: ParitySel,
    pub flow: FlowSel,
    /// Assert DTR when the port opens.
    pub dtr: bool,
    /// Assert RTS when the port opens.
    pub rts: bool,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baud: 115200,
            custom_bauds: Vec::new(),
            data_bits: DataBitsSel::Eight,
            stop_bits: StopBitsSel::One,
            parity: ParitySel::None,
            flow: FlowSel::None,
            dtr: true,
            rts: true,
        }
    }
}

/// A concrete, driver-level description of how to open a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPlan {
    pub baud: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
    pub flow: FlowControl,
    /// Non-fatal caveat worth surfacing to the user in the log.
    pub note: Option<String>,
}

impl SerialConfig {
    /// Whether a non-standard baud rate can be remembered.
    pub fn can_add_custom_baud(&self, baud: u32) -> bool {
        (CUSTOM_BAUD_RANGE.0..=CUSTOM_BAUD_RANGE.1).contains(&baud)
            && !STANDARD_BAUDS.contains(&baud)
            && !self.custom_bauds.contains(&baud)
            && self
                .custom_bauds
                .iter()
                .filter(|candidate| {
                    (CUSTOM_BAUD_RANGE.0..=CUSTOM_BAUD_RANGE.1).contains(candidate)
                        && !STANDARD_BAUDS.contains(candidate)
                })
                .count()
                < MAX_CUSTOM_BAUDS
    }

    /// Add a valid, non-standard baud rate for future runs.
    pub fn add_custom_baud(&mut self, baud: u32) -> bool {
        self.normalize_custom_bauds();
        if !self.can_add_custom_baud(baud) {
            return false;
        }
        self.custom_bauds.push(baud);
        self.custom_bauds.sort_unstable();
        true
    }

    /// Remove a remembered baud rate.
    pub fn remove_custom_baud(&mut self, baud: u32) -> bool {
        let previous_len = self.custom_bauds.len();
        self.custom_bauds.retain(|candidate| *candidate != baud);
        previous_len != self.custom_bauds.len()
    }

    /// Standard and remembered choices, sorted without duplicates.
    pub fn baud_choices(&self) -> Vec<u32> {
        let mut choices: std::collections::BTreeSet<u32> = STANDARD_BAUDS.into_iter().collect();
        choices.extend(
            self.custom_bauds
                .iter()
                .copied()
                .filter(|baud| (CUSTOM_BAUD_RANGE.0..=CUSTOM_BAUD_RANGE.1).contains(baud)),
        );
        choices.into_iter().collect()
    }

    fn normalize_custom_bauds(&mut self) {
        self.custom_bauds.retain(|baud| {
            (CUSTOM_BAUD_RANGE.0..=CUSTOM_BAUD_RANGE.1).contains(baud)
                && !STANDARD_BAUDS.contains(baud)
        });
        self.custom_bauds.sort_unstable();
        self.custom_bauds.dedup();
        self.custom_bauds.truncate(MAX_CUSTOM_BAUDS);
    }

    /// A one-line summary such as `115200 8-N-1`.
    pub fn summary(&self) -> String {
        let parity = match self.parity {
            ParitySel::None => "N",
            ParitySel::Odd => "O",
            ParitySel::Even => "E",
        };
        let stop = match self.stop_bits {
            StopBitsSel::One => "1",
            StopBitsSel::OnePointFive => "1.5",
            StopBitsSel::Two => "2",
        };
        format!("{}-{}{}{}", self.baud, self.data_bits.bits(), parity, stop)
    }

    /// Translate the user's selection into driver settings.
    ///
    /// 1.5 stop bits deserves a word of explanation.  POSIX termios (Linux and
    /// macOS) has no dedicated flag for it: the *only* way to request 1.5 stop
    /// bits is `CSTOPB` together with 5 data bits, which is what the kernel
    /// interprets as 1.5 rather than 2.  Windows, by contrast, has a real
    /// `ONE5STOPBITS` DCB value, but the `serialport` crate does not expose it.
    /// Rather than silently pretending, we report exactly what happened.
    pub fn to_plan(&self) -> OpenPlan {
        let mut note = None;

        let stop_bits = match self.stop_bits {
            StopBitsSel::One => StopBits::One,
            StopBitsSel::Two => StopBits::Two,
            StopBitsSel::OnePointFive => {
                if cfg!(target_os = "linux") && self.data_bits == DataBitsSel::Five {
                    // CSTOPB + CS5 == 1.5 stop bits on Linux.
                    StopBits::Two
                } else {
                    note = Some(if cfg!(target_os = "linux") {
                        crate::i18n::tr("本系统 (Linux) 仅在数据位为 5 时支持 1.5 位停止位，已按 2 位停止位处理")
                            .to_owned()
                    } else {
                        crate::i18n::tr("当前平台/驱动不支持 1.5 位停止位，已按 2 位停止位处理")
                            .to_owned()
                    });
                    StopBits::Two
                }
            }
        };

        OpenPlan {
            baud: self.baud,
            data_bits: self.data_bits.to_driver(),
            stop_bits,
            parity: self.parity.to_driver(),
            flow: self.flow.to_driver(),
            note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_format() {
        let cfg = SerialConfig::default();
        assert_eq!(cfg.summary(), "115200-8N1");

        let cfg = SerialConfig {
            baud: 9600,
            data_bits: DataBitsSel::Seven,
            parity: ParitySel::Even,
            stop_bits: StopBitsSel::Two,
            ..Default::default()
        };
        assert_eq!(cfg.summary(), "9600-7E2");
    }

    #[test]
    fn default_plan_matches_selection() {
        let cfg = SerialConfig::default();
        let plan = cfg.to_plan();
        assert_eq!(plan.baud, 115200);
        assert_eq!(plan.data_bits, DataBits::Eight);
        assert_eq!(plan.stop_bits, StopBits::One);
        assert_eq!(plan.parity, Parity::None);
        assert_eq!(plan.flow, FlowControl::None);
        assert!(plan.note.is_none());
    }

    #[test]
    fn one_point_five_is_never_silently_wrong() {
        let cfg = SerialConfig {
            stop_bits: StopBitsSel::OnePointFive,
            ..Default::default()
        };
        let plan = cfg.to_plan();
        // Either it is genuinely representable, or the user is told.
        if plan.note.is_none() {
            assert!(cfg!(target_os = "linux") && cfg.data_bits == DataBitsSel::Five);
        }
    }

    #[test]
    fn custom_bauds_can_be_added_removed_and_persisted() {
        let mut cfg = SerialConfig::default();
        assert!(cfg.add_custom_baud(123456));
        assert!(!cfg.add_custom_baud(123456));
        assert!(cfg.add_custom_baud(72000));
        assert!(!cfg.add_custom_baud(9600));
        assert!(!cfg.add_custom_baud(CUSTOM_BAUD_RANGE.0 - 1));
        assert!(!cfg.add_custom_baud(CUSTOM_BAUD_RANGE.1 + 1));

        assert_eq!(cfg.custom_bauds, vec![72000, 123456]);
        let choices = cfg.baud_choices();
        assert!(choices.contains(&72000));
        assert!(choices.contains(&123456));
        assert_eq!(
            choices
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            choices.len()
        );

        let text = serde_json::to_string(&cfg).unwrap();
        let back: SerialConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(back.custom_bauds, cfg.custom_bauds);
        assert_eq!(back.baud_choices(), cfg.baud_choices());

        assert!(cfg.remove_custom_baud(72000));
        assert!(!cfg.remove_custom_baud(72000));
        assert!(!cfg.baud_choices().contains(&72000));
    }
}
