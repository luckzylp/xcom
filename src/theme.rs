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
    /// Selection highlight.
    pub selection: Color32,
    /// Text of the selection.
    pub selection_text: Color32,
    /// Rows echoed back from our own transmissions.
    pub sent: Color32,
}

/// The classic XCOM terminal look.
pub const DARK_LOG: LogPalette = LogPalette {
    background: Color32::from_rgb(0x00, 0x00, 0x00),
    foreground: Color32::from_rgb(0x00, 0xFF, 0x00),
    timestamp: Color32::from_rgb(0x6A, 0xA8, 0x6A),
    selection: Color32::from_rgb(0x1E, 0x4D, 0x2E),
    selection_text: Color32::from_rgb(0xEA, 0xFF, 0xEA),
    // Amber reads clearly against the green of received data.
    sent: Color32::from_rgb(0xFF, 0xB7, 0x4D),
};

/// 白底黑字.
pub const LIGHT_LOG: LogPalette = LogPalette {
    background: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    foreground: Color32::from_rgb(0x00, 0x00, 0x00),
    timestamp: Color32::from_rgb(0x80, 0x80, 0x80),
    selection: Color32::from_rgb(0xCC, 0xE4, 0xFF),
    selection_text: Color32::from_rgb(0x00, 0x00, 0x00),
    sent: Color32::from_rgb(0xB2, 0x5E, 0x00),
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
            foreground: [0x00, 0xFF, 0x00],
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
            // suits echoed rows, so the theme's choice is kept.
            sent: base.sent,
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
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_log_is_the_classic_terminal_look() {
        assert_eq!(DARK_LOG.background, Color32::BLACK);
        assert_eq!(DARK_LOG.foreground.g(), 0xFF, "green text");
        assert_eq!(DARK_LOG.foreground.r(), 0x00);
    }

    #[test]
    fn light_log_is_white_on_black() {
        assert_eq!(LIGHT_LOG.background, Color32::WHITE);
        assert_eq!(LIGHT_LOG.foreground, Color32::BLACK);
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
}
