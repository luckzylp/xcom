//! Every visible panel of the application.
//!
//! The layout mirrors the original tool: configuration down the left, the
//! receive log filling the middle, the send area beneath it, and the
//! multi-send / protocol-transfer pages in a panel on the right.  A status bar
//! along the bottom carries the byte counters, the handshake lines and the
//! clock.

use eframe::egui::{self, Context, RichText, Ui};
use std::path::PathBuf;

use crate::app::{Level, SideTab, XcomApp, draw_log};
use crate::encoding::EncodingId;
use crate::i18n::{self, Lang, tr};
use crate::logstore::{HexStyle, ViewMode};
use crate::protocol::{ChecksumKind, TaskKind};
use crate::serial::config::{CUSTOM_BAUD_RANGE, MAX_CUSTOM_BAUDS, STANDARD_BAUDS};
use crate::serial::{DataBitsSel, FlowSel, ParitySel, StopBitsSel};
use crate::theme::{self, ThemeMode};
use crate::ui::dialog;

/// Where a test can read what the real panels actually laid out.
///
/// The drag tests must measure the production code path rather than a
/// stand-in, or they pass whether or not the fix is present.  Outside tests
/// every setter is an empty function that inlines away.
pub(crate) mod probe {
    #[cfg(test)]
    use eframe::egui;
    #[cfg(test)]
    use std::cell::Cell;
    #[cfg(test)]
    use std::cell::RefCell;

    #[cfg(test)]
    thread_local! {
        static LEFT_CONTENT_WIDTH: Cell<f32> = const { Cell::new(0.0) };
        static SEND_PANEL_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static EDITOR_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static SERIAL_CONTENT_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static OPEN_BUTTON_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static REFRESH_BUTTON_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_BUTTON_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_WINDOW_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_INPUT_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_ADD_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_DELETE_RECTS: RefCell<Vec<(u32, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static CUSTOM_BAUD_HEADER_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_TITLE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_CLOSE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static CUSTOM_BAUD_BUTTON_LABEL: RefCell<String> = const { RefCell::new(String::new()) };
        static CUSTOM_BAUD_ICON_SHAPES: Cell<usize> = const { Cell::new(0) };
        static SERIAL_COMBO_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static SEND_TOOLBAR_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static MULTI_HEADER_RECTS: RefCell<Vec<egui::Rect>> = const { RefCell::new(Vec::new()) };
        static MULTI_ACTIVE_ROW_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static MULTI_STATUS_TEXT: RefCell<String> = const { RefCell::new(String::new()) };
        static ABOUT_WINDOW_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_HEADER_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_TITLE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_CLOSE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static TOP_BAR_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
    }

    #[cfg(test)]
    pub(crate) fn set_left_content_width(width: f32) {
        LEFT_CONTENT_WIDTH.with(|c| c.set(width));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_left_content_width(_width: f32) {}

    #[cfg(test)]
    pub(crate) fn left_content_width() -> f32 {
        LEFT_CONTENT_WIDTH.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_send_panel_rect(rect: egui::Rect) {
        SEND_PANEL_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_send_panel_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn send_panel_rect() -> egui::Rect {
        SEND_PANEL_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_editor_rect(rect: egui::Rect) {
        EDITOR_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_editor_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn editor_rect() -> egui::Rect {
        EDITOR_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_serial_content_rect(rect: egui::Rect) {
        SERIAL_CONTENT_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_serial_content_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn serial_content_rect() -> egui::Rect {
        SERIAL_CONTENT_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_open_button_rect(rect: egui::Rect) {
        OPEN_BUTTON_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_open_button_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn open_button_rect() -> egui::Rect {
        OPEN_BUTTON_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_refresh_button_rect(rect: egui::Rect) {
        REFRESH_BUTTON_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_refresh_button_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn refresh_button_rect() -> egui::Rect {
        REFRESH_BUTTON_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_button_rect(rect: egui::Rect) {
        CUSTOM_BAUD_BUTTON_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_button_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_button_rect() -> egui::Rect {
        CUSTOM_BAUD_BUTTON_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn begin_custom_baud_manager() {
        CUSTOM_BAUD_WINDOW_RECT.with(|c| c.set(egui::Rect::NOTHING));
        CUSTOM_BAUD_INPUT_RECT.with(|c| c.set(egui::Rect::NOTHING));
        CUSTOM_BAUD_ADD_RECT.with(|c| c.set(egui::Rect::NOTHING));
        CUSTOM_BAUD_DELETE_RECTS.with(|rects| rects.borrow_mut().clear());
        CUSTOM_BAUD_HEADER_RECT.with(|c| c.set(egui::Rect::NOTHING));
        CUSTOM_BAUD_TITLE_RECT.with(|c| c.set(egui::Rect::NOTHING));
        CUSTOM_BAUD_CLOSE_RECT.with(|c| c.set(egui::Rect::NOTHING));
        CUSTOM_BAUD_BUTTON_LABEL.with(|c| c.borrow_mut().clear());
        CUSTOM_BAUD_ICON_SHAPES.with(|c| c.set(0));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_custom_baud_manager() {}

    #[cfg(test)]
    pub(crate) fn set_custom_baud_window_rect(rect: egui::Rect) {
        CUSTOM_BAUD_WINDOW_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_window_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_window_rect() -> egui::Rect {
        CUSTOM_BAUD_WINDOW_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_input_rect(rect: egui::Rect) {
        CUSTOM_BAUD_INPUT_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_input_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_input_rect() -> egui::Rect {
        CUSTOM_BAUD_INPUT_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_add_rect(rect: egui::Rect) {
        CUSTOM_BAUD_ADD_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_add_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_add_rect() -> egui::Rect {
        CUSTOM_BAUD_ADD_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn push_custom_baud_delete_rect(baud: u32, rect: egui::Rect) {
        CUSTOM_BAUD_DELETE_RECTS.with(|rects| rects.borrow_mut().push((baud, rect)));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_custom_baud_delete_rect<R>(_baud: u32, _rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_delete_rects() -> Vec<(u32, egui::Rect)> {
        CUSTOM_BAUD_DELETE_RECTS.with(|rects| rects.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_header_rect(rect: egui::Rect) {
        CUSTOM_BAUD_HEADER_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_header_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_header_rect() -> egui::Rect {
        CUSTOM_BAUD_HEADER_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_title_rect(rect: egui::Rect) {
        CUSTOM_BAUD_TITLE_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_title_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_title_rect() -> egui::Rect {
        CUSTOM_BAUD_TITLE_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_close_rect(rect: egui::Rect) {
        CUSTOM_BAUD_CLOSE_RECT.with(|c| c.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_close_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_close_rect() -> egui::Rect {
        CUSTOM_BAUD_CLOSE_RECT.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_button_label(label: &str) {
        CUSTOM_BAUD_BUTTON_LABEL.with(|c| *c.borrow_mut() = label.to_owned());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_button_label(_label: &str) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_button_label() -> String {
        CUSTOM_BAUD_BUTTON_LABEL.with(|c| c.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn set_custom_baud_icon_shapes(shapes: usize) {
        CUSTOM_BAUD_ICON_SHAPES.with(|c| c.set(shapes));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_custom_baud_icon_shapes(_shapes: usize) {}

    #[cfg(test)]
    pub(crate) fn custom_baud_icon_shapes() -> usize {
        CUSTOM_BAUD_ICON_SHAPES.with(|c| c.get())
    }

    #[cfg(test)]
    pub(crate) fn begin_serial_combos() {
        SERIAL_COMBO_RECTS.with(|rects| rects.borrow_mut().clear());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_serial_combos() {}

    #[cfg(test)]
    pub(crate) fn push_serial_combo_rect(name: &'static str, rect: egui::Rect) {
        SERIAL_COMBO_RECTS.with(|rects| rects.borrow_mut().push((name, rect)));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_serial_combo_rect<R>(_name: &'static str, _rect: R) {}

    #[cfg(test)]
    pub(crate) fn serial_combo_rects() -> Vec<(&'static str, egui::Rect)> {
        SERIAL_COMBO_RECTS.with(|rects| rects.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn begin_send_toolbar() {
        SEND_TOOLBAR_RECTS.with(|rects| rects.borrow_mut().clear());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_send_toolbar() {}

    #[cfg(test)]
    pub(crate) fn push_send_toolbar_rect(name: &'static str, rect: egui::Rect) {
        SEND_TOOLBAR_RECTS.with(|rects| rects.borrow_mut().push((name, rect)));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_send_toolbar_rect<R>(_name: &'static str, _rect: R) {}

    #[cfg(test)]
    pub(crate) fn send_toolbar_rects() -> Vec<(&'static str, egui::Rect)> {
        SEND_TOOLBAR_RECTS.with(|rects| rects.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn begin_multi_headers() {
        MULTI_HEADER_RECTS.with(|rects| rects.borrow_mut().clear());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_multi_headers() {}

    /// Forget last frame's "being sent" markers.
    ///
    /// Called at the top of the panel rather than where they are drawn: the
    /// status line is drawn above the table, so a reset inside the table would
    /// wipe it immediately.
    #[cfg(test)]
    pub(crate) fn begin_multi_panel() {
        MULTI_ACTIVE_ROW_RECT.with(|rect| rect.set(egui::Rect::NOTHING));
        MULTI_STATUS_TEXT.with(|text| text.borrow_mut().clear());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_multi_panel() {}

    #[cfg(test)]
    pub(crate) fn push_multi_header_rect(rect: egui::Rect) {
        MULTI_HEADER_RECTS.with(|rects| rects.borrow_mut().push(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_multi_header_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn multi_header_rects() -> Vec<egui::Rect> {
        MULTI_HEADER_RECTS.with(|rects| rects.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn set_multi_active_row_rect(rect: egui::Rect) {
        MULTI_ACTIVE_ROW_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_multi_active_row_rect<R>(_rect: R) {}

    /// The row the panel marked as being sent, or `Rect::NOTHING` if none.
    #[cfg(test)]
    pub(crate) fn multi_active_row_rect() -> egui::Rect {
        MULTI_ACTIVE_ROW_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_multi_status_text(text: &str) {
        MULTI_STATUS_TEXT.with(|cell| *cell.borrow_mut() = text.to_owned());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_multi_status_text(_text: &str) {}

    #[cfg(test)]
    pub(crate) fn multi_status_text() -> String {
        MULTI_STATUS_TEXT.with(|cell| cell.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn set_about_window_rect(rect: egui::Rect) {
        ABOUT_WINDOW_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_about_window_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn about_window_rect() -> egui::Rect {
        ABOUT_WINDOW_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_about_header_rect(rect: egui::Rect) {
        ABOUT_HEADER_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_about_header_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn about_header_rect() -> egui::Rect {
        ABOUT_HEADER_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_about_title_rect(rect: egui::Rect) {
        ABOUT_TITLE_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_about_title_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn about_title_rect() -> egui::Rect {
        ABOUT_TITLE_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_about_close_rect(rect: egui::Rect) {
        ABOUT_CLOSE_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_about_close_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn about_close_rect() -> egui::Rect {
        ABOUT_CLOSE_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn begin_top_bar() {
        TOP_BAR_RECTS.with(|rects| rects.borrow_mut().clear());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_top_bar() {}

    #[cfg(test)]
    pub(crate) fn push_top_bar_rect(name: &'static str, rect: egui::Rect) {
        TOP_BAR_RECTS.with(|rects| rects.borrow_mut().push((name, rect)));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_top_bar_rect<R>(_name: &'static str, _rect: R) {}

    #[cfg(test)]
    pub(crate) fn top_bar_rects() -> Vec<(&'static str, egui::Rect)> {
        TOP_BAR_RECTS.with(|rects| rects.borrow().clone())
    }
}

/// Serial-settings panel width, in points.
pub(crate) const LEFT_PANEL_DEFAULT: f32 = 300.0;
pub(crate) const LEFT_PANEL_MIN: f32 = 269.0;
pub(crate) const LEFT_PANEL_MAX: f32 = 460.0;
const SERIAL_COMBO_WIDTH: f32 = 140.0;

/// Tool panel width.  The default is two thirds of the original 430 so the
/// receive area gets more room out of the box.
pub(crate) const RIGHT_PANEL_DEFAULT: f32 = 287.0;
pub(crate) const RIGHT_PANEL_MIN: f32 = 260.0;
pub(crate) const RIGHT_PANEL_MAX: f32 = 760.0;

/// Smallest width the centre column keeps when either side panel is dragged
/// wide.
///
/// Without it the configuration and tool panels can be dragged past the point
/// where the receive view is usable, which is the very thing collapsing them
/// exists to avoid.  It makes [`LEFT_PANEL_MAX`] and [`RIGHT_PANEL_MAX`] *soft*
/// maxima: they apply only when the window can spare the room.
pub(crate) const CENTER_MIN: f32 = 320.0;

/// Slack allowed for the separator lines egui draws between the panels.
const SEPARATOR_SLACK: f32 = 4.0;

/// Height of the send area.
pub(crate) const SEND_AREA_DEFAULT: f32 = 196.0;
pub(crate) const SEND_AREA_MIN: f32 = 96.0;
pub(crate) const SEND_AREA_MAX: f32 = 560.0;

/// Which way a panel extends from its fixed edge.
///
/// The size a pointer implies depends on this: a panel anchored to the right
/// grows as the pointer moves *left*, so the sign differs per side.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitterGrowth {
    /// The settings panel: extends right from the window's left edge.
    Rightward,
    /// The tool panel: extends left from the window's right edge.
    Leftward,
    /// The send area: extends up from the centre column's bottom edge.
    Upward,
}

impl SplitterGrowth {
    /// How far the pointer has moved away from the panel's fixed edge since the
    /// drag began.  Positive means "grow the panel".
    fn growth_delta(self, pointer: egui::Pos2, origin: egui::Pos2) -> f32 {
        match self {
            Self::Rightward => pointer.x - origin.x,
            Self::Leftward => origin.x - pointer.x,
            Self::Upward => origin.y - pointer.y,
        }
    }

    fn cursor(self) -> egui::CursorIcon {
        match self {
            Self::Upward => egui::CursorIcon::ResizeVertical,
            _ => egui::CursorIcon::ResizeHorizontal,
        }
    }

    /// The grab band straddling the splitter line.
    fn band(self, boundary: f32, cross: egui::Rangef, grab: f32) -> egui::Rect {
        match self {
            Self::Rightward | Self::Leftward => {
                egui::Rect::from_x_y_ranges((boundary - grab)..=(boundary + grab), cross)
            }
            Self::Upward => {
                egui::Rect::from_x_y_ranges(cross, (boundary - grab)..=(boundary + grab))
            }
        }
    }
}

/// Which setting a splitter writes back to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitterTarget {
    ConfigWidth,
    ToolWidth,
    SendHeight,
}

/// A splitter to be registered once every panel has been laid out.
///
/// Registering them last matters: the log view allocates a drag-sensing rect
/// that reaches the tool panel's edge, and a widget added later wins the
/// interaction.  Handling the splitters after the centre panel keeps them on
/// top so the line itself is always grabbable.
pub(crate) struct PendingSplitter {
    pub id: &'static str,
    pub growth: SplitterGrowth,
    pub boundary: f32,
    pub cross: egui::Rangef,
    /// The panel's size before this frame, used as the drag origin.  Zero when
    /// the panel is collapsed, so one drag rule covers both directions.
    pub current_size: f32,
    pub min: f32,
    pub max: f32,
    pub target: SplitterTarget,
    /// Whether the panel is currently collapsed to its edge.
    pub collapsed: bool,
}

/// A draggable splitter, drawn and tracked by the app.
///
/// egui's own panel resizing is not used here.  Its `default_size` is
/// order-dependent against `min_size`/`max_size`, so a panel configured the
/// obvious way silently falls back to its minimum, and the resulting width ends
/// up driven by the panel's *content* -- which made the settings panel ignore
/// attempts to shrink it, and made the tool panel snap back in the English UI
/// where the translated labels are wider.  Owning the number makes the
/// behaviour obvious and testable; it is persisted with the rest of the
/// settings.
///
/// Register every pending splitter.  Returns nothing; drags are written
/// straight back into the settings.
pub(crate) fn apply_splitters(app: &mut XcomApp, ui: &mut Ui) {
    for spec in std::mem::take(&mut app.pending_splitters) {
        // A collapsed panel has no line to aim at, so its edge strip has to be
        // easier to hit than the separator between two visible panels.
        const GRAB: f32 = 6.0;
        const COLLAPSED_GRAB: f32 = 16.0;
        let grab = if spec.collapsed { COLLAPSED_GRAB } else { GRAB };
        let rect = spec.growth.band(spec.boundary, spec.cross, grab);
        let response = ui.interact(
            rect,
            egui::Id::new(("splitter", spec.id)),
            egui::Sense::click_and_drag(),
        );
        let response = if spec.collapsed {
            response.on_hover_text(collapsed_hint(spec.target))
        } else {
            response
        };
        if spec.collapsed {
            paint_collapsed_strip(ui, &spec, &response);
        }
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(spec.growth.cursor());
        }

        // The size is tracked as a delta from where the drag started, not as an
        // absolute distance from a fixed edge.  The edge moves as the panel
        // resizes, so an absolute reading is recomputed differently on every
        // frame of the gesture -- the panel overshoots and then creeps back.
        let state_id = egui::Id::new(("splitter-drag", spec.id));
        let current = if spec.collapsed {
            0.0
        } else {
            spec.current_size
        };

        // Record the origin when the button goes down, not when egui decides a
        // drag has begun: the threshold means those are different frames, and
        // using the later one would make the first delta zero.
        if response.is_pointer_button_down_on()
            && ui.data(|data| data.get_temp::<(f32, egui::Pos2)>(state_id).is_none())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            ui.data_mut(|data| data.insert_temp(state_id, (current, pointer)));
        }

        // A click on a collapsed strip is the discoverable way back: the drag
        // that closed the panel is not one the user can retrace by eye.
        if spec.collapsed && response.clicked() {
            set_panel_open(&mut app.settings, spec.target, true);
            app.mark_settings_dirty();
        }

        if response.dragged() {
            let origin = ui.data(|data| data.get_temp::<(f32, egui::Pos2)>(state_id));
            if let (Some((start_size, start_pointer)), Some(pointer)) =
                (origin, response.interact_pointer_pos())
            {
                // The raw size is what decides a collapse: clamping it to the
                // panel's minimum first would make a collapse unreachable,
                // since it can never come out below that minimum.
                let size = start_size + spec.growth.growth_delta(pointer, start_pointer);
                match spec.target {
                    SplitterTarget::SendHeight => {
                        app.settings.send_area_height = size.clamp(spec.min, spec.max)
                    }
                    target => {
                        // Below a third of the panel's own minimum, the drag
                        // stops pretending: it collapses to the window edge
                        // instead of leaving an unusable sliver.  The
                        // threshold is a fixed distance, and the same one on
                        // both sides, so the gesture reads the same either way.
                        let collapsed = size < spec.min / COLLAPSE_FRACTION;
                        set_panel_open(&mut app.settings, target, !collapsed);
                        if !collapsed {
                            // The stored width is the one to restore to, so
                            // collapsing must not overwrite it.
                            set_panel_width(
                                &mut app.settings,
                                target,
                                size.clamp(spec.min, spec.max),
                            );
                        }
                    }
                }
                app.mark_settings_dirty();
            }
        }

        if response.drag_stopped() || !response.is_pointer_button_down_on() {
            ui.data_mut(|data| data.remove::<(f32, egui::Pos2)>(state_id));
        }
    }
}

/// Below this fraction of a panel's minimum width, a drag collapses it.
const COLLAPSE_FRACTION: f32 = 3.0;

/// Width of the strip drawn along the edge of a collapsed panel.
const COLLAPSED_STRIP: f32 = 3.0;

/// Show or collapse the panel a splitter controls.
///
/// Visibility *is* the collapsed state: the checkbox in the top bar and the
/// drag are two ways to set the same thing, so they can never disagree.
fn set_panel_open(settings: &mut crate::settings::Settings, target: SplitterTarget, open: bool) {
    match target {
        SplitterTarget::ConfigWidth => settings.show_config_panel = open,
        SplitterTarget::ToolWidth => settings.show_tool_panel = open,
        SplitterTarget::SendHeight => {}
    }
}

/// Remember the width a panel should be restored to.
fn set_panel_width(settings: &mut crate::settings::Settings, target: SplitterTarget, width: f32) {
    match target {
        SplitterTarget::ConfigWidth => settings.left_panel_width = width,
        SplitterTarget::ToolWidth => settings.right_panel_width = width,
        SplitterTarget::SendHeight => settings.send_area_height = width,
    }
}

/// Hover text for the strip of a collapsed panel.
fn collapsed_hint(target: SplitterTarget) -> &'static str {
    match target {
        SplitterTarget::ConfigWidth => tr("展开配置面板"),
        SplitterTarget::ToolWidth => tr("展开工具面板"),
        SplitterTarget::SendHeight => "",
    }
}

/// Draw the edge strip of a collapsed panel: a hairline that is always there,
/// and a chevron once the pointer reaches it.
fn paint_collapsed_strip(ui: &Ui, spec: &PendingSplitter, response: &egui::Response) {
    let (rect, chevron_center, points_right) = match spec.growth {
        SplitterGrowth::Rightward => (
            egui::Rect::from_x_y_ranges(
                spec.boundary..=(spec.boundary + COLLAPSED_STRIP),
                spec.cross,
            ),
            egui::pos2(spec.boundary + 12.0, spec.cross.center()),
            true,
        ),
        SplitterGrowth::Leftward => (
            egui::Rect::from_x_y_ranges(
                (spec.boundary - COLLAPSED_STRIP)..=spec.boundary,
                spec.cross,
            ),
            egui::pos2(spec.boundary - 12.0, spec.cross.center()),
            false,
        ),
        // The send area cannot collapse, so it has no strip.
        SplitterGrowth::Upward => return,
    };

    let visuals = ui.visuals();
    let active = response.hovered() || response.dragged();
    let strip = if active {
        visuals.selection.bg_fill
    } else {
        visuals.widgets.noninteractive.bg_stroke.color
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::ZERO, strip);

    if active {
        // Painted rather than written: the UI falls back to a CJK face for
        // symbols, and a face without the glyph renders tofu.
        paint_chevron(ui, chevron_center, points_right, visuals.selection.bg_fill);
    }
}

/// Paint a small `›` / `‹` out of two line segments.
fn paint_chevron(ui: &Ui, center: egui::Pos2, points_right: bool, color: egui::Color32) {
    let tip = if points_right { 3.0 } else { -3.0 };
    let half = 4.5;
    let stroke = egui::Stroke::new(1.8, color);
    let elbow = egui::pos2(center.x + tip, center.y);
    let painter = ui.painter();
    painter.line_segment([egui::pos2(center.x - tip, center.y - half), elbow], stroke);
    painter.line_segment([elbow, egui::pos2(center.x - tip, center.y + half)], stroke);
}

/// Colour used for a status message of the given severity.
fn level_color(level: Level) -> egui::Color32 {
    match level {
        Level::Info => egui::Color32::from_rgb(0x9A, 0xA0, 0xA6),
        Level::Success => egui::Color32::from_rgb(0x4C, 0xAF, 0x50),
        Level::Warning => egui::Color32::from_rgb(0xFF, 0xA7, 0x26),
        Level::Error => egui::Color32::from_rgb(0xE5, 0x53, 0x53),
    }
}

/// A small filled circle, used for the connection and handshake indicators.
fn led(ui: &mut Ui, on: bool, on_color: egui::Color32) {
    let size = egui::vec2(10.0, 10.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let color = if on {
        on_color
    } else {
        ui.visuals().weak_text_color()
    };
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// A labelled combo box.  Returns `true` when the selection changed.
fn combo<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: &mut T,
    options: &[T],
    label: impl Fn(T) -> String,
) -> bool {
    combo_response(ui, id, current, options, label).0
}

/// As [`combo`], but also hands back the response rect so a test can measure
/// where the control actually landed.
fn combo_response<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: &mut T,
    options: &[T],
    label: impl Fn(T) -> String,
) -> (bool, egui::Rect) {
    let mut changed = false;
    let response = egui::ComboBox::from_id_salt(id)
        .selected_text(label(*current))
        .width(SERIAL_COMBO_WIDTH)
        .show_ui(ui, |ui| {
            for option in options {
                if ui
                    .selectable_value(current, *option, label(*option))
                    .clicked()
                {
                    changed = true;
                }
            }
        })
        .response;
    (changed, response.rect)
}

/// Let the user choose one installed family, with `None` meaning “automatic”.
fn font_combo(ui: &mut Ui, id: &str, current: &mut Option<String>) -> bool {
    let mut changed = false;
    let selected_text = current.clone().unwrap_or_else(|| tr("自动").to_owned());
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text)
        .width(170.0)
        .height(280.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), tr("自动")).clicked() {
                *current = None;
                changed = true;
            }
            for family in crate::fonts::available_families() {
                let selected = current.as_deref() == Some(family.as_str());
                if ui.selectable_label(selected, family.clone()).clicked() {
                    *current = Some(family);
                    changed = true;
                }
            }
        });
    changed
}

// ---------------------------------------------------------------------------
// Top bar
// ---------------------------------------------------------------------------

pub(crate) fn top_bar(app: &mut XcomApp, ui: &mut Ui) {
    egui::Panel::top("top_bar").show(ui, |ui| {
        probe::begin_top_bar();
        // One row, one centre line.  `horizontal_wrapped` seeds its row extent
        // from `interact_size.y` and re-centres every item against the row as
        // it stands *at that moment*: as soon as a control is taller than that
        // seed the row grows mid-row and each later item drifts by a pixel or
        // two.  Sizing the row up front keeps one shared centre.
        let row_height = ui.ctx().fonts_mut(|fonts| {
            fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
        }) + 10.0;
        ui.spacing_mut().interact_size.y = row_height;
        ui.horizontal_wrapped(|ui| {
            let theme_label = ui.label(tr("主题"));
            probe::push_top_bar_rect("theme_label", theme_label.rect);
            let mut theme_mode = app.settings.theme;
            let (theme_changed, theme_rect) =
                combo_response(ui, "theme_mode", &mut theme_mode, &ThemeMode::ALL, |m| {
                    format!("{} {}", m.icon(), tr(m.label()))
                });
            probe::push_top_bar_rect("theme_combo", theme_rect);
            if theme_changed {
                app.settings.theme = theme_mode;
                app.mark_settings_dirty();
            }

            ui.separator();
            let lang_label = ui.label(tr("语言"));
            probe::push_top_bar_rect("lang_label", lang_label.rect);
            let mut language = i18n::lang();
            let lang_combo = egui::ComboBox::from_id_salt("language")
                .selected_text(language.label())
                .width(110.0)
                .show_ui(ui, |ui| {
                    for candidate in Lang::ALL {
                        if ui
                            .selectable_value(&mut language, candidate, candidate.label())
                            .clicked()
                        {
                            i18n::set_lang(candidate);
                            app.settings.lang = Some(candidate);
                            app.mark_settings_dirty();
                        }
                    }
                });
            probe::push_top_bar_rect("lang_combo", lang_combo.response.rect);

            ui.separator();
            let mut show_config = app.settings.show_config_panel;
            let config_panel = ui.checkbox(&mut show_config, tr("显示配置面板"));
            probe::push_top_bar_rect("config_panel", config_panel.rect);
            if config_panel.changed() {
                app.settings.show_config_panel = show_config;
                app.mark_settings_dirty();
            }

            ui.separator();
            let mut show_tool = app.settings.show_tool_panel;
            let tool_panel = ui.checkbox(&mut show_tool, tr("显示工具面板"));
            probe::push_top_bar_rect("tool_panel", tool_panel.rect);
            if tool_panel.changed() {
                app.settings.show_tool_panel = show_tool;
                app.mark_settings_dirty();
            }

            ui.separator();
            let mut log_colors = app.settings.log_colors;
            let log_colors_checkbox = ui.checkbox(&mut log_colors.enabled, tr("自定义接收区配色"));
            probe::push_top_bar_rect("log_colors", log_colors_checkbox.rect);
            if log_colors_checkbox.changed() {
                app.settings.log_colors = log_colors;
                app.mark_settings_dirty();
            }
            if app.settings.log_colors.enabled {
                ui.label(tr("字色"));
                let mut fg = app.settings.log_colors.foreground;
                if ui.color_edit_button_srgb(&mut fg).changed() {
                    app.settings.log_colors.foreground = fg;
                    app.mark_settings_dirty();
                }
                ui.label(tr("底色"));
                let mut bg = app.settings.log_colors.background;
                if ui.color_edit_button_srgb(&mut bg).changed() {
                    app.settings.log_colors.background = bg;
                    app.mark_settings_dirty();
                }
            }

            ui.separator();
            let reset = ui.button(tr("恢复默认设置"));
            probe::push_top_bar_rect("reset", reset.rect);
            if reset.clicked() {
                app.settings = crate::settings::Settings::default();
                app.log
                    .set_limits(app.settings.max_rows, app.settings.max_raw);
                app.log.set_options(app.settings.display);
                app.mark_settings_dirty();
                app.info(tr("已恢复默认设置"));
            }
            let about = ui.button(tr("关于"));
            probe::push_top_bar_rect("about", about.rect);
            if about.clicked() {
                app.show_about = true;
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Left configuration panel
// ---------------------------------------------------------------------------

pub(crate) fn left_panel(app: &mut XcomApp, ui: &mut Ui) {
    // Measured before the panel is added: this is the whole width the three
    // columns have to share, which is what caps the panel.
    let available = ui.available_rect_before_wrap();
    let max = left_panel_max(app, available.width());

    if !app.settings.show_config_panel {
        probe::set_left_content_width(0.0);
        app.pending_splitters.push(collapsed_splitter(
            "config_panel",
            SplitterGrowth::Rightward,
            SplitterTarget::ConfigWidth,
            available,
            LEFT_PANEL_MIN,
            max,
        ));
        return;
    }

    let width = app.settings.left_panel_width.clamp(LEFT_PANEL_MIN, max);
    let panel = egui::Panel::left("config_panel")
        .resizable(false)
        .exact_size(width)
        .show(ui, |ui| {
            // Fill the panel rather than shrinking to the content, so the
            // scrollbar sits against the panel edge and the whole panel
            // scrolls.  This is presentation only -- the splitter below does
            // not depend on it (measured, not assumed).
            let scroll = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    serial_section(app, ui);
                    ui.add_space(6.0);
                    display_section(app, ui);
                    ui.add_space(6.0);
                    encoding_section(app, ui);
                    ui.add_space(6.0);
                    statistics_section(app, ui);
                });
            probe::set_left_content_width(scroll.inner_rect.width());
        });

    let outer = panel.response.rect;
    app.pending_splitters.push(PendingSplitter {
        id: "config_panel",
        growth: SplitterGrowth::Rightward,
        boundary: outer.right(),
        cross: outer.y_range(),
        current_size: width,
        min: LEFT_PANEL_MIN,
        max,
        target: SplitterTarget::ConfigWidth,
        collapsed: false,
    });
}

/// The widest the configuration panel may be dragged to.
///
/// The centre column keeps [`CENTER_MIN`], so this maximum shrinks as the tool
/// panel grows and as the window narrows.  The floor at the panel's own minimum
/// keeps the clamp well-formed on a window too narrow to honour either — there
/// the centre simply gives way, which is unavoidable rather than a choice.
fn left_panel_max(app: &XcomApp, available: f32) -> f32 {
    let tool = if app.settings.show_tool_panel {
        app.settings.right_panel_width
    } else {
        0.0
    };
    (available - CENTER_MIN - tool - SEPARATOR_SLACK).max(LEFT_PANEL_MIN)
}

/// The splitter of a panel that is collapsed: a strip along the window edge,
/// with no panel behind it.
///
/// The splitter is registered even then, because a collapsed panel has to stay
/// draggable — otherwise the only way back would be the top bar.
fn collapsed_splitter(
    id: &'static str,
    growth: SplitterGrowth,
    target: SplitterTarget,
    area: egui::Rect,
    min: f32,
    max: f32,
) -> PendingSplitter {
    let boundary = match growth {
        SplitterGrowth::Rightward => area.left(),
        _ => area.right(),
    };
    PendingSplitter {
        id,
        growth,
        boundary,
        cross: area.y_range(),
        current_size: 0.0,
        min,
        max,
        target,
        collapsed: true,
    }
}

/// Paint the "custom baud rate" icon: a tune / sliders glyph.
///
/// Drawn as geometry rather than a font symbol on purpose.  The UI falls back
/// to whatever CJK face the system offers, and such a face cannot be relied on
/// to carry a symbol like U+2699 (gear) or U+270E (pencil); a missing glyph
/// renders as tofu.  Painting the shape keeps it identical on every platform.
///
/// Returns the number of painted shapes so a test can prove the icon is real.
fn paint_tune_icon(ui: &Ui, rect: egui::Rect, color: egui::Color32) -> usize {
    let painter = ui.painter();
    let side = rect.size().min_elem();
    let half = side * 0.34;
    let center = rect.center();
    let stroke = egui::Stroke::new(1.4, color);
    let knob_radius = (side * 0.09).clamp(1.2, 2.6);
    // Three rails with a knob on each: the conventional "adjust / customise"
    // glyph.  Knobs alternate sides so the rails read as controls, not bullets.
    let rows = [-0.70_f32, 0.0, 0.70];
    let knobs = [0.45_f32, -0.55, 0.45];
    let mut shapes = 0;
    for (row, knob) in rows.iter().zip(knobs) {
        let y = center.y + row * half;
        painter.line_segment(
            [
                egui::pos2(center.x - half, y),
                egui::pos2(center.x + half, y),
            ],
            stroke,
        );
        shapes += 1;
        painter.circle_filled(egui::pos2(center.x + knob * half, y), knob_radius, color);
        shapes += 1;
    }
    shapes
}

fn serial_section(app: &mut XcomApp, ui: &mut Ui) {
    let open = app.link.is_open();
    let manager_id = egui::Id::new("custom_baud_manager");
    let input_id = egui::Id::new("custom_baud_input");
    let mut manager_open = ui
        .ctx()
        .data(|data| data.get_temp::<bool>(manager_id).unwrap_or(false));
    probe::begin_custom_baud_manager();
    let serial_bounds = ui.max_rect();
    probe::set_serial_content_rect(serial_bounds);
    probe::begin_serial_combos();
    egui::CollapsingHeader::new(tr("串口设置"))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("serial_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("串口"));
                    ui.horizontal(|row_ui| {
                        let ports = app.ports.ports().to_vec();
                        let current = app.settings.serial.port.clone();
                        let selected_text = ports
                            .iter()
                            .find(|p| p.name == current)
                            .map(|p| p.display())
                            .unwrap_or_else(|| {
                                if current.is_empty() {
                                    tr("（无可用串口）").to_owned()
                                } else {
                                    current.clone()
                                }
                            });
                        let refresh_width = 24.0;
                        let port_combo_width = (row_ui.available_width()
                            - refresh_width
                            - row_ui.spacing().item_spacing.x)
                            .clamp(72.0, SERIAL_COMBO_WIDTH);
                        let port_combo_rect = row_ui
                            .add_enabled_ui(!open, |ui| {
                                let port_combo = egui::ComboBox::from_id_salt("port_select")
                                    .selected_text(selected_text)
                                    .width(port_combo_width)
                                    .truncate()
                                    .show_ui(ui, |ui| {
                                        if ports.is_empty() {
                                            ui.weak(tr("未检测到串口"));
                                        }
                                        for port in &ports {
                                            let item = ui.selectable_label(
                                                port.name == app.settings.serial.port,
                                                port.display(),
                                            );
                                            let item = item.on_hover_text(
                                                port.detail().unwrap_or_else(|| port.display()),
                                            );
                                            if item.clicked() {
                                                app.settings.serial.port = port.name.clone();
                                                app.mark_settings_dirty();
                                            }
                                        }
                                    });
                                probe::push_serial_combo_rect("port", port_combo.response.rect);
                                port_combo.response.rect
                            })
                            .inner;
                        let refresh = row_ui
                            .add_sized(
                                [refresh_width, port_combo_rect.height()],
                                egui::Button::new("↻"),
                            )
                            .on_hover_text(tr("刷新"));
                        probe::set_refresh_button_rect(refresh.rect);
                        if refresh.clicked() {
                            app.ports.poll();
                            app.info(tr("已刷新串口列表"));
                        }
                    });
                    ui.end_row();

                    // ---- baud rate ------------------------------------
                    ui.label(tr("波特率"));
                    ui.horizontal(|row_ui| {
                        let choices = app.settings.serial.baud_choices();
                        let standard = STANDARD_BAUDS.contains(&app.settings.serial.baud);
                        let mut baud = app.settings.serial.baud;
                        let label = if standard {
                            baud.to_string()
                        } else {
                            format!("{baud}{}", tr("（自定义）"))
                        };
                        let manager_width = 24.0;
                        let baud_width = (row_ui.available_width()
                            - manager_width
                            - row_ui.spacing().item_spacing.x)
                            .clamp(72.0, SERIAL_COMBO_WIDTH);
                        let baud_combo_rect = row_ui
                            .add_enabled_ui(!open, |ui| {
                                let combo = egui::ComboBox::from_id_salt("baud_select")
                                    .selected_text(label)
                                    .width(baud_width)
                                    .truncate()
                                    .show_ui(ui, |ui| {
                                        for rate in choices {
                                            if ui
                                                .selectable_value(&mut baud, rate, rate.to_string())
                                                .clicked()
                                            {
                                                app.settings.serial.baud = rate;
                                                app.mark_settings_dirty();
                                            }
                                        }
                                    });
                                probe::push_serial_combo_rect("baud", combo.response.rect);
                                combo.response.rect
                            })
                            .inner;
                        let manage = row_ui.add_sized(
                            [manager_width, baud_combo_rect.height()],
                            egui::Button::new(""),
                        );
                        let icon_shapes =
                            paint_tune_icon(row_ui, manage.rect, row_ui.visuals().text_color());
                        probe::set_custom_baud_icon_shapes(icon_shapes);
                        probe::set_custom_baud_button_label("");
                        let manage = manage.on_hover_text(tr("管理自定义波特率"));
                        probe::set_custom_baud_button_rect(manage.rect);
                        if manage.clicked() {
                            manager_open = true;
                        }
                    });
                    ui.end_row();

                    ui.label(tr("数据位"));
                    {
                        let mut value = app.settings.serial.data_bits;
                        let options = DataBitsSel::ALL;
                        ui.add_enabled_ui(!open, |ui| {
                            if combo(ui, "data_bits", &mut value, &options, |v| {
                                v.label().to_owned()
                            }) {
                                app.settings.serial.data_bits = value;
                                app.mark_settings_dirty();
                            }
                            probe::push_serial_combo_rect("data_bits", ui.min_rect());
                        });
                    }
                    ui.end_row();

                    ui.label(tr("停止位"));
                    {
                        let mut value = app.settings.serial.stop_bits;
                        let options = StopBitsSel::ALL;
                        ui.add_enabled_ui(!open, |ui| {
                            if combo(ui, "stop_bits", &mut value, &options, |v| {
                                v.label().to_owned()
                            }) {
                                app.settings.serial.stop_bits = value;
                                app.mark_settings_dirty();
                            }
                            probe::push_serial_combo_rect("stop_bits", ui.min_rect());
                        });
                    }
                    ui.end_row();

                    ui.label(tr("校验位"));
                    {
                        let mut value = app.settings.serial.parity;
                        let options = ParitySel::ALL;
                        ui.add_enabled_ui(!open, |ui| {
                            if combo(ui, "parity", &mut value, &options, |v| {
                                tr(v.label()).to_owned()
                            }) {
                                app.settings.serial.parity = value;
                                app.mark_settings_dirty();
                            }
                            probe::push_serial_combo_rect("parity", ui.min_rect());
                        });
                    }
                    ui.end_row();

                    ui.label(tr("流控"));
                    {
                        let mut value = app.settings.serial.flow;
                        let options = FlowSel::ALL;
                        ui.add_enabled_ui(!open, |ui| {
                            if combo(ui, "flow", &mut value, &options, |v| {
                                tr(v.label()).to_owned()
                            }) {
                                app.settings.serial.flow = value;
                                app.mark_settings_dirty();
                            }
                            probe::push_serial_combo_rect("flow", ui.min_rect());
                        });
                    }
                    ui.end_row();

                    ui.label(tr("DTR / RTS"));
                    ui.horizontal(|ui| {
                        let mut dtr = app.settings.serial.dtr;
                        if ui.checkbox(&mut dtr, "DTR").changed() {
                            app.settings.serial.dtr = dtr;
                            if open && let Err(err) = app.link.set_dtr(dtr) {
                                app.error(crate::i18n::tr1("设置 DTR 失败: {}", err));
                            }
                            app.mark_settings_dirty();
                        }
                        let mut rts = app.settings.serial.rts;
                        if ui.checkbox(&mut rts, "RTS").changed() {
                            app.settings.serial.rts = rts;
                            if open && let Err(err) = app.link.set_rts(rts) {
                                app.error(crate::i18n::tr1("设置 RTS 失败: {}", err));
                            }
                            app.mark_settings_dirty();
                        }
                    });
                    ui.end_row();
                });

            ui.add_space(4.0);
            let (label, color) = if open {
                (tr("关闭串口"), egui::Color32::from_rgb(0xE5, 0x53, 0x53))
            } else {
                (tr("打开串口"), egui::Color32::from_rgb(0x4C, 0xAF, 0x50))
            };
            // Do not let the button stretch to the panel edge: a full-width
            // button reads as a separator rather than a control.  Cap it and
            // leave a clear gap.
            let button_width = (serial_bounds.width() - 24.0).clamp(96.0, 168.0);
            let button_height = 28.0_f32.max(
                ui.ctx().fonts_mut(|fonts| {
                    fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
                }) + 10.0,
            );
            let button_center = egui::pos2(
                serial_bounds.center().x - 16.0,
                ui.cursor().min.y + button_height / 2.0,
            );
            let open_button = ui.put(
                egui::Rect::from_center_size(
                    button_center,
                    egui::vec2(button_width, button_height),
                ),
                egui::Button::new(RichText::new(label).strong().color(color)),
            );
            probe::set_open_button_rect(open_button.rect);
            if open_button.clicked() {
                app.toggle_port();
            }
            ui.small(crate::i18n::tr1("当前: {}", app.settings.serial.summary()));
            ui.small(tr("提示: 串口打开时无法修改通信参数，需先关闭串口"));
        });

    let ctx = ui.ctx().clone();
    let mut pending_baud = ctx.data(|data| data.get_temp::<u32>(input_id).unwrap_or(115_201));
    let mut remove_baud = None;
    let mut close_requested = false;
    // `title_bar(false)` drops egui's own bar, whose `TextStyle::Heading`
    // caption (~1.4x the body) is what made this small dialog look
    // top-heavy.  Without a title bar egui falls back to "drag anywhere", so
    // the compact header we draw below keeps the dialog movable.
    let popup = egui::Window::new(tr("自定义波特率"))
        .id(manager_id)
        .title_bar(false)
        .default_width(288.0)
        .collapsible(false)
        .resizable(false)
        .open(&mut manager_open)
        .show(&ctx, |ui| {
            ui.set_min_width(268.0);

            // ── compact header ──────────────────────────────────────────
            let paint_icon = |ui: &Ui, rect, color| paint_tune_icon(ui, rect, color);
            let header = dialog::header(ui, tr("自定义波特率"), Some(&paint_icon));
            probe::set_custom_baud_header_rect(header.rect);
            probe::set_custom_baud_title_rect(header.title_rect);
            probe::set_custom_baud_close_rect(header.close_rect);
            if header.close_clicked {
                close_requested = true;
            }
            ui.separator();

            // ── add a rate ──────────────────────────────────────────────
            let can_add = app.settings.serial.can_add_custom_baud(pending_baud);
            ui.add_space(2.0);
            ui.label(tr("添加波特率"));
            ui.horizontal(|ui| {
                let input = ui.add_sized(
                    [(ui.available_width() - 64.0).max(96.0), 24.0],
                    egui::DragValue::new(&mut pending_baud)
                        .speed(100.0)
                        .range(CUSTOM_BAUD_RANGE.0..=CUSTOM_BAUD_RANGE.1),
                );
                probe::set_custom_baud_input_rect(input.rect);
                let add = ui.add_enabled(
                    can_add,
                    egui::Button::new(tr("添加")).min_size(egui::vec2(56.0, 24.0)),
                );
                probe::set_custom_baud_add_rect(add.rect);
                if add.clicked() && app.settings.serial.add_custom_baud(pending_baud) {
                    app.mark_settings_dirty();
                }
            });
            if !can_add {
                let hint = if app.settings.serial.custom_bauds.len() >= MAX_CUSTOM_BAUDS {
                    tr("自定义波特率列表已满")
                } else {
                    tr("该波特率已存在或为标准值")
                };
                ui.weak(hint);
            }

            // ── saved rates ─────────────────────────────────────────────
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);
            ui.label(tr("已保存的自定义波特率"));
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    let rates = app.settings.serial.custom_bauds.clone();
                    if rates.is_empty() {
                        ui.weak(tr("暂无自定义波特率"));
                    }
                    for rate in rates {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(rate.to_string()).monospace());
                            ui.weak("bps");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let delete = ui
                                        .add_sized([24.0, 22.0], egui::Button::new("−"))
                                        .on_hover_text(tr("删除"));
                                    probe::push_custom_baud_delete_rect(rate, delete.rect);
                                    if delete.clicked() {
                                        remove_baud = Some(rate);
                                    }
                                },
                            );
                        });
                    }
                });
        });
    if let Some(popup) = popup {
        probe::set_custom_baud_window_rect(popup.response.rect);
    }
    if close_requested {
        manager_open = false;
    }
    if let Some(rate) = remove_baud
        && app.settings.serial.remove_custom_baud(rate)
    {
        app.mark_settings_dirty();
    }
    ctx.data_mut(|data| {
        data.insert_temp(manager_id, manager_open);
        data.insert_temp(input_id, pending_baud);
    });
}

fn display_section(app: &mut XcomApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tr("显示设置"))
        .default_open(true)
        .show(ui, |ui| {
            let mut display = app.settings.display;
            let mut changed = false;

            let mut hex = display.mode == ViewMode::Hex;
            if ui.checkbox(&mut hex, tr("16进制显示")).changed() {
                display.mode = if hex { ViewMode::Hex } else { ViewMode::Text };
                changed = true;
            }

            if display.mode == ViewMode::Hex {
                ui.horizontal(|ui| {
                    ui.label(tr("HEX 样式"));
                    let mut style = display.hex_style;
                    if combo(
                        ui,
                        "hex_style",
                        &mut style,
                        &[HexStyle::Stream, HexStyle::Dump],
                        |s| match s {
                            HexStyle::Stream => tr("连续").to_owned(),
                            HexStyle::Dump => tr("16 字节/行").to_owned(),
                        },
                    ) {
                        display.hex_style = style;
                        changed = true;
                    }
                    let mut upper = display.upper_hex;
                    if ui.checkbox(&mut upper, tr("大写")).changed() {
                        display.upper_hex = upper;
                        changed = true;
                    }
                });
                if display.hex_style == HexStyle::Dump {
                    ui.horizontal(|ui| {
                        ui.label(tr("每行字节"));
                        let mut per_row = display.hex_bytes_per_row;
                        if ui
                            .add(egui::DragValue::new(&mut per_row).range(1..=64))
                            .changed()
                        {
                            display.hex_bytes_per_row = per_row;
                            changed = true;
                        }
                    });
                }
            }

            let mut markers = display.markers;
            if ui
                .checkbox(&mut markers, tr("方向标记"))
                .on_hover_text(tr("接收的数据以 -> 开头，发送的数据以 <- 开头"))
                .changed()
            {
                display.markers = markers;
                changed = true;
            }

            let mut timestamp = display.timestamp;
            if ui.checkbox(&mut timestamp, tr("时间戳")).changed() {
                display.timestamp = timestamp;
                changed = true;
            }
            if display.timestamp {
                ui.small(tr("以回车换行断帧；数据中若不含回车换行则不会显示时间戳"));
            }

            let mut wrap = display.wrap;
            if ui.checkbox(&mut wrap, tr("自动换行")).changed() {
                display.wrap = wrap;
                changed = true;
            }

            let mut auto_scroll = app.settings.auto_scroll;
            if ui
                .checkbox(&mut auto_scroll, tr("自动滚动到最新"))
                .changed()
            {
                app.settings.auto_scroll = auto_scroll;
                app.mark_settings_dirty();
            }

            ui.horizontal(|ui| {
                ui.label(tr("行距"));
                let mut spacing = app.settings.line_spacing;
                if ui
                    .add(egui::Slider::new(&mut spacing, 0.0..=10.0).fixed_decimals(0))
                    .changed()
                {
                    app.settings.line_spacing = spacing;
                    app.mark_settings_dirty();
                }
            });

            ui.separator();
            ui.strong(tr("字体设置"));
            let mut fonts = app.settings.fonts.clone();
            let mut font_changed = false;
            egui::Grid::new("font_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("界面字体"));
                    let mut family = fonts.ui_family.clone();
                    font_changed |= font_combo(ui, "ui_font_family", &mut family);
                    fonts.ui_family = family;
                    ui.end_row();

                    ui.label(tr("界面字号"));
                    let mut size = fonts.ui_size;
                    font_changed |= ui
                        .add(egui::Slider::new(&mut size, 9.0..=24.0).fixed_decimals(0))
                        .changed();
                    fonts.ui_size = size;
                    ui.end_row();

                    ui.label(tr("接收区字体"));
                    let mut family = fonts.receive_family.clone();
                    font_changed |= font_combo(ui, "receive_font_family", &mut family);
                    fonts.receive_family = family;
                    ui.end_row();

                    ui.label(tr("接收区字号"));
                    let mut size = fonts.receive_size;
                    font_changed |= ui
                        .add(egui::Slider::new(&mut size, 8.0..=32.0).fixed_decimals(0))
                        .changed();
                    fonts.receive_size = size;
                    ui.end_row();

                    ui.label(tr("发送区字体"));
                    let mut family = fonts.send_family.clone();
                    font_changed |= font_combo(ui, "send_font_family", &mut family);
                    fonts.send_family = family;
                    ui.end_row();

                    ui.label(tr("发送区字号"));
                    let mut size = fonts.send_size;
                    font_changed |= ui
                        .add(egui::Slider::new(&mut size, 8.0..=32.0).fixed_decimals(0))
                        .changed();
                    fonts.send_size = size;
                    ui.end_row();
                });
            if font_changed {
                fonts.normalize();
                app.settings.fonts = fonts;
                app.mark_settings_dirty();
            }

            if changed {
                app.settings.display = display;
                app.log.set_options(display);
                app.mark_settings_dirty();
            }

            // The clear/save actions live in the receive toolbar, right next
            // to the data they act on; repeating them here was redundant.
        });
}

fn encoding_section(app: &mut XcomApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tr("编码设置"))
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("接收编码"));
                let choice = app.settings.encoding;
                let label = match choice {
                    None => crate::i18n::tr1("自动识别（{}）", app.encoding_label()),
                    Some(id) => tr(id.label()).to_owned(),
                };
                let mut newly_selected = None;
                egui::ComboBox::from_id_salt("rx_encoding")
                    .selected_text(label)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(choice.is_none(), tr("自动识别"))
                            .clicked()
                        {
                            newly_selected = Some(None);
                        }
                        for id in EncodingId::ALL {
                            if ui
                                .selectable_label(choice == Some(id), tr(id.label()))
                                .clicked()
                            {
                                newly_selected = Some(Some(id));
                            }
                        }
                    });
                if let Some(selection) = newly_selected {
                    app.set_encoding(selection);
                }
            });

            ui.horizontal(|ui| {
                ui.label(tr("发送编码"));
                let choice = app.settings.tx_encoding;
                let label = match choice {
                    None => tr("跟随接收编码").to_owned(),
                    Some(id) => tr(id.label()).to_owned(),
                };
                let mut newly_selected = None;
                egui::ComboBox::from_id_salt("tx_encoding")
                    .selected_text(label)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(choice.is_none(), tr("跟随接收编码"))
                            .clicked()
                        {
                            newly_selected = Some(None);
                        }
                        for id in EncodingId::ALL {
                            if ui
                                .selectable_label(choice == Some(id), tr(id.label()))
                                .clicked()
                            {
                                newly_selected = Some(Some(id));
                            }
                        }
                    });
                if let Some(selection) = newly_selected {
                    app.settings.tx_encoding = selection;
                    app.mark_settings_dirty();
                }
            });

            ui.small(crate::i18n::tr1("当前接收: {}", app.encoding_label()));
            ui.small(crate::i18n::tr1("已分析 {} 字节", app.auto.total()));
            ui.small(tr("自动识别会持续观察数据流，锁定后不再频繁切换"));
        });
}

fn statistics_section(app: &mut XcomApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tr("统计"))
        .default_open(false)
        .show(ui, |ui| {
            egui::Grid::new("stats_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(tr("发送字节"));
                    ui.label(app.tx_bytes.to_string());
                    ui.end_row();
                    ui.label(tr("接收字节"));
                    ui.label(app.rx_bytes.to_string());
                    ui.end_row();
                    ui.label(tr("显示行数"));
                    ui.label(app.log.row_count().to_string());
                    ui.end_row();
                    ui.label(tr("缓存字节"));
                    ui.label(app.log.retained_bytes().to_string());
                    ui.end_row();
                    ui.label(tr("未显示字节"));
                    ui.label(app.pending_rx_len().to_string());
                    ui.end_row();
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("最大保留行数"));
                let mut rows = app.settings.max_rows;
                if ui
                    .add(
                        egui::DragValue::new(&mut rows)
                            .speed(1000.0)
                            .range(1_000..=5_000_000),
                    )
                    .changed()
                {
                    app.settings.max_rows = rows;
                    app.log.set_limits(rows, app.settings.max_raw);
                    app.mark_settings_dirty();
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("最大缓存 (MB)"));
                let mut mb = app.settings.max_raw / (1024 * 1024);
                if ui
                    .add(egui::DragValue::new(&mut mb).speed(1.0).range(1..=512))
                    .changed()
                {
                    app.settings.max_raw = mb * 1024 * 1024;
                    app.log
                        .set_limits(app.settings.max_rows, app.settings.max_raw);
                    app.mark_settings_dirty();
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Centre: log view and send area
// ---------------------------------------------------------------------------

pub(crate) fn center_panel(app: &mut XcomApp, ui: &mut Ui) {
    egui::CentralPanel::default().show(ui, |ui| {
        // The send area sits at the bottom of the central column, so that the
        // configuration and tool panels flank the full height of the window.
        let height = app.settings.send_area_height;
        let panel = egui::Panel::bottom("send_area")
            .resizable(false)
            .exact_size(height)
            .show(ui, |ui| send_area(app, ui));
        let outer = panel.response.rect;
        app.pending_splitters.push(PendingSplitter {
            id: "send_area",
            growth: SplitterGrowth::Upward,
            boundary: outer.top(),
            cross: outer.x_range(),
            current_size: app.settings.send_area_height,
            min: SEND_AREA_MIN,
            max: SEND_AREA_MAX,
            target: SplitterTarget::SendHeight,
            // The send area shrinks to its minimum rather than collapsing:
            // there is nothing to fold it into.
            collapsed: false,
        });

        log_toolbar(app, ui);
        ui.separator();

        let palette = theme::log_palette(ui.ctx(), app.settings.theme, app.settings.log_colors);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, palette.background);
        draw_log(app, ui, palette);
    });
}

fn log_toolbar(app: &mut XcomApp, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        let label = if app.show_paused {
            tr("继续显示")
        } else {
            tr("暂停显示")
        };
        if ui.button(label).clicked() {
            if app.show_paused {
                app.resume_display();
            } else {
                app.show_paused = true;
                app.info(tr("显示已暂停，数据仍在后台接收"));
            }
        }

        let copy_enabled = app.log_view.has_selection();
        if ui
            .add_enabled(copy_enabled, egui::Button::new(tr("复制选中")))
            .clicked()
        {
            copy_selection(app, ui);
        }
        if ui.button(tr("全选")).clicked() {
            let store = &app.log;
            app.log_view.select_all(store);
        }
        if ui.button(tr("清除接收")).clicked() {
            app.clear_receive();
        }
        if ui.button(tr("保存接收数据")).clicked() {
            save_log(app);
        }
        if ui.button(tr("保存原始数据")).clicked() {
            save_raw_log(app);
        }

        ui.separator();
        let mut echo = app.settings.echo_sent;
        if ui
            .checkbox(&mut echo, tr("回显发送"))
            .on_hover_text(tr("把发出的内容显示在接收区，便于确认是否真的发出"))
            .changed()
        {
            app.settings.echo_sent = echo;
            app.mark_settings_dirty();
        }
        ui.separator();
        ui.label(RichText::new(app.encoding_label()).monospace());
        if app.log_view.has_selection() {
            ui.label(
                RichText::new(tr("已选中，Ctrl+C 复制"))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
    });
}

/// Copy the current selection to the system clipboard.
fn copy_selection(app: &mut XcomApp, ui: &Ui) {
    let text = {
        let XcomApp { log, log_view, .. } = app;
        log_view.selected_text(log)
    };
    match text {
        Some(text) => {
            ui.ctx().copy_text(text);
            app.ok(tr("已复制选中内容到剪贴板"));
        }
        None => app.warn(tr("没有可复制的内容")),
    }
}

const SEND_RIGHT_PAD: f32 = 6.0;

/// Which edge of the toolbar the action group is anchored to.
enum ActionsAnchor {
    /// The roomy case: one row with the options, the group hugging the right.
    Right,
    /// The cramped case: a row of its own, the group starting at the left.
    Left,
}

/// The single send button, rightmost of the group.
fn send_button(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    let button = ui.add_sized(
        [96.0, btn_height],
        egui::Button::new(RichText::new(tr("发送 (Ctrl+Enter)")).strong()),
    );
    probe::push_send_toolbar_rect("send", button.rect);
    if button.clicked() {
        app.send_once();
    }
}

/// 发送文件, or 终止发送 while a file transfer is running.
fn file_button(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    let button = if app.file_send.is_some() {
        ui.add_sized([76.0, btn_height], egui::Button::new(tr("终止发送")))
    } else {
        ui.add_sized([76.0, btn_height], egui::Button::new(tr("发送文件")))
    };
    probe::push_send_toolbar_rect("file", button.rect);
    if button.clicked() {
        if app.file_send.is_some() {
            app.abort_file_send();
        } else if let Some(path) = pick_open_file(tr("选择要发送的文件")) {
            app.start_file_send(path);
        }
    }
}

fn clear_button(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    let button = ui.add_sized([76.0, btn_height], egui::Button::new(tr("清除发送")));
    probe::push_send_toolbar_rect("clear", button.rect);
    if button.clicked() {
        app.settings.send.buffer.clear();
        app.tx_bytes = 0;
        app.rx_bytes = 0;
        app.mark_settings_dirty();
        app.info(tr("已清除发送区与收发计数"));
    }
}

/// The history dropdown, leftmost of the group.
fn history_button(app: &mut XcomApp, ui: &mut Ui) {
    let history = egui::ComboBox::from_id_salt("send_history")
        .selected_text(tr("历史"))
        .width(68.0)
        .show_ui(ui, |ui| {
            if app.settings.send.history.is_empty() {
                ui.weak(tr("暂无历史"));
            }
            let history = app.settings.send.history.clone();
            for entry in history {
                let preview: String = entry.chars().take(40).collect();
                if ui.selectable_label(false, preview).clicked() {
                    app.settings.send.buffer = entry;
                    app.mark_settings_dirty();
                }
            }
        });
    probe::push_send_toolbar_rect("history", history.response.rect);
}

/// Lay out 历史 · 清除发送 · 发送文件 · 发送, in that reading order.
///
/// The group is added in reverse for a right-to-left ui, which places the first
/// widget at its *right* edge; a left-anchored row needs the natural order.
fn send_actions(app: &mut XcomApp, ui: &mut Ui, btn_height: f32, anchor: ActionsAnchor) {
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
    ui.spacing_mut().interact_size.y = btn_height;

    match anchor {
        ActionsAnchor::Right => {
            send_button(app, ui, btn_height);
            file_button(app, ui, btn_height);
            clear_button(app, ui, btn_height);
            history_button(app, ui);
        }
        ActionsAnchor::Left => {
            history_button(app, ui);
            clear_button(app, ui, btn_height);
            file_button(app, ui, btn_height);
            send_button(app, ui, btn_height);
        }
    }
}

fn right_aligned_send_actions(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    let avail_w = (ui.available_width() - SEND_RIGHT_PAD).max(0.0);
    ui.allocate_ui_with_layout(
        egui::vec2(avail_w, btn_height),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| send_actions(app, ui, btn_height, ActionsAnchor::Right),
    );
}

/// The cramped case: the group takes a row of its own and starts at the left
/// edge, so it lines up with the 发送区 options above it.
fn left_aligned_send_actions(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), btn_height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| send_actions(app, ui, btn_height, ActionsAnchor::Left),
    );
}

/// Lay out only the send editor and report the height it took, so a test can
/// observe that dragging the separator really resizes it.
fn send_area(app: &mut XcomApp, ui: &mut Ui) {
    probe::set_send_panel_rect(ui.max_rect());
    probe::begin_send_toolbar();

    // Toolbar row height: tall enough that text/checkboxes are not clipped.
    let btn_height = 28.0_f32.max(
        ui.ctx().fonts_mut(|fonts| {
            fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
        }) + 10.0,
    );
    // At the minimum window size the center panel is narrower than the left
    // options plus the fixed-width action group. Keep them on one row when
    // there is room; otherwise move the actions to a second, right-aligned row
    // instead of letting the right-to-left layout overlap the options.
    let one_row_min = 620.0 * (app.settings.fonts.ui_size / 13.0).max(1.0);
    let actions_on_second_row = ui.available_width() < one_row_min;

    ui.horizontal(|ui| {
        ui.set_height(btn_height);
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);

        // ── Left: label + configuration controls ────────────────────────────
        let section_label = ui.label(RichText::new(tr("发送区")).strong());
        probe::push_send_toolbar_rect("section", section_label.rect);
        ui.separator();

        let mut hex = app.settings.send.hex;
        let hex_control = ui.checkbox(&mut hex, tr("16进制发送"));
        probe::push_send_toolbar_rect("hex", hex_control.rect);
        if hex_control.changed() {
            app.settings.send.hex = hex;
            app.mark_settings_dirty();
        }
        let mut newline = app.settings.send.append_newline;
        let newline_control = ui.checkbox(&mut newline, tr("发送新行"));
        probe::push_send_toolbar_rect("newline", newline_control.rect);
        if newline_control.changed() {
            app.settings.send.append_newline = newline;
            app.mark_settings_dirty();
        }
        if app.settings.send.append_newline {
            let mut cr = app.settings.send.newline_cr;
            let mut lf = app.settings.send.newline_lf;
            let a = ui.checkbox(&mut cr, "CR").changed();
            let b = ui.checkbox(&mut lf, "LF").changed();
            if a || b {
                app.settings.send.newline_cr = cr;
                app.settings.send.newline_lf = lf;
                app.mark_settings_dirty();
            }
        }

        let mut timed = app.settings.send.timed;
        let can_timed = app.link.is_open();
        if ui
            .add_enabled(can_timed, egui::Checkbox::new(&mut timed, tr("定时发送")))
            .changed()
        {
            app.settings.send.timed = timed;
            app.mark_settings_dirty();
        }
        if app.settings.send.timed {
            let mut period = app.settings.send.period_ms;
            if ui
                .add(
                    egui::DragValue::new(&mut period)
                        .speed(10.0)
                        .range(1..=3_600_000)
                        .suffix(" ms"),
                )
                .changed()
            {
                app.settings.send.period_ms = period;
                app.mark_settings_dirty();
            }
        }

        if !actions_on_second_row {
            right_aligned_send_actions(app, ui, btn_height);
        }
    });

    if actions_on_second_row {
        ui.horizontal(|ui| {
            ui.set_height(btn_height);
            left_aligned_send_actions(app, ui, btn_height);
        });
    }

    if let Some(send) = app.file_send.as_ref() {
        let progress = send.progress();
        let remaining = send.remaining();
        let name = send.path.clone();
        ui.add(
            egui::ProgressBar::new(progress)
                .text(crate::i18n::tr2("{} · 剩余 {} 字节", name, remaining))
                .desired_height(16.0),
        );
    }

    // Fill whatever height the user has dragged the send area to.  A fixed
    // `desired_rows` left the editor four rows tall no matter how the panel
    // was resized, which reads as "dragging does nothing".
    let available = ui.available_size();
    let editor_size = egui::vec2(available.x, available.y.max(48.0));
    let editor = ui.add(
        egui::TextEdit::multiline(&mut app.settings.send.buffer)
            .font(crate::fonts::send_font_id(app.settings.fonts.send_size))
            .desired_width(editor_size.x)
            .min_size(editor_size)
            .hint_text(if app.settings.send.hex {
                tr("输入十六进制，如 01 02 0A（可省略空格）")
            } else {
                tr("输入要发送的内容")
            }),
    );
    probe::set_editor_rect(editor.rect);
}

// ---------------------------------------------------------------------------
// Right: multi-send and protocol pages
// ---------------------------------------------------------------------------

pub(crate) fn tool_panel(app: &mut XcomApp, ui: &mut Ui) {
    // Whatever the configuration panel left behind: the tool panel is laid out
    // second, so this is the space that is actually still free.
    let available = ui.available_rect_before_wrap();
    let max = (available.width() - CENTER_MIN - SEPARATOR_SLACK).max(RIGHT_PANEL_MIN);

    if !app.settings.show_tool_panel {
        app.pending_splitters.push(collapsed_splitter(
            "tool_panel",
            SplitterGrowth::Leftward,
            SplitterTarget::ToolWidth,
            available,
            RIGHT_PANEL_MIN,
            max,
        ));
        return;
    }

    let width = app.settings.right_panel_width.clamp(RIGHT_PANEL_MIN, max);
    let panel = egui::Panel::right("tool_panel")
        .resizable(false)
        .exact_size(width)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(app.side_tab == SideTab::Multi, tr("多条发送"))
                    .clicked()
                {
                    app.side_tab = SideTab::Multi;
                }
                if ui
                    .selectable_label(app.side_tab == SideTab::Protocol, tr("协议传输"))
                    .clicked()
                {
                    app.side_tab = SideTab::Protocol;
                }
            });
            ui.separator();
            match app.side_tab {
                SideTab::Multi => multi_panel(app, ui),
                SideTab::Protocol => protocol_panel(app, ui),
            }
        });

    let outer = panel.response.rect;
    app.pending_splitters.push(PendingSplitter {
        id: "tool_panel",
        growth: SplitterGrowth::Leftward,
        boundary: outer.left(),
        cross: outer.y_range(),
        current_size: width,
        min: RIGHT_PANEL_MIN,
        max,
        target: SplitterTarget::ToolWidth,
        collapsed: false,
    });
}

/// Split the space available to the multi-send table between its two text
/// columns.
///
/// `fixed` is the total width of the checkbox, index and button columns, which
/// do not scale; everything left over is shared 62/38 between 内容 and 备注.
fn multi_column_widths(available: f32, spacing: f32) -> (f32, f32) {
    let fixed = 30.0 + 24.0 + 30.0 + 32.0 + 58.0 + 52.0;
    // No per-column floor: a floor larger than the leftover space would push
    // the table past the panel edge, which is the very problem this solves.
    // The two columns always sum to the leftover, so the table fits exactly.
    let flexible = (available - spacing * 6.0 - fixed).max(0.0);
    (flexible * 0.62, flexible * 0.38)
}

/// Colour of the "currently sending" marker in the multi-send table.
///
/// The theme's selection colour, which is the one colour egui defines for both
/// the dark and the light theme that already means "this is the one you are on".
fn active_entry_color(ui: &Ui) -> egui::Color32 {
    ui.visuals().selection.bg_fill
}

/// A translucent wash of [`active_entry_color`], for a whole row's background.
fn active_entry_wash(ui: &Ui) -> egui::Color32 {
    let colour = active_entry_color(ui);
    egui::Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), 60)
}

fn multi_panel(app: &mut XcomApp, ui: &mut Ui) {
    probe::begin_multi_panel();

    // ---- paging -----------------------------------------------------------
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("首页")).clicked() {
            app.settings.multi.first_page();
        }
        if ui.button(tr("上一页")).clicked() {
            app.settings.multi.previous_page();
        }
        ui.label(crate::i18n::tr2(
            "第 {} / {} 页",
            app.settings.multi.page + 1,
            crate::multi::PAGES,
        ));
        if ui.button(tr("下一页")).clicked() {
            app.settings.multi.next_page();
        }
        if ui.button(tr("尾页")).clicked() {
            app.settings.multi.last_page();
        }
    });

    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("导入条目")).clicked()
            && let Some(path) = pick_open_file(tr("导入发送条目"))
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let multi = &mut app.settings.multi;
                    match multi.import_json(&text) {
                        Ok(count) => {
                            app.mark_settings_dirty();
                            app.ok(crate::i18n::tr1("已导入 {} 个条目", count));
                        }
                        Err(err) => app.error(err),
                    }
                }
                Err(err) => app.error(crate::i18n::tr1("读取文件失败: {}", err)),
            }
        }
        if ui.button(tr("导出条目")).clicked()
            && let Some(path) = pick_save_file(tr("导出发送条目"), "xcom-entries.json", "json")
        {
            let multi = &app.settings.multi;
            match multi.export_json() {
                Ok(text) => match std::fs::write(&path, text) {
                    Ok(()) => app.ok(crate::i18n::tr1("已导出到 {}", path.display())),
                    Err(err) => app.error(crate::i18n::tr1("写入失败: {}", err)),
                },
                Err(err) => app.error(err),
            }
        }
        let can_send = app.link.is_open() && !app.multi_run_active();
        if ui
            .add_enabled(can_send, egui::Button::new(tr("发送已勾选")))
            .on_hover_text(tr("按顺序发送本页所有已勾选且非空的条目"))
            .clicked()
        {
            app.start_multi_run(app.settings.multi.page, false, std::time::Instant::now());
        }

        let mut loop_enabled = app.settings.multi.loop_enabled;
        let can_loop = app.link.is_open();
        if ui
            .add_enabled(
                can_loop,
                egui::Checkbox::new(&mut loop_enabled, tr("循环发送")),
            )
            .changed()
        {
            app.settings.multi.loop_enabled = loop_enabled;
            let page = app.settings.multi.page;
            if loop_enabled {
                app.start_multi_run(page, true, std::time::Instant::now());
            } else {
                app.stop_multi_run(Level::Info, tr("已停止循环发送"));
            }
            app.mark_settings_dirty();
        }
        if app.settings.multi.loop_enabled {
            let mut period = app.settings.multi.loop_period_ms;
            if ui
                .add(
                    egui::DragValue::new(&mut period)
                        .speed(10.0)
                        .range(1..=3_600_000)
                        .suffix(" ms"),
                )
                .changed()
            {
                app.settings.multi.loop_period_ms = period;
                app.mark_settings_dirty();
            }
        }

        // Which entry the run is on, next to the controls that started it.
        // Deliberately not part of the table: it stays visible while the table
        // is scrolled, and while the user has paged away from the run.
        if let Some(sent) = app.multi_run_sent() {
            ui.separator();
            let text = crate::i18n::tr3(
                "正在发送 #{} ({} / {})",
                sent.index,
                sent.ordinal,
                sent.round_len,
            );
            probe::set_multi_status_text(&text);
            ui.label(RichText::new(text).strong().color(active_entry_color(ui)));
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label(tr("轮次间隔"));
        let mut forever = app.settings.multi.loop_count.is_none();
        if ui.checkbox(&mut forever, tr("无限循环")).changed() {
            app.settings.multi.loop_count = if forever { None } else { Some(10) };
            app.multi_cycles = 0;
            app.mark_settings_dirty();
        }
        match app.settings.multi.loop_count {
            None => {
                ui.label(
                    RichText::new(tr("不限次数"))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }
            Some(limit) => {
                let mut value = limit;
                if ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .speed(1.0)
                            .range(1..=1_000_000),
                    )
                    .changed()
                {
                    app.settings.multi.loop_count = Some(value);
                    app.multi_cycles = 0;
                    app.mark_settings_dirty();
                }
                ui.label(
                    RichText::new(crate::i18n::tr2(
                        "已完成 {} / {} 次",
                        app.multi_cycles,
                        limit,
                    ))
                    .small()
                    .color(ui.visuals().weak_text_color()),
                );
            }
        }
    });

    ui.horizontal_wrapped(|ui| {
        let mut keypad = app.settings.multi.keypad_enabled;
        if ui
            .checkbox(&mut keypad, tr("关联数字键盘 (1-9,0)"))
            .changed()
        {
            app.settings.multi.keypad_enabled = keypad;
            app.mark_settings_dirty();
        }
        let mut cr = app.settings.multi.newline_cr;
        let mut lf = app.settings.multi.newline_lf;
        let a = ui.checkbox(&mut cr, tr("新行 CR")).changed();
        let b = ui.checkbox(&mut lf, tr("新行 LF")).changed();
        if a || b {
            app.settings.multi.newline_cr = cr;
            app.settings.multi.newline_lf = lf;
            app.mark_settings_dirty();
        }
    });

    ui.separator();

    // ---- entries ----------------------------------------------------------
    let page = app.settings.multi.page;
    // Which row to mark.  A run is tied to the page it started on, so a
    // highlight on any other page's row would point at a different entry
    // entirely.
    let active_index = match app.multi_run_page() {
        Some(running) if running == page => app.multi_run_sent().map(|sent| sent.index),
        _ => None,
    };
    let mut send_request: Option<usize> = None;
    let mut dirty = false;

    // egui's `Grid` reuses the previous frame's column widths as a *floor*
    // (`child_size.max(prev_width)`), so a table built with it can grow but
    // never shrink -- dragging the splitter back left the columns too wide and
    // pushed the send button off the edge.  Laying the rows out by hand with
    // explicit widths is what makes them track the panel in both directions.
    //
    // These five widths must sum to the `fixed` constant in
    // `multi_column_widths`, or the row will not fill the panel exactly.
    let enable_w = 30.0;
    let index_w = 24.0;
    let hex_w = 30.0;
    let newline_w = 32.0;
    // Delay applied after this entry, before the next one.
    let delay_w = 58.0;
    let send_w = 52.0;

    const GAP: f32 = 5.0;
    let row_height = 20.0_f32.max(
        ui.ctx().fonts_mut(|fonts| {
            fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
        }) + 4.0,
    );

    let (text_width, note_width) = multi_column_widths(ui.available_width(), GAP);

    // Fill the panel with rows rather than fixing each page at ten entries.
    let rows_that_fit = ((ui.available_height() - row_height * 1.5) / (row_height + 4.0)).floor();
    let visible = rows_that_fit.max(crate::multi::ENTRIES_PER_PAGE as f32) as usize;
    if app.settings.multi.ensure_len(page, visible) {
        app.mark_settings_dirty();
        dirty = true;
    }
    let total_rows = app
        .settings
        .multi
        .pages
        .get(page)
        .map_or(0, Vec::len)
        .max(visible);

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(GAP, 4.0);
            probe::begin_multi_headers();

            ui.horizontal(|ui| {
                for (width, header) in [
                    (enable_w, tr("启用")),
                    (index_w, "#"),
                    (text_width, tr("内容")),
                    (note_width, tr("备注")),
                    (hex_w, "HEX"),
                    (newline_w, tr("新行")),
                    (delay_w, tr("延迟")),
                    (send_w, ""),
                ] {
                    let header = ui.add_sized(
                        [width, row_height],
                        egui::Label::new(RichText::new(header).strong()),
                    );
                    probe::push_multi_header_rect(header.rect);
                }
            });

            for index in 0..total_rows {
                let Some(entry) = app.settings.multi.entry_mut(page, index) else {
                    continue;
                };
                let active = active_index == Some(index);
                let stripe = index % 2 == 1;
                let mut row_rect = egui::Rect::NOTHING;
                ui.horizontal(|ui| {
                    let row = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), row_height),
                    );
                    row_rect = row;
                    if stripe {
                        ui.painter().rect_filled(
                            row,
                            egui::CornerRadius::ZERO,
                            ui.visuals().faint_bg_color,
                        );
                    }
                    if active {
                        // Painted over the zebra stripe, so the marker reads the
                        // same on both: a wash for the row, and a solid bar on
                        // its leading edge.
                        ui.painter().rect_filled(
                            row,
                            egui::CornerRadius::ZERO,
                            active_entry_wash(ui),
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                row.min,
                                egui::pos2(row.min.x + 3.0, row.max.y),
                            ),
                            egui::CornerRadius::ZERO,
                            active_entry_color(ui),
                        );
                    }
                    let mut row_changed = false;
                    row_changed |= ui
                        .add_sized(
                            [enable_w, row_height],
                            egui::Checkbox::new(&mut entry.enabled, ""),
                        )
                        .changed();
                    ui.add_sized([index_w, row_height], egui::Label::new(format!("{index}")));
                    row_changed |= ui
                        .add_sized(
                            [text_width, row_height],
                            egui::TextEdit::singleline(&mut entry.text).hint_text(if entry.hex {
                                "01 02 0A"
                            } else {
                                tr("指令")
                            }),
                        )
                        .changed();
                    row_changed |= ui
                        .add_sized(
                            [note_width, row_height],
                            egui::TextEdit::singleline(&mut entry.note).hint_text(tr("备注")),
                        )
                        .changed();
                    row_changed |= ui
                        .add_sized([hex_w, row_height], egui::Checkbox::new(&mut entry.hex, ""))
                        .changed();
                    row_changed |= ui
                        .add_sized(
                            [newline_w, row_height],
                            egui::Checkbox::new(&mut entry.newline, ""),
                        )
                        .changed();
                    let mut delay = entry.delay_ms;
                    if ui
                        .add_sized(
                            [delay_w, row_height],
                            egui::DragValue::new(&mut delay)
                                .speed(5.0)
                                .range(0..=3_600_000)
                                .suffix(" ms"),
                        )
                        .changed()
                    {
                        entry.delay_ms = delay;
                        dirty = true;
                    }
                    if ui
                        .add_sized([send_w, row_height], egui::Button::new(tr("发送")))
                        .clicked()
                    {
                        send_request = Some(index);
                    }
                    if row_changed {
                        dirty = true;
                    }
                });
                if active {
                    probe::set_multi_active_row_rect(row_rect);
                }
            }
        });

    if dirty {
        app.mark_settings_dirty();
    }
    if let Some(index) = send_request {
        app.send_multi_entry(page, index, false);
    }

    // ---- numeric keypad shortcuts ----------------------------------------
    if app.settings.multi.keypad_enabled && !ui.ctx().egui_wants_keyboard_input() {
        let pressed = ui.input(|i| {
            use egui::Key::*;
            [
                (Num1, 0),
                (Num2, 1),
                (Num3, 2),
                (Num4, 3),
                (Num5, 4),
                (Num6, 5),
                (Num7, 6),
                (Num8, 7),
                (Num9, 8),
                (Num0, 9),
            ]
            .into_iter()
            .find(|(key, _)| i.key_pressed(*key))
            .map(|(_, index)| index)
        });
        if let Some(index) = pressed {
            app.send_multi_entry(page, index, false);
        }
    }
}

fn protocol_panel(app: &mut XcomApp, ui: &mut Ui) {
    let running = app.protocol.is_running();
    let mut config = app.protocol.config().clone();

    ui.horizontal_wrapped(|ui| {
        ui.label(tr("地址"));
        ui.add(
            egui::DragValue::new(&mut config.address)
                .range(0..=255)
                .speed(1.0),
        );
        ui.label(tr("功能"));
        ui.add(
            egui::DragValue::new(&mut config.function)
                .range(0..=255)
                .speed(1.0),
        );
        ui.label(tr("校验"));
        let mut checksum = config.checksum;
        if combo(
            ui,
            "protocol_checksum",
            &mut checksum,
            &ChecksumKind::ALL,
            |c| tr(c.label()).to_owned(),
        ) {
            config.checksum = checksum;
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label(tr("帧周期"));
        ui.add(
            egui::DragValue::new(&mut config.frame_period_ms)
                .speed(10.0)
                .range(1..=600_000)
                .suffix(" ms"),
        );
        ui.label(tr("最大重发"));
        ui.add(
            egui::DragValue::new(&mut config.max_retries)
                .speed(1.0)
                .range(0..=255),
        );
        ui.label(tr("最大数据长度"));
        ui.add(
            egui::DragValue::new(&mut config.max_data_len)
                .speed(1.0)
                .range(1..=255),
        );
    });

    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut config.auto_send, tr("自动发送"));
        ui.checkbox(&mut config.show_raw, tr("显示原始帧"));
        ui.checkbox(&mut config.auto_wrap, tr("自动换行"));
    });

    if let Some(warning) = config.period_warning(app.settings.serial.baud) {
        ui.colored_label(level_color(Level::Warning), warning);
    }

    let previous = app.protocol.config().clone();
    if config != previous {
        app.settings.protocol = config.clone();
        app.protocol.set_config(config);
        app.mark_settings_dirty();
    }

    ui.separator();
    ui.label(RichText::new(tr("发送数据")).strong());
    ui.add(
        egui::TextEdit::multiline(&mut app.settings.protocol_data)
            .font(egui::TextStyle::Monospace)
            .desired_rows(3)
            .desired_width(f32::INFINITY)
            .hint_text(tr("十六进制，如 00 00 00 0A")),
    );

    ui.horizontal_wrapped(|ui| {
        let enabled = app.link.is_open();
        if ui
            .add_enabled(enabled && !running, egui::Button::new(tr("单次发送")))
            .clicked()
        {
            protocol_start(app, TaskKind::Single);
        }
        if ui
            .add_enabled(enabled && !running, egui::Button::new(tr("自动发送")))
            .clicked()
        {
            protocol_start(app, TaskKind::Auto);
        }
        if ui
            .add_enabled(enabled && !running, egui::Button::new(tr("文件发送")))
            .clicked()
            && let Some(path) = pick_open_file(tr("选择要传输的文件"))
        {
            match std::fs::read(&path) {
                Ok(data) => start_protocol_file(app, data, &path),
                Err(err) => app.error(crate::i18n::tr1("读取文件失败: {}", err)),
            }
        }
        if ui
            .add_enabled(running, egui::Button::new(tr("停止")))
            .clicked()
        {
            app.protocol.stop(tr("用户已停止传输"));
            app.protocol_progress = None;
        }
        if let Some(kind) = app.protocol.task_kind() {
            ui.label(
                RichText::new(crate::i18n::tr1("正在{}", tr(kind.label())))
                    .color(level_color(Level::Info)),
            );
        }
    });

    if let Some((sent, total)) = app.protocol_progress {
        let ratio = if total == 0 {
            0.0
        } else {
            sent as f32 / total as f32
        };
        ui.add(
            egui::ProgressBar::new(ratio)
                .text(crate::i18n::tr2("{} / {} 字节", sent, total))
                .desired_height(16.0),
        );
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("响应区")).strong());
        if ui.button(tr("清除")).clicked() {
            app.protocol_log.clear();
        }
        ui.label(
            RichText::new(crate::i18n::tr1("{} 条", app.protocol_log.len()))
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if app.protocol_log.is_empty() {
                ui.weak(tr("暂无协议交互记录"));
            }
            for line in &app.protocol_log {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(&line.stamp)
                            .monospace()
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.label(
                        RichText::new(&line.text)
                            .monospace()
                            .small()
                            .color(level_color(line.level)),
                    );
                });
            }
        });
}

fn protocol_start(app: &mut XcomApp, kind: TaskKind) {
    let raw = app.settings.protocol_data.clone();
    let data = if raw.trim().is_empty() {
        Vec::new()
    } else {
        match crate::hex::parse(&raw) {
            Ok(bytes) => bytes,
            Err(err) => {
                app.error(crate::i18n::tr1("数据格式错误: {}", err));
                return;
            }
        }
    };
    if data.is_empty() && kind != TaskKind::File {
        app.warn(tr("请先输入要发送的数据"));
        return;
    }
    app.protocol_log.clear();
    let now = std::time::Instant::now();
    match kind {
        TaskKind::Single => app.protocol.start_single(data, now),
        TaskKind::Auto => app.protocol.start_auto(data, now),
        TaskKind::File => {}
    }
    app.info(crate::i18n::tr1("已启动{}", tr(kind.label())));
}

fn start_protocol_file(app: &mut XcomApp, data: Vec<u8>, path: &std::path::Path) {
    app.protocol_log.clear();
    app.protocol.start_file(data, std::time::Instant::now());
    app.info(crate::i18n::tr1("已启动文件传输: {}", path.display()));
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

pub(crate) fn status_bar(app: &mut XcomApp, ui: &mut Ui) {
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let open = app.link.is_open();
            led(ui, open, egui::Color32::from_rgb(0x4C, 0xAF, 0x50));
            if open {
                ui.label(
                    RichText::new(format!(
                        "{} [{}]",
                        app.settings.serial.port,
                        app.settings.serial.summary()
                    ))
                    .monospace(),
                );
            } else {
                ui.label(RichText::new(tr("未连接")).color(ui.visuals().weak_text_color()));
            }

            ui.separator();
            ui.label(RichText::new(crate::i18n::tr1("编码 {}", app.encoding_label())).monospace());

            ui.separator();
            ui.label(
                RichText::new(crate::i18n::tr2(
                    "发送: {}  接收: {}",
                    app.tx_bytes,
                    app.rx_bytes,
                ))
                .monospace(),
            );

            ui.separator();
            for (name, state) in [
                ("CTS", app.signals.cts),
                ("DSR", app.signals.dsr),
                ("DCD", app.signals.dcd),
                ("RI", app.signals.ri),
            ] {
                ui.label(RichText::new(name).small().monospace());
                led(ui, state, egui::Color32::from_rgb(0x4C, 0xAF, 0x50));
            }

            if app.show_paused {
                ui.separator();
                ui.label(
                    RichText::new(crate::i18n::tr1(
                        "已暂停（缓存 {} 字节）",
                        app.pending_rx_len(),
                    ))
                    .color(level_color(Level::Warning)),
                );
            } else if app.settings.display.timestamp && !app.log.pending_line().is_empty() {
                // Timestamp framing only emits a row once CR/LF arrives, so show
                // the incomplete frame here instead of leaving the user staring
                // at an apparently dead port.
                let preview: String = app.log.pending_line().chars().take(60).collect();
                ui.separator();
                ui.label(
                    RichText::new(crate::i18n::tr1("接收中: {}", preview))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string())
                        .monospace(),
                );
                if let Some(message) = app.latest_message() {
                    ui.separator();
                    ui.label(
                        RichText::new(&message.text)
                            .color(level_color(message.level))
                            .small(),
                    );
                }
            });
        });
    });
}

// ---------------------------------------------------------------------------
// About
// ---------------------------------------------------------------------------

pub(crate) fn about_window(app: &mut XcomApp, ctx: &Context) {
    if !app.show_about {
        return;
    }
    let mut open = true;
    let mut close_requested = false;
    // The title bar is egui's own by default, and it hard-codes its close
    // button on the right with no way to mirror it.  Drawing the header here is
    // what lets the button follow the desktop's layout; with no title bar of
    // its own, egui keeps the window draggable from anywhere.
    let window = egui::Window::new(tr("关于 XCOM"))
        .id(egui::Id::new("about_dialog"))
        .title_bar(false)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let header = dialog::header(ui, tr("关于 XCOM"), None);
            probe::set_about_header_rect(header.rect);
            probe::set_about_title_rect(header.title_rect);
            probe::set_about_close_rect(header.close_rect);
            if header.close_clicked {
                close_requested = true;
            }
            ui.separator();

            ui.label(RichText::new(tr("XCOM 串口调试助手")).strong().size(16.0));
            ui.label(crate::i18n::tr1("版本 {}", env!("CARGO_PKG_VERSION")));
            ui.separator();
            ui.label(tr(
                "使用 Rust + egui 开发的跨平台串口调试工具，可在 Windows / Linux / macOS 上运行。",
            ));
            ui.add_space(4.0);
            ui.label(tr("功能："));
            for line in [
                tr("• 串口参数全面可配（自定义波特率、1/1.5/2 停止位）"),
                tr("• 十六进制 / 文本收发，时间戳断帧"),
                tr("• 编码自动识别与手动指定"),
                tr("• 单条 / 多条 / 协议传输三种发送方式"),
                tr("• 黑色与白色主题"),
            ] {
                ui.label(line);
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr("本软件为开源实现，界面不含任何广告。"))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        });
    if let Some(window) = window {
        probe::set_about_window_rect(window.response.rect);
    }
    if !open || close_requested {
        app.show_about = false;
    }
}

// ---------------------------------------------------------------------------
// File dialogs
// ---------------------------------------------------------------------------

/// Native "open file" dialog, falling back to nothing if unavailable.
fn pick_open_file(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_file()
}

/// Native "save file" dialog.
fn pick_save_file(title: &str, default_name: &str, extension: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(default_name)
        .add_filter(extension.to_uppercase(), &[extension])
        .save_file()
}

/// Save the retained receive buffer as raw bytes, byte for byte.
fn save_raw_log(app: &mut XcomApp) {
    let Some(path) = pick_save_file(tr("保存原始数据"), "xcom-receive.bin", "bin") else {
        return;
    };
    match std::fs::write(&path, app.log.raw()) {
        Ok(()) => {
            let bytes = app.log.retained_bytes();
            app.ok(crate::i18n::tr2(
                "已保存 {} 字节到 {}",
                bytes,
                path.display(),
            ));
        }
        Err(err) => app.error(crate::i18n::tr1("保存失败: {}", err)),
    }
}

/// Save the receive log to disk in the currently displayed form.
fn save_log(app: &mut XcomApp) {
    let Some(path) = pick_save_file(tr("保存接收数据"), "xcom-receive.txt", "txt") else {
        return;
    };
    let text = app.log.render_all();
    match std::fs::write(&path, text) {
        Ok(()) => app.ok(crate::i18n::tr1("已保存到 {}", path.display())),
        Err(err) => app.error(crate::i18n::tr1("保存失败: {}", err)),
    }
}

#[cfg(test)]
mod column_tests {
    use super::multi_column_widths;

    #[test]
    fn columns_grow_with_the_panel() {
        let narrow = multi_column_widths(340.0, 6.0);
        let wide = multi_column_widths(700.0, 6.0);
        assert!(wide.0 > narrow.0, "narrow {narrow:?} wide {wide:?}");
        assert!(wide.1 > narrow.1, "narrow {narrow:?} wide {wide:?}");
    }

    #[test]
    fn columns_plus_fixed_columns_fit_the_panel() {
        // The whole point of the fix: the table must not overflow its panel.
        for available in [340.0f32, 420.0, 560.0, 700.0] {
            let (text, note) = multi_column_widths(available, 6.0);
            let fixed = 30.0 + 26.0 + 34.0 + 34.0 + 58.0;
            let used = text + note + fixed + 6.0 * 6.0;
            assert!(used <= available + 1.0, "available {available} used {used}");
        }
    }

    #[test]
    fn columns_never_go_negative_when_the_panel_is_tiny() {
        for available in [0.0f32, 40.0, 120.0, 200.0] {
            let (text, note) = multi_column_widths(available, 6.0);
            assert!(text >= 0.0 && note >= 0.0, "{available}: {text} {note}");
        }
    }
}
