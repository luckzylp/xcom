//! Title bars for the dialogs the application draws itself.
//!
//! egui draws a window's own title bar, and hard-codes its close button on the
//! right with no switch to mirror it.  The two dialogs that should look like
//! they belong to the desktop — custom baud rates and About — therefore draw
//! their own header: the same row as before, with the close button on
//! whichever side the desktop puts its window buttons.
//!
//! Only the *position* follows the desktop.  There is no minimize or maximize
//! button, because neither dialog can be resized or minimized, and the header
//! is still just a row inside an egui window, which keeps the window movable
//! from anywhere (egui falls back to that when a window has no title bar).

use std::process::Command;
use std::sync::OnceLock;

use eframe::egui::{self, RichText, Ui};

use crate::i18n::tr;

/// Paints a dialog's leading glyph inside `rect`.
///
/// Returns the number of shapes drawn, so a test can prove the icon is real
/// geometry rather than an empty allocation.
pub(crate) type IconPainter<'a> = &'a dyn Fn(&Ui, egui::Rect, egui::Color32) -> usize;

/// Whether the desktop puts window close buttons on the left.
///
/// Probed once per process and cached, the same way the title-bar font probe
/// works: the answer only changes when the user edits their desktop settings,
/// and a dialog must not pay for a process spawn every time it opens.
pub(crate) fn close_on_left() -> bool {
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        read_button_layout().is_some_and(|layout| layout_puts_close_on_left(&layout))
    })
}

/// Reads the desktop's window-button layout, if it has one.
///
/// `gsettings` is the same source the title-bar font probe already uses, and
/// on a GNOME session it is the setting the window manager itself reads.
/// Anything that is not a GNOME session — no `gsettings`, no session bus, no
/// such key — reports `None`, which puts the close button back on the right.
#[cfg(target_os = "linux")]
fn read_button_layout() -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // `gsettings` prints the value quoted and unescaped:
    // 'close,minimize,maximize:appmenu'
    let layout = String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .trim_matches('\'')
        .trim()
        .to_owned();
    (!layout.is_empty()).then_some(layout)
}

#[cfg(not(target_os = "linux"))]
fn read_button_layout() -> Option<String> {
    // Windows and macOS draw their own decorations, and put the close button
    // on the right, so there is nothing to follow.
    None
}

/// Whether a GDK `button-layout` value puts `close` on the left.
///
/// The value is `left:right`, each side a comma-separated list — GNOME's
/// default is `appmenu:minimize,maximize,close`, and a layout with the close
/// button on the left reads `close,minimize,maximize:appmenu`.
///
/// A value with no colon is not a layout at all, and `sctk-adwaita` — which
/// draws the real window frame — treats it as "unset" and falls back to
/// right-hand buttons.  Following that keeps our dialogs lined up with the
/// frame around them, which is the whole point.
fn layout_puts_close_on_left(layout: &str) -> bool {
    match layout.split_once(':') {
        Some((left, _)) => left.split(',').any(|button| button.trim() == "close"),
        None => false,
    }
}

/// Geometry and outcome of a dialog header row.
pub(crate) struct Header {
    /// The whole header row.
    pub rect: egui::Rect,
    /// The title label.
    pub title_rect: egui::Rect,
    /// The close button.
    pub close_rect: egui::Rect,
    pub close_clicked: bool,
}

/// Draw a dialog's header: optional icon, title, and the close button on the
/// desktop's side of the window.
pub(crate) fn header(ui: &mut Ui, title: &str, icon: Option<IconPainter<'_>>) -> Header {
    let close_left = close_on_left();
    let mut close_clicked = false;
    let mut close_rect = egui::Rect::NOTHING;
    let mut title_rect = egui::Rect::NOTHING;

    let row = ui.horizontal(|ui| {
        if close_left {
            let (rect, clicked) = close_button(ui);
            close_rect = rect;
            close_clicked = clicked;
        }
        if let Some(paint) = icon {
            let side = ui.spacing().interact_size.y.min(18.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            paint(ui, rect, ui.visuals().text_color());
        }
        title_rect = ui.label(RichText::new(title).strong()).rect;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !close_left {
                let (rect, clicked) = close_button(ui);
                close_rect = rect;
                close_clicked = clicked;
            }
        });
    });

    Header {
        rect: row.response.rect,
        title_rect,
        close_rect,
        close_clicked,
    }
}

/// The close button, sized like the one egui's own title bar draws.
fn close_button(ui: &mut Ui) -> (egui::Rect, bool) {
    let close = ui
        .add_sized([22.0, 18.0], egui::Button::new("×").frame(false))
        .on_hover_text(tr("关闭"));
    (close.rect, close.clicked())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_hand_layout_puts_close_on_the_left() {
        // The value read from this project's development machine.
        assert!(layout_puts_close_on_left("close,minimize,maximize:appmenu"));
        assert!(layout_puts_close_on_left("close:minimize,maximize"));
    }

    #[test]
    fn a_right_hand_layout_does_not() {
        // GNOME's default, and what Windows and macOS look like.
        assert!(!layout_puts_close_on_left(
            "appmenu:minimize,maximize,close"
        ));
        assert!(!layout_puts_close_on_left("menu:close"));
        assert!(!layout_puts_close_on_left(":close"));
    }

    #[test]
    fn a_value_without_a_colon_is_not_a_layout() {
        // `sctk-adwaita`, which draws the real frame, reads this as "unset"
        // and falls back to right-hand buttons.  Matching it matters: the
        // dialogs must land on the same side as the frame around them.
        assert!(!layout_puts_close_on_left("close,minimize,maximize"));
        assert!(!layout_puts_close_on_left(""));
    }

    #[test]
    fn only_the_exact_button_name_counts() {
        // `closer` is not `close`, and an empty side is not a match either.
        assert!(!layout_puts_close_on_left("closer:appmenu"));
        assert!(!layout_puts_close_on_left("appmenu,something:close"));
        assert!(layout_puts_close_on_left("appmenu, close :minimize"));
    }
}
