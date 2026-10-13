//! Persisted application settings.
//!
//! Everything the user can tweak is stored in a single JSON document under the
//! platform's configuration directory.  The structure is `#[serde(default)]`
//! throughout, so a settings file written by an older build still loads: any
//! field it lacks simply takes its default.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::encoding::EncodingChoice;
use crate::i18n::Lang;
use crate::logstore::DisplayOptions;
use crate::multi::MultiSend;
use crate::protocol::ProtocolConfig;
use crate::serial::SerialConfig;
use crate::theme::{LogColorOverride, ThemeMode};

/// Options for the single-send panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendSettings {
    /// 16进制发送.
    pub hex: bool,
    /// 发送新行.
    pub append_newline: bool,
    /// Emit CR as part of the newline.
    pub newline_cr: bool,
    /// Emit LF as part of the newline.
    pub newline_lf: bool,
    /// 定时发送.
    pub timed: bool,
    /// 定时发送周期 (ms).
    pub period_ms: u64,
    /// Recently sent payloads, newest first.
    pub history: Vec<String>,
    /// The send buffer itself, restored on startup.
    pub buffer: String,
}

impl Default for SendSettings {
    fn default() -> Self {
        Self {
            hex: false,
            append_newline: true,
            newline_cr: true,
            newline_lf: true,
            timed: false,
            period_ms: 1000,
            history: Vec::new(),
            buffer: String::new(),
        }
    }
}

impl SendSettings {
    /// Cap on remembered payloads.
    pub const MAX_HISTORY: usize = 32;

    /// The newline suffix implied by the flags.
    pub fn newline_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2);
        if self.newline_cr {
            out.push(b'\r');
        }
        if self.newline_lf {
            out.push(b'\n');
        }
        out
    }

    /// Remember a payload, de-duplicating and bounding the list.
    pub fn remember(&mut self, payload: &str) {
        if payload.trim().is_empty() {
            return;
        }
        self.history.retain(|h| h != payload);
        self.history.insert(0, payload.to_owned());
        self.history.truncate(Self::MAX_HISTORY);
    }
}

/// The complete persisted state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeMode,
    /// Interface language; `None` means "not chosen yet, follow the system".
    pub lang: Option<Lang>,
    pub log_colors: LogColorOverride,
    pub serial: SerialConfig,
    pub display: DisplayOptions,
    /// Receive decoding: `None` means auto-detect.
    pub encoding: EncodingChoice,
    /// Transmit encoding: `None` means "follow the receive encoding".
    pub tx_encoding: EncodingChoice,
    pub send: SendSettings,
    pub multi: MultiSend,
    pub protocol: ProtocolConfig,
    /// The protocol panel's data field, restored on startup.
    pub protocol_data: String,
    /// Keep the receive view pinned to the newest line.
    pub auto_scroll: bool,
    /// Mirror transmitted data into the receive view.
    pub echo_sent: bool,
    /// Pause the display without dropping bytes.
    pub paused: bool,
    /// Extra spacing between log rows, in points.
    pub line_spacing: f32,
    pub log_font_size: f32,
    /// Maximum retained display rows.
    pub max_rows: usize,
    /// Maximum retained raw bytes.
    pub max_raw: usize,
    /// Width of the serial-settings panel, in points.  Owned by the app rather
    /// than by egui's panel state: see `ui::panels::splitter`.
    pub left_panel_width: f32,
    /// Width of the multi-send / protocol panel.
    pub right_panel_width: f32,
    /// Height of the send area below the log.
    pub send_area_height: f32,
    /// Window size, restored on startup.
    pub window_size: Option<[f32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            lang: None,
            log_colors: LogColorOverride::default(),
            serial: SerialConfig::default(),
            display: DisplayOptions::default(),
            encoding: None,
            tx_encoding: None,
            send: SendSettings::default(),
            multi: MultiSend::default(),
            protocol: ProtocolConfig::default(),
            protocol_data: String::new(),
            auto_scroll: true,
            echo_sent: true,
            paused: false,
            line_spacing: 0.0,
            log_font_size: 13.0,
            max_rows: 200_000,
            max_raw: 32 * 1024 * 1024,
            left_panel_width: crate::ui::panels::LEFT_PANEL_DEFAULT,
            right_panel_width: crate::ui::panels::RIGHT_PANEL_DEFAULT,
            send_area_height: crate::ui::panels::SEND_AREA_DEFAULT,
            window_size: None,
        }
    }
}

impl Settings {
    /// Clamp anything that could be nonsensical after a hand-edit.
    pub fn normalize(&mut self) {
        self.send.period_ms = self.send.period_ms.clamp(1, 3_600_000);
        self.log_font_size = self.log_font_size.clamp(8.0, 32.0);
        self.line_spacing = self.line_spacing.clamp(0.0, 12.0);
        self.max_rows = self.max_rows.clamp(1_000, 5_000_000);
        self.max_raw = self.max_raw.clamp(64 * 1024, 512 * 1024 * 1024);
        self.multi.normalize();
        self.protocol.normalize();
        self.left_panel_width = self.left_panel_width.clamp(
            crate::ui::panels::LEFT_PANEL_MIN,
            crate::ui::panels::LEFT_PANEL_MAX,
        );
        self.right_panel_width = self.right_panel_width.clamp(
            crate::ui::panels::RIGHT_PANEL_MIN,
            crate::ui::panels::RIGHT_PANEL_MAX,
        );
        self.send_area_height = self.send_area_height.clamp(
            crate::ui::panels::SEND_AREA_MIN,
            crate::ui::panels::SEND_AREA_MAX,
        );
    }

    /// Directory holding the settings file.
    pub fn config_dir() -> Option<PathBuf> {
        // Resolved by hand rather than pulling in a `directories` dependency:
        // three small platform rules are easy to keep correct and auditable.
        #[cfg(target_os = "windows")]
        {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .map(|p| p.join("xcom"))
        }
        #[cfg(target_os = "macos")]
        {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("xcom")
            })
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .map(|p| p.join("xcom"))
        }
    }

    /// Full path of the settings file.
    pub fn path() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("settings.json"))
    }

    /// Load from disk, falling back to defaults on any problem.
    ///
    /// A corrupt or unreadable settings file must never prevent the program
    /// from starting, so every failure is reported and swallowed.
    pub fn load() -> Self {
        match Self::path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// Load from an explicit path.  Split out from [`load`](Self::load) so it
    /// can be tested without touching process-wide environment variables.
    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(mut settings) => {
                settings.normalize();
                settings
            }
            Err(err) => {
                log::warn!("配置文件解析失败 ({err})，已使用默认设置");
                Self::default()
            }
        }
    }

    /// Write to disk, creating the directory if needed.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or_else(|| crate::i18n::tr("无法确定配置目录").to_owned())?;
        self.save_to(&path)
    }

    /// Write to an explicit path.
    pub fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| crate::i18n::tr1("创建配置目录失败: {}", e))?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| crate::i18n::tr1("序列化失败: {}", e))?;
        // Write to a temporary file and rename, so an interrupted save cannot
        // leave a truncated settings file behind.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| crate::i18n::tr1("写入失败: {}", e))?;
        std::fs::rename(&tmp, path).map_err(|e| crate::i18n::tr1("替换失败: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_json() {
        let settings = Settings::default();
        let text = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(settings, back);
    }

    #[test]
    fn partial_json_fills_in_defaults() {
        // Simulates a settings file written by an older build.
        let text = r#"{"theme":"Light","send":{"hex":true,"period_ms":250}}"#;
        let settings: Settings = serde_json::from_str(text).unwrap();
        assert_eq!(settings.theme, ThemeMode::Light);
        assert!(settings.send.hex);
        assert_eq!(settings.send.period_ms, 250);
        // Fields absent from the document keep their defaults.
        assert!(settings.send.append_newline);
        assert_eq!(settings.log_font_size, Settings::default().log_font_size);
        assert_eq!(settings.multi.pages.len(), crate::multi::PAGES);
    }

    #[test]
    fn loop_count_persists_and_defaults_to_infinite() {
        assert_eq!(Settings::default().multi.loop_count, None);

        let mut settings = Settings::default();
        settings.multi.loop_count = Some(7);
        let text = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.multi.loop_count, Some(7));
    }

    #[test]
    fn a_settings_file_missing_newer_fields_still_loads() {
        // Every nested config struct must tolerate fields it does not contain.
        // If one rejects the document, `load_from` falls back to *all* defaults
        // and the user silently loses their entire configuration -- so adding
        // a field in a future version would look like a factory reset.
        let legacy = r#"{
            "theme": "Light",
            "serial": {"port": "/dev/ttyUSB0", "baud": 9600},
            "display": {"mode": "Hex"},
            "multi": {"loop_period_ms": 250},
            "protocol": {"address": 66}
        }"#;
        let settings: Settings =
            serde_json::from_str(legacy).expect("legacy settings must still load");

        assert_eq!(settings.theme, ThemeMode::Light);
        assert_eq!(settings.serial.port, "/dev/ttyUSB0");
        assert_eq!(settings.serial.baud, 9600);
        assert_eq!(settings.multi.loop_period_ms, 250);
        assert_eq!(settings.protocol.address, 66);

        // Fields absent from the document take their defaults.
        assert_eq!(settings.serial.data_bits, crate::serial::DataBitsSel::Eight);
        assert_eq!(settings.multi.loop_count, None);
        assert_eq!(
            settings.protocol.max_retries,
            crate::protocol::ProtocolConfig::default().max_retries
        );
        assert_eq!(settings.display.hex_bytes_per_row, 16);
        assert_eq!(settings.multi.pages.len(), crate::multi::PAGES);
    }

    #[test]
    fn normalize_clamps_hostile_values() {
        let mut settings = Settings {
            log_font_size: 900.0,
            max_rows: 0,
            max_raw: 1,
            ..Default::default()
        };
        settings.send.period_ms = 0;
        settings.normalize();
        assert_eq!(settings.log_font_size, 32.0);
        assert_eq!(settings.send.period_ms, 1);
        assert!(settings.max_rows >= 1_000);
        assert!(settings.max_raw >= 64 * 1024);
    }

    #[test]
    fn newline_bytes_follow_flags() {
        let mut send = SendSettings::default();
        assert_eq!(send.newline_bytes(), b"\r\n");
        send.newline_cr = false;
        assert_eq!(send.newline_bytes(), b"\n");
        send.newline_lf = false;
        assert!(send.newline_bytes().is_empty());
    }

    #[test]
    fn history_is_deduplicated_bounded_and_newest_first() {
        let mut send = SendSettings::default();
        send.remember("first");
        send.remember("second");
        send.remember("first");
        assert_eq!(
            send.history[0], "first",
            "re-sent entries move to the front"
        );
        assert_eq!(send.history.len(), 2);

        for i in 0..(SendSettings::MAX_HISTORY * 2) {
            send.remember(&format!("entry{i}"));
        }
        assert_eq!(send.history.len(), SendSettings::MAX_HISTORY);
    }

    #[test]
    fn history_ignores_blank_payloads() {
        let mut send = SendSettings::default();
        send.remember("   ");
        send.remember("");
        assert!(send.history.is_empty());
    }

    #[test]
    fn config_dir_is_platform_appropriate() {
        let dir = Settings::config_dir();
        // In this environment HOME is set, so a directory must be resolvable.
        if std::env::var_os("HOME").is_some() {
            let dir = dir.expect("config dir");
            assert!(dir.to_string_lossy().contains("xcom"));
        }
    }

    #[test]
    fn save_and_load_round_trip() {
        let temp = std::env::temp_dir().join(format!("xcom-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let path = temp.join("nested").join("settings.json");

        let settings = Settings {
            theme: ThemeMode::Light,
            protocol: ProtocolConfig {
                address: 0x42,
                ..Default::default()
            },
            send: SendSettings {
                buffer: "hello".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        };
        settings.save_to(&path).expect("save should succeed");
        assert!(path.exists(), "save must create parent directories");

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.theme, ThemeMode::Light);
        assert_eq!(loaded.send.buffer, "hello");
        assert_eq!(loaded.protocol.address, 0x42);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn missing_settings_file_falls_back_to_defaults() {
        let path = std::env::temp_dir().join("xcom-settings-does-not-exist-12345.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Settings::load_from(&path), Settings::default());
    }

    #[test]
    fn corrupt_settings_file_falls_back_to_defaults() {
        let temp = std::env::temp_dir().join(format!("xcom-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let path = temp.join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();

        assert_eq!(Settings::load_from(&path), Settings::default());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let temp = std::env::temp_dir().join(format!("xcom-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let path = temp.join("settings.json");
        Settings::default().save_to(&path).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&temp)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left {leftovers:?}");
        let _ = std::fs::remove_dir_all(&temp);
    }
}
