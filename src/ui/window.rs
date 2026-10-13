//! Window background painting.
//!
//! On Wayland the application supplies its own client-side decorations
//! (`sctk-adwaita`), which rounds the title bar but leaves everything below it
//! to us.  To get the fully rounded window GNOME users expect, the app paints
//! its own background with rounded bottom corners into a transparent surface.
//!
//! The transparency is deliberately fail-safe.  `clear_color` is reported with
//! zero alpha, so:
//!
//! * on a compositor that honours alpha, the corner pixels stay transparent and
//!   the window looks rounded;
//! * on one that does not, the corners take the panel colour and the window is
//!   merely square — never black.
//!
//! Only Linux needs this: Windows 11 and macOS round their own windows, and
//! they keep their native decorations.

use eframe::egui::{Color32, CornerRadius, Ui};

/// Radius of the window's bottom corners, in points.
const CORNER_RADIUS: u8 = 12;

/// Whether the application paints its own window background.
///
/// Only true where we own the decorations, so that native chrome on other
/// platforms is left untouched.
pub const PAINTS_OWN_BACKGROUND: bool = cfg!(target_os = "linux");

/// The corner rounding to apply to the window background.
///
/// The top corners are left square: the decoration frame already rounds them,
/// and our content starts below it.
pub const WINDOW_CORNERS: CornerRadius = CornerRadius {
    nw: 0,
    ne: 0,
    sw: CORNER_RADIUS,
    se: CORNER_RADIUS,
};

/// Paint the window background behind every panel.
///
/// Call this first, before any panel is laid out, and make the panel frames
/// transparent so they do not paint square corners over it.
pub fn paint_background(ui: &Ui, color: Color32) {
    if !PAINTS_OWN_BACKGROUND {
        return;
    }
    ui.painter()
        .rect_filled(ui.max_rect(), WINDOW_CORNERS, color);
}

/// The colour reported to the compositor for pixels we never paint.
///
/// Zero alpha is what makes the rounded corners possible; see the module docs
/// for why that degrades safely.
pub fn clear_color(background: Color32) -> [f32; 4] {
    let rgb = [
        background.r() as f32 / 255.0,
        background.g() as f32 / 255.0,
        background.b() as f32 / 255.0,
    ];
    if PAINTS_OWN_BACKGROUND {
        // Alpha zero: the corners we never paint stay see-through.
        [rgb[0], rgb[1], rgb[2], 0.0]
    } else {
        [rgb[0], rgb[1], rgb[2], 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_linux_paints_its_own_background() {
        assert_eq!(PAINTS_OWN_BACKGROUND, cfg!(target_os = "linux"));
    }

    #[test]
    fn top_corners_stay_square_and_bottom_ones_round() {
        // Evaluated in a const block so a regression is caught at compile time
        // rather than waiting for the test suite.  The decoration frame already
        // rounds the top corners, so rounding them again would cut a notch out
        // of the title bar.
        const {
            assert!(WINDOW_CORNERS.nw == 0);
            assert!(WINDOW_CORNERS.ne == 0);
            assert!(WINDOW_CORNERS.sw > 0);
            assert!(WINDOW_CORNERS.sw == WINDOW_CORNERS.se);
        }
    }

    #[test]
    fn clear_colour_is_transparent_where_we_own_the_frame() {
        let colour = clear_color(Color32::from_rgb(0x1E, 0x1E, 0x1E));
        if PAINTS_OWN_BACKGROUND {
            assert_eq!(colour[3], 0.0, "alpha must be zero so corners round");
        } else {
            assert_eq!(colour[3], 1.0, "native chrome must stay opaque");
        }
    }
}
