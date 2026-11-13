//! Dark and light themes, and the colours used by the receive log.
//!
//! The original tool ships a black-background/green-text terminal look, plus a
//! "白底黑字" (white background, black text) alternative.  Both are preserved
//! here as the *log* palettes of the two themes, while the surrounding
//! application chrome follows gpui-kit's component theme.

use gpui_kit::{App, Hsla, Rgba};
use serde::{Deserialize, Serialize};

use gpui_kit::component::Theme as ComponentTheme;

/// Which theme the application should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Classic terminal look: black log, green text.
    #[default]
    Dark,
    /// 白底黑字: white log, black text.
    Light,
    /// Follow the operating system preference.
    System,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System];

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "黑色主题",
            ThemeMode::Light => "白色主题",
            ThemeMode::System => "跟随系统",
        }
    }
}

/// Build a colour from a packed `0xRRGGBB` value at compile time.
const fn rgb(hex: u32) -> Rgba {
    Rgba {
        r: ((hex >> 16) & 0xFF) as f32 / 255.0,
        g: ((hex >> 8) & 0xFF) as f32 / 255.0,
        b: (hex & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

/// Colours for the receive log area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogPalette {
    pub background: Rgba,
    pub foreground: Rgba,
    /// Colour of the `[HH:MM:SS.mmm]` prefix.
    pub timestamp: Rgba,
    /// Arrow in front of data received from the device.
    pub received_marker: Rgba,
    /// Arrow in front of data we transmitted.
    pub sent_marker: Rgba,
    /// Selection highlight.
    pub selection: Rgba,
    /// Text of the selection.
    pub selection_text: Rgba,
}

/// The classic XCOM terminal look.
pub const DARK_LOG: LogPalette = LogPalette {
    background: rgb(0x000000),
    foreground: rgb(0xE0E0E0),
    timestamp: rgb(0x808080),
    // Cyan for incoming, gold for outgoing: the arrows are recognisable at a
    // glance without reading the line, even at a small font size.
    received_marker: rgb(0x40C8FF),
    sent_marker: rgb(0xFFD166),
    selection: rgb(0x1E4D2E),
    selection_text: rgb(0xEAFFEA),
};

/// 白底黑字.
pub const LIGHT_LOG: LogPalette = LogPalette {
    background: rgb(0xFFFFFF),
    foreground: rgb(0x000000),
    timestamp: rgb(0x808080),
    received_marker: rgb(0x0072AE),
    // A deeper shade than the echoed row text, so the arrow reads as a marker
    // rather than as part of the data.
    sent_marker: rgb(0x8B4200),
    selection: rgb(0xCCE4FF),
    selection_text: rgb(0x000000),
};

/// User override for the log colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogColorOverride {
    /// When false the theme's default log palette is used.
    pub enabled: bool,
    pub foreground: [u8; 3],
    pub background: [u8; 3],
}

impl Default for LogColorOverride {
    fn default() -> Self {
        Self {
            enabled: false,
            foreground: [0xFF, 0xFF, 0xFF],
            background: [0x00, 0x00, 0x00],
        }
    }
}

impl LogColorOverride {
    /// Apply the override on top of a theme palette.
    pub fn apply(self, base: LogPalette) -> LogPalette {
        if !self.enabled {
            return base;
        }
        let foreground = Rgba {
            r: self.foreground[0] as f32 / 255.0,
            g: self.foreground[1] as f32 / 255.0,
            b: self.foreground[2] as f32 / 255.0,
            a: 1.0,
        };
        let background = Rgba {
            r: self.background[0] as f32 / 255.0,
            g: self.background[1] as f32 / 255.0,
            b: self.background[2] as f32 / 255.0,
            a: 1.0,
        };
        // Keep the selection legible against whatever background was chosen.
        let selection = blend(background, foreground, 0.35);
        LogPalette {
            background,
            foreground,
            selection,
            selection_text: foreground,
            // Timestamps stay distinguishable without clashing.
            timestamp: blend(background, foreground, 0.65),
            // A custom foreground/background pair says nothing about which hue
            // suits echoed rows, so both received and echoed text use the same
            // colour -- only the direction marker distinguishes them.
            received_marker: base.received_marker,
            sent_marker: base.sent_marker,
        }
    }
}

/// Linear blend between two colours; `t` of 0 yields `a`, 1 yields `b`.
pub fn blend(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    Rgba {
        r: mix(a.r, b.r),
        g: mix(a.g, b.g),
        b: mix(a.b, b.b),
        a: mix(a.a, b.a),
    }
}

/// Whether the given mode currently resolves to a dark appearance.
pub fn is_dark(cx: &App, mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => ComponentTheme::global(cx).is_dark(),
    }
}

/// The colours of the multi-send "currently sending" marker.
#[derive(Clone, Copy, PartialEq)]
pub struct ActiveEntryColors {
    /// The bar down the leading edge of the row in flight.
    pub bar: Hsla,
    /// The wash over the whole of that row.
    pub wash: Hsla,
}

/// Colours for the entry a multi-send run is on.
pub fn active_entry_colors(cx: &App, mode: ThemeMode) -> ActiveEntryColors {
    if is_dark(cx, mode) {
        ActiveEntryColors {
            bar: Hsla::from(rgb(0x4DA6FF)),
            wash: Hsla::from(rgb(0x4DA6FF)).opacity(0.28),
        }
    } else {
        ActiveEntryColors {
            bar: Hsla::from(rgb(0x1B62B0)),
            wash: Hsla::from(rgb(0x1B62B0)).opacity(0.22),
        }
    }
}

/// The log palette in effect.
pub fn log_palette(cx: &App, mode: ThemeMode, over: LogColorOverride) -> LogPalette {
    let base = if is_dark(cx, mode) {
        DARK_LOG
    } else {
        LIGHT_LOG
    };
    over.apply(base)
}

/// Install the component theme for the selected mode and font settings.
///
/// Called when the mode, language or font selection changes, not every frame.
pub fn apply(cx: &mut App, mode: ThemeMode) {
    match mode {
        ThemeMode::Dark => ComponentTheme::change(gpui_kit::component::ThemeMode::Dark, None, cx),
        ThemeMode::Light => ComponentTheme::change(gpui_kit::component::ThemeMode::Light, None, cx),
        ThemeMode::System => ComponentTheme::sync_system_appearance(None, cx),
    }
}

/// Points of interface text per rem. `font_size` is the application zoom anchor.
pub fn set_base_font(cx: &mut App, family: Option<&str>, size: f32) {
    let family = family
        .map(str::to_owned)
        .or_else(crate::fonts::default_ui_family);
    ComponentTheme::update(cx, |theme| {
        if let Some(family) = family {
            theme.font_family = family.into();
        }
        theme.font_size = gpui_kit::px(size);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: Rgba) -> f32 {
        let channel = |v: f32| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    }

    fn contrast(a: Rgba, b: Rgba) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn blend_endpoints_are_exact() {
        let a = DARK_LOG.background;
        let b = DARK_LOG.foreground;
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
    }

    #[test]
    fn dark_log_is_light_on_black() {
        assert!(luminance(DARK_LOG.foreground) > luminance(DARK_LOG.background));
        assert!(contrast(DARK_LOG.foreground, DARK_LOG.background) >= 7.0);
        assert!(contrast(DARK_LOG.timestamp, DARK_LOG.background) >= 4.5);
    }

    #[test]
    fn light_log_is_dark_on_white() {
        assert!(luminance(LIGHT_LOG.foreground) < luminance(LIGHT_LOG.background));
        assert!(contrast(LIGHT_LOG.foreground, LIGHT_LOG.background) >= 7.0);
    }

    #[test]
    fn markers_are_distinct_from_the_data_they_precede() {
        for palette in [DARK_LOG, LIGHT_LOG] {
            assert_ne!(palette.received_marker, palette.foreground);
            assert_ne!(palette.sent_marker, palette.foreground);
            assert_ne!(palette.received_marker, palette.sent_marker);
        }
    }

    #[test]
    fn the_sending_marker_stands_out_against_the_panel() {
        let colours = active_entry_colors_for_test(true);
        assert!(contrast(colours.bar.into(), rgb(0x1E1E1E)) >= 4.5);
        assert!(colours.wash.a > 0.0);
    }

    fn active_entry_colors_for_test(dark: bool) -> ActiveEntryColors {
        if dark {
            ActiveEntryColors {
                bar: Hsla::from(rgb(0x4DA6FF)),
                wash: Hsla::from(rgb(0x4DA6FF)).opacity(0.28),
            }
        } else {
            ActiveEntryColors {
                bar: Hsla::from(rgb(0x1B62B0)),
                wash: Hsla::from(rgb(0x1B62B0)).opacity(0.22),
            }
        }
    }

    #[test]
    fn disabled_override_passes_the_theme_through() {
        let over = LogColorOverride::default();
        assert_eq!(over.apply(DARK_LOG), DARK_LOG);
    }

    #[test]
    fn enabled_override_replaces_both_colours() {
        let over = LogColorOverride {
            enabled: true,
            foreground: [0x12, 0x34, 0x56],
            background: [0xFE, 0xDC, 0xBA],
        };
        let palette = over.apply(DARK_LOG);
        assert_eq!(palette.foreground, rgb(0x123456));
        assert_eq!(palette.background, rgb(0xFEDCBA));
        assert_eq!(palette.selection_text, palette.foreground);
    }

    #[test]
    fn theme_mode_labels_are_distinct() {
        let labels: Vec<_> = ThemeMode::ALL.iter().map(|m| m.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
    }
}
