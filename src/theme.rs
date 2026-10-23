//! Dark and light themes, and the colours used by the receive log.
//!
//! The original tool ships a black-background/green-text terminal look, plus a
//! "白底黑字" (white background, black text) alternative.  Both are preserved
//! here as the *log* palettes of the two themes, while the surrounding
//! application chrome follows a conventional dark/light widget theme.

use eframe::egui::{self, Color32, Stroke, Visuals};
use serde::{Deserialize, Serialize};

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

    pub fn icon(self) -> &'static str {
        match self {
            ThemeMode::Dark => "🌙",
            ThemeMode::Light => "☀",
            ThemeMode::System => "🖥",
        }
    }
}

/// Colours for the receive log area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogPalette {
    pub background: Color32,
    pub foreground: Color32,
    /// Colour of the `[HH:MM:SS.mmm]` prefix.
    pub timestamp: Color32,
    /// Arrow in front of data received from the device.
    pub received_marker: Color32,
    /// Arrow in front of data we transmitted.
    pub sent_marker: Color32,
    /// Selection highlight.
    pub selection: Color32,
    /// Text of the selection.
    pub selection_text: Color32,
}

/// The classic XCOM terminal look.
pub const DARK_LOG: LogPalette = LogPalette {
    background: Color32::from_rgb(0x00, 0x00, 0x00),
    foreground: Color32::from_rgb(0xE0, 0xE0, 0xE0),
    timestamp: Color32::from_rgb(0x80, 0x80, 0x80),
    // Cyan for incoming, gold for outgoing: the arrows are recognisable at a
    // glance without reading the line, even at a small font size.
    received_marker: Color32::from_rgb(0x40, 0xC8, 0xFF),
    sent_marker: Color32::from_rgb(0xFF, 0xD1, 0x66),
    selection: Color32::from_rgb(0x1E, 0x4D, 0x2E),
    selection_text: Color32::from_rgb(0xEA, 0xFF, 0xEA),
};

/// 白底黑字.
pub const LIGHT_LOG: LogPalette = LogPalette {
    background: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    foreground: Color32::from_rgb(0x00, 0x00, 0x00),
    timestamp: Color32::from_rgb(0x80, 0x80, 0x80),
    received_marker: Color32::from_rgb(0x00, 0x72, 0xAE),
    // A deeper shade than the echoed row text, so the arrow reads as a marker
    // rather than as part of the data.
    sent_marker: Color32::from_rgb(0x8B, 0x42, 0x00),
    selection: Color32::from_rgb(0xCC, 0xE4, 0xFF),
    selection_text: Color32::from_rgb(0x00, 0x00, 0x00),
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
        let foreground =
            Color32::from_rgb(self.foreground[0], self.foreground[1], self.foreground[2]);
        let background =
            Color32::from_rgb(self.background[0], self.background[1], self.background[2]);
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
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Background colour of the application chrome (panels and status bar).
///
/// Also used as the window backdrop, which is why it is public: the rounded
/// window background is painted in this colour.
pub fn panel_background(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(0x1E, 0x1E, 0x1E)
    } else {
        Color32::from_rgb(0xF3, 0xF3, 0xF3)
    }
}

/// Whether the given mode currently resolves to a dark appearance.
pub fn is_dark(ctx: &egui::Context, mode: ThemeMode) -> bool {
    match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => ctx.theme() == egui::Theme::Dark,
    }
}

/// The colours of the multi-send "currently sending" marker.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ActiveEntryColors {
    /// The bar down the leading edge of the row in flight.
    pub bar: Color32,
    /// The wash over the whole of that row.
    pub wash: Color32,
}

/// Colours for the entry a multi-send run is on.
///
/// Deliberately not [`egui::Visuals::selection`]'s fill, which is what the
/// marker used to take: that colour is dimmed to 45% for the dark theme
/// because it is a *text selection* wash, and against the dark panel it left
/// the marker barely visible.  These two are picked for contrast against the
/// panel in each theme instead.
pub fn active_entry_colors(ctx: &egui::Context, mode: ThemeMode) -> ActiveEntryColors {
    if is_dark(ctx, mode) {
        ActiveEntryColors {
            bar: Color32::from_rgb(0x4D, 0xA6, 0xFF),
            wash: Color32::from_rgba_unmultiplied(0x4D, 0xA6, 0xFF, 72),
        }
    } else {
        ActiveEntryColors {
            bar: Color32::from_rgb(0x1B, 0x62, 0xB0),
            wash: Color32::from_rgba_unmultiplied(0x1B, 0x62, 0xB0, 56),
        }
    }
}

/// The log palette in effect.
pub fn log_palette(ctx: &egui::Context, mode: ThemeMode, over: LogColorOverride) -> LogPalette {
    let base = if is_dark(ctx, mode) {
        DARK_LOG
    } else {
        LIGHT_LOG
    };
    over.apply(base)
}

/// Install the widget visuals for both themes and select the active one.
///
/// Called only when the mode changes, not every frame.
pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_visuals_of(egui::Theme::Dark, tuned_dark());
    ctx.set_visuals_of(egui::Theme::Light, tuned_light());
    ctx.set_theme(match mode {
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::System => egui::ThemePreference::System,
    });
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

/// A slightly calmer dark theme than egui's default, closer to a native tool.
fn tuned_dark() -> Visuals {
    let mut v = Visuals::dark();
    let panel = Color32::from_rgb(0x1E, 0x1E, 0x1E);
    let surface = Color32::from_rgb(0x25, 0x25, 0x26);
    let field = Color32::from_rgb(0x14, 0x14, 0x14);
    let accent = Color32::from_rgb(0x2E, 0x86, 0xDE);

    // When the application paints its own rounded window background, the
    // panels must not paint square corners over it.
    v.panel_fill = if crate::ui::window::PAINTS_OWN_BACKGROUND {
        Color32::TRANSPARENT
    } else {
        panel
    };
    v.window_fill = surface;
    v.extreme_bg_color = field;
    v.faint_bg_color = Color32::from_rgb(0x2A, 0x2A, 0x2B);
    v.code_bg_color = field;
    v.selection.bg_fill = accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0xC8, 0xDC, 0xF0));
    v.hyperlink_color = Color32::from_rgb(0x54, 0xA6, 0xF0);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(0x3A, 0x3A, 0x3C));
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x33, 0x33, 0x35);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x3E, 0x3E, 0x41);
    v.widgets.active.bg_fill = accent.gamma_multiply(0.7);

    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
    v.widgets.active.corner_radius = egui::CornerRadius::same(4);
    v
}

/// A light theme with neutral greys rather than egui's blue-tinted default.
fn tuned_light() -> Visuals {
    let mut v = Visuals::light();
    let panel = Color32::from_rgb(0xF3, 0xF3, 0xF3);
    let surface = Color32::from_rgb(0xFA, 0xFA, 0xFA);
    let field = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    let accent = Color32::from_rgb(0x2E, 0x86, 0xDE);

    v.panel_fill = if crate::ui::window::PAINTS_OWN_BACKGROUND {
        Color32::TRANSPARENT
    } else {
        panel
    };
    v.window_fill = surface;
    v.extreme_bg_color = field;
    v.faint_bg_color = Color32::from_rgb(0xE9, 0xE9, 0xEA);
    v.code_bg_color = field;
    v.selection.bg_fill = accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(0x20, 0x40, 0x60));
    v.hyperlink_color = Color32::from_rgb(0x1B, 0x62, 0xB0);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(0xD0, 0xD0, 0xD2));
    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.inactive.bg_fill = Color32::from_rgb(0xE0, 0xE0, 0xE2);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xD5, 0xD5, 0xD8);
    v.widgets.active.bg_fill = accent.gamma_multiply(0.6);

    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(4);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(4);
    v.widgets.active.corner_radius = egui::CornerRadius::same(4);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_log_is_light_on_black() {
        assert_eq!(DARK_LOG.background, Color32::BLACK);
        assert!(
            luminance(DARK_LOG.foreground) > 0.5,
            "the default dark-theme text must be light, got {:?}",
            DARK_LOG.foreground
        );
    }

    #[test]
    fn light_log_is_white_on_black() {
        assert_eq!(LIGHT_LOG.background, Color32::WHITE);
        assert_eq!(LIGHT_LOG.foreground, Color32::BLACK);
    }

    #[test]
    fn markers_are_distinct_from_the_data_they_precede() {
        for palette in [DARK_LOG, LIGHT_LOG] {
            // Received and sent arrows must not be confusable with each other
            // or with the text they introduce.
            assert_ne!(palette.received_marker, palette.sent_marker);
            assert_ne!(palette.received_marker, palette.foreground);
            assert_ne!(palette.received_marker, palette.background);
            assert_ne!(palette.sent_marker, palette.background);
        }
    }

    #[test]
    fn disabled_override_passes_the_theme_through() {
        let over = LogColorOverride::default();
        assert_eq!(over.apply(DARK_LOG), DARK_LOG);
        assert_eq!(over.apply(LIGHT_LOG), LIGHT_LOG);
    }

    #[test]
    fn enabled_override_replaces_both_colours() {
        let over = LogColorOverride {
            enabled: true,
            foreground: [0x11, 0x22, 0x33],
            background: [0xAA, 0xBB, 0xCC],
        };
        let palette = over.apply(DARK_LOG);
        assert_eq!(palette.foreground, Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(palette.background, Color32::from_rgb(0xAA, 0xBB, 0xCC));
        // Derived colours must be plausible and not equal to either input.
        assert_ne!(palette.selection, palette.background);
        assert_ne!(palette.timestamp, palette.background);
    }

    #[test]
    fn blend_endpoints_are_exact() {
        let a = Color32::from_rgb(0, 0, 0);
        let b = Color32::from_rgb(255, 128, 64);
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        assert_eq!(blend(a, b, 0.5), Color32::from_rgb(128, 64, 32));
        // Out-of-range t is clamped rather than wrapping.
        assert_eq!(blend(a, b, -1.0), a);
        assert_eq!(blend(a, b, 2.0), b);
    }

    #[test]
    fn theme_mode_labels_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for mode in ThemeMode::ALL {
            assert!(seen.insert(mode.label()));
            assert!(!mode.icon().is_empty());
        }
    }

    /// WCAG relative luminance.
    fn luminance(colour: Color32) -> f32 {
        let channel = |value: u8| {
            let value = value as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(colour.r()) + 0.7152 * channel(colour.g()) + 0.0722 * channel(colour.b())
    }

    /// WCAG contrast ratio, brighter over darker.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn the_sending_marker_stands_out_against_the_panel() {
        // Reported: against the dark panel the marker was barely visible.  It
        // had been borrowing `Visuals::selection.bg_fill`, which this theme
        // dims to 45% for text selection.
        let ctx = egui::Context::default();
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let colours = active_entry_colors(&ctx, mode);
            let panel = panel_background(is_dark(&ctx, mode));
            let ratio = contrast(colours.bar, panel);
            assert!(
                ratio >= 4.5,
                "{mode:?}: the marker bar only has a {ratio:.2}:1 contrast \
                 against {panel:?}, so it will not stand out"
            );
            assert!(
                colours.wash.a() > 0,
                "{mode:?}: the row wash must actually tint the row"
            );
            assert_ne!(
                colours.bar, colours.wash,
                "{mode:?}: the bar and the wash are different jobs"
            );
        }
    }
}
