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
use crate::highlight::{HighlightRule, MAX_RULES};
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
        /// Where the table's scroll area actually ended up, and how tall its
        /// content is.  A bigger content height means the user must scroll.
        static MULTI_TABLE_VIEWPORT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static MULTI_TABLE_CONTENT: Cell<egui::Vec2> = const { Cell::new(egui::Vec2::ZERO) };
        static MULTI_TABLE_SCROLLBARS: Cell<[bool; 2]> = const { Cell::new([false; 2]) };
        static MULTI_LAST_ROW_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static MULTI_SEND_STATUS_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        /// The room the tool panel had before its contents were laid out: what
        /// the panel hands the multi-send table and the bar under it.
        static TOOL_PANEL_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static BOTTOM_STATUS_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_WINDOW_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_HEADER_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_TITLE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static ABOUT_CLOSE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static TOP_BAR_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static HIGHLIGHT_TABLE_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
        static HIGHLIGHT_KEYWORD_RECTS: RefCell<Vec<egui::Rect>> =
            const { RefCell::new(Vec::new()) };
        static PROTOCOL_ROW_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static PROTOCOL_CONTENT_RECT: Cell<egui::Rect> = const { Cell::new(egui::Rect::NOTHING) };
    }

    #[cfg(test)]
    pub(crate) fn begin_protocol_panel() {
        PROTOCOL_ROW_RECTS.with(|rects| rects.borrow_mut().clear());
        PROTOCOL_CONTENT_RECT.with(|rect| rect.set(egui::Rect::NOTHING));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_protocol_panel() {}

    #[cfg(test)]
    pub(crate) fn push_protocol_row(name: &'static str, rect: egui::Rect) {
        PROTOCOL_ROW_RECTS.with(|rects| rects.borrow_mut().push((name, rect)));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_protocol_row<R>(_name: &'static str, _rect: R) {}

    #[cfg(test)]
    pub(crate) fn protocol_rows() -> Vec<(&'static str, egui::Rect)> {
        PROTOCOL_ROW_RECTS.with(|rects| rects.borrow().clone())
    }

    #[cfg(test)]
    pub(crate) fn set_protocol_content_rect(rect: egui::Rect) {
        PROTOCOL_CONTENT_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_protocol_content_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn protocol_content_rect() -> egui::Rect {
        PROTOCOL_CONTENT_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn begin_highlight_table() {
        HIGHLIGHT_TABLE_RECT.with(|cell| cell.set(egui::Rect::NOTHING));
        HIGHLIGHT_KEYWORD_RECTS.with(|rects| rects.borrow_mut().clear());
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn begin_highlight_table() {}

    #[cfg(test)]
    pub(crate) fn set_highlight_table_rect(rect: egui::Rect) {
        HIGHLIGHT_TABLE_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_highlight_table_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn push_highlight_keyword_rect(rect: egui::Rect) {
        HIGHLIGHT_KEYWORD_RECTS.with(|rects| rects.borrow_mut().push(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn push_highlight_keyword_rect<R>(_rect: R) {}

    #[cfg(test)]
    pub(crate) fn highlight_table_rect() -> egui::Rect {
        HIGHLIGHT_TABLE_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn highlight_keyword_rects() -> Vec<egui::Rect> {
        HIGHLIGHT_KEYWORD_RECTS.with(|rects| rects.borrow().clone())
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
        MULTI_TABLE_VIEWPORT.with(|rect| rect.set(egui::Rect::NOTHING));
        MULTI_TABLE_CONTENT.with(|size| size.set(egui::Vec2::ZERO));
        MULTI_TABLE_SCROLLBARS.with(|bars| bars.set([false; 2]));
        MULTI_LAST_ROW_RECT.with(|rect| rect.set(egui::Rect::NOTHING));
        MULTI_SEND_STATUS_RECT.with(|rect| rect.set(egui::Rect::NOTHING));
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
    pub(crate) fn set_multi_table(
        viewport: egui::Rect,
        content: egui::Vec2,
        scrollbars: [bool; 2],
    ) {
        MULTI_TABLE_VIEWPORT.with(|cell| cell.set(viewport));
        MULTI_TABLE_CONTENT.with(|cell| cell.set(content));
        MULTI_TABLE_SCROLLBARS.with(|cell| cell.set(scrollbars));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_multi_table<R, V, S>(_viewport: R, _content: V, _scrollbars: S) {}

    /// The scroll viewport/content dimensions and whether egui laid out a
    /// horizontal or vertical scroll bar.
    #[cfg(test)]
    pub(crate) fn multi_table() -> (egui::Rect, egui::Vec2, [bool; 2]) {
        (
            MULTI_TABLE_VIEWPORT.with(|cell| cell.get()),
            MULTI_TABLE_CONTENT.with(|cell| cell.get()),
            MULTI_TABLE_SCROLLBARS.with(|cell| cell.get()),
        )
    }

    #[cfg(test)]
    pub(crate) fn set_multi_last_row_rect(rect: egui::Rect) {
        MULTI_LAST_ROW_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_multi_last_row_rect<R>(_rect: R) {}

    /// The bottom-most entry row the table laid out.
    #[cfg(test)]
    pub(crate) fn multi_last_row_rect() -> egui::Rect {
        MULTI_LAST_ROW_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_multi_send_status_rect(rect: egui::Rect) {
        MULTI_SEND_STATUS_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_multi_send_status_rect<R>(_rect: R) {}

    /// The send status bar under the multi-send table.
    #[cfg(test)]
    pub(crate) fn multi_send_status_rect() -> egui::Rect {
        MULTI_SEND_STATUS_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_tool_panel_rect(rect: egui::Rect) {
        TOOL_PANEL_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_tool_panel_rect<R>(_rect: R) {}

    /// The room the tool panel gave its contents.
    #[cfg(test)]
    pub(crate) fn tool_panel_rect() -> egui::Rect {
        TOOL_PANEL_RECT.with(|cell| cell.get())
    }

    #[cfg(test)]
    pub(crate) fn set_bottom_status_rect(rect: egui::Rect) {
        BOTTOM_STATUS_RECT.with(|cell| cell.set(rect));
    }
    #[cfg(not(test))]
    #[inline(always)]
    pub(crate) fn set_bottom_status_rect<R>(_rect: R) {}

    /// The window's bottom status bar.
    #[cfg(test)]
    pub(crate) fn bottom_status_rect() -> egui::Rect {
        BOTTOM_STATUS_RECT.with(|cell| cell.get())
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

/// Tool panel width, in points.
///
/// The multi-send table carries eight columns -- a checkbox, an index, two text
/// fields, two more checkboxes, a delay and a send button -- and its row has to
/// fit the panel or the table grows a horizontal scroll bar that hides the send
/// buttons.  This is the narrowest panel whose text fields are still wide
/// enough to read an entry in, and it leaves the centre column the room its
/// send toolbar needs at the window the app opens with.
pub(crate) const RIGHT_PANEL_DEFAULT: f32 = 320.0;

/// Narrowest the tool panel may be.
///
/// The floor is what the multi-send table's row needs at the interface font it
/// ships with -- its columns at their narrowest, the seven gaps between them
/// and two usable text fields, plus the panel's own frame.  The splitter stops
/// here and a settings file that remembers a narrower panel is lifted to it on
/// load, because a narrower one cannot show the table: it would scroll, which
/// is exactly what the panel minimum exists to prevent.  (A larger interface
/// font, or the wider English headers, can still outgrow it — dragging the
/// splitter is then the answer, or collapsing the panel altogether.)
pub(crate) const RIGHT_PANEL_MIN: f32 = 300.0;
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
///
/// `SEND_AREA_MIN` is only the floor stored in settings.  The live floor is
/// measured from the toolbar by [`send_area_min_height`], because the toolbar
/// grows with the interface font and may occupy two rows.
pub(crate) const SEND_AREA_DEFAULT: f32 = 196.0;
pub(crate) const SEND_AREA_MIN: f32 = 96.0;
pub(crate) const SEND_AREA_MAX: f32 = 560.0;

/// Height below which the send editor is no longer worth showing, in points.
pub(crate) const SEND_EDITOR_MIN: f32 = 48.0;

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
                // Below a third of the panel's own minimum, the drag stops
                // pretending: it collapses to the window edge instead of
                // leaving an unusable sliver.  The threshold is a fixed
                // distance, and the same one on every side, so the gesture
                // reads the same whichever panel is dragged.
                let collapsed = size < spec.min / COLLAPSE_FRACTION;
                set_panel_open(&mut app.settings, spec.target, !collapsed);
                if !collapsed {
                    // The stored size is the one to restore to, so collapsing
                    // must not overwrite it.
                    set_panel_width(
                        &mut app.settings,
                        spec.target,
                        size.clamp(spec.min, spec.max),
                    );
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
        SplitterTarget::SendHeight => settings.show_send_area = open,
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
        SplitterTarget::SendHeight => tr("展开发送区"),
    }
}

/// Direction a collapsed panel's chevron points, i.e. the way it expands.
#[derive(Clone, Copy)]
enum Chevron {
    Left,
    Right,
    Up,
}

/// Draw the edge strip of a collapsed panel: a hairline that is always there,
/// and a chevron once the pointer reaches it.
fn paint_collapsed_strip(ui: &Ui, spec: &PendingSplitter, response: &egui::Response) {
    let (rect, chevron_center, direction) = match spec.growth {
        SplitterGrowth::Rightward => (
            egui::Rect::from_x_y_ranges(
                spec.boundary..=(spec.boundary + COLLAPSED_STRIP),
                spec.cross,
            ),
            egui::pos2(spec.boundary + 12.0, spec.cross.center()),
            Chevron::Right,
        ),
        SplitterGrowth::Leftward => (
            egui::Rect::from_x_y_ranges(
                (spec.boundary - COLLAPSED_STRIP)..=spec.boundary,
                spec.cross,
            ),
            egui::pos2(spec.boundary - 12.0, spec.cross.center()),
            Chevron::Left,
        ),
        SplitterGrowth::Upward => (
            egui::Rect::from_x_y_ranges(
                spec.cross,
                (spec.boundary - COLLAPSED_STRIP)..=spec.boundary,
            ),
            egui::pos2(spec.cross.center(), spec.boundary - 12.0),
            Chevron::Up,
        ),
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
        paint_chevron(ui, chevron_center, direction, visuals.selection.bg_fill);
    }
}

/// Paint a small `›` / `‹` / `˄` out of two line segments.
fn paint_chevron(ui: &Ui, center: egui::Pos2, direction: Chevron, color: egui::Color32) {
    let tip = 3.0;
    let half = 4.5;
    let stroke = egui::Stroke::new(1.8, color);
    let (start, elbow, end) = match direction {
        Chevron::Right => (
            egui::pos2(center.x - tip, center.y - half),
            egui::pos2(center.x + tip, center.y),
            egui::pos2(center.x - tip, center.y + half),
        ),
        Chevron::Left => (
            egui::pos2(center.x + tip, center.y - half),
            egui::pos2(center.x - tip, center.y),
            egui::pos2(center.x + tip, center.y + half),
        ),
        Chevron::Up => (
            egui::pos2(center.x - half, center.y + tip),
            egui::pos2(center.x, center.y - tip),
            egui::pos2(center.x + half, center.y + tip),
        ),
    };
    let painter = ui.painter();
    painter.line_segment([start, elbow], stroke);
    painter.line_segment([elbow, end], stroke);
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

/// As [`combo`], but never wider than the room left on the current row.
///
/// A combo box is a horizontal child, so egui's wrapping row cannot move it to
/// the next line after it has started laying it out.  Capping the requested
/// width to what is actually available keeps a wide choice control from
/// painting past a narrow panel.
fn combo_capped<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: &mut T,
    options: &[T],
    label: impl Fn(T) -> String,
    max_width: f32,
) -> bool {
    let width = ui.available_width().min(max_width);
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*current))
        .width(width)
        .show_ui(ui, |ui| {
            for option in options {
                if ui
                    .selectable_value(current, *option, label(*option))
                    .clicked()
                {
                    changed = true;
                }
            }
        });
    changed
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
// Menu bar
// ---------------------------------------------------------------------------

/// A drop-down menu item, with its keyboard shortcut at the right edge.
fn menu_item(ui: &mut Ui, label: &str, shortcut: &str) -> bool {
    let button = if shortcut.is_empty() {
        egui::Button::new(label)
    } else {
        egui::Button::new(label).shortcut_text(shortcut)
    };
    ui.add(button).clicked()
}

/// A menu item that opens a dialog, marked with the conventional ellipsis.
fn menu_dialog_item(ui: &mut Ui, label: &'static str) -> bool {
    menu_dialog_item_shortcut(ui, label, "")
}

/// As [`menu_dialog_item`], with the shortcut shown at the right edge.
fn menu_dialog_item_shortcut(ui: &mut Ui, label: &'static str, shortcut: &str) -> bool {
    menu_item(ui, &format!("{}…", tr(label)), shortcut)
}

/// egui temp-data key for the custom-baud dialog.
///
/// The serial panel draws the dialog and the Tools menu asks for it, so both
/// routes have to agree on where "open" is stored.
fn custom_baud_manager_id() -> egui::Id {
    egui::Id::new("custom_baud_manager")
}

/// The application menu bar.
///
/// Configuration that used to sit in one crowded strip is organised the way a
/// desktop application would: commands live in the menu of the thing they act
/// on, toggles are check items under View, and the settings that need room for
/// labels and colour pickers live in the display settings page.
pub(crate) fn menu_bar(app: &mut XcomApp, ui: &mut Ui) {
    egui::Panel::top("menu_bar").show(ui, |ui| {
        probe::begin_top_bar();
        egui::MenuBar::new().ui(ui, |ui| {
            let file = ui.menu_button(tr("文件"), |ui| file_menu(app, ui));
            probe::push_top_bar_rect("file", file.response.rect);
            let edit = ui.menu_button(tr("编辑"), |ui| edit_menu(app, ui));
            probe::push_top_bar_rect("edit", edit.response.rect);
            let view = ui.menu_button(tr("视图"), |ui| view_menu(app, ui));
            probe::push_top_bar_rect("view", view.response.rect);
            let tools = ui.menu_button(tr("工具"), |ui| tools_menu(app, ui));
            probe::push_top_bar_rect("tools", tools.response.rect);
            let help = ui.menu_button(tr("帮助"), |ui| help_menu(app, ui));
            probe::push_top_bar_rect("help", help.response.rect);
        });
    });
}

fn file_menu(app: &mut XcomApp, ui: &mut Ui) {
    // Save / Save As carry the shortcuts users expect from desktop apps.
    if menu_dialog_item_shortcut(ui, "保存接收数据", "Ctrl+S") {
        save_log(app);
    }
    if menu_dialog_item_shortcut(ui, "保存原始数据", "Ctrl+Shift+S") {
        save_raw_log(app);
    }
    ui.separator();
    if menu_dialog_item(ui, "日志文件") {
        app.show_log_file = true;
    }
    ui.separator();
    if menu_dialog_item(ui, "发送文件")
        && let Some(path) = pick_open_file(tr("选择要发送的文件"))
    {
        app.start_file_send(path);
    }
    ui.separator();
    if menu_item(ui, tr("退出"), "Ctrl+Q") {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn edit_menu(app: &mut XcomApp, ui: &mut Ui) {
    let copy = ui.add_enabled(
        app.log_view.has_selection(),
        egui::Button::new(tr("复制选中")).shortcut_text("Ctrl+C"),
    );
    if copy.clicked() {
        copy_selection(app, ui);
    }
    if menu_item(ui, tr("全选"), "Ctrl+A") {
        let store = &app.log;
        app.log_view.select_all(store);
    }
    ui.separator();
    if menu_item(ui, tr("清除接收"), "") {
        app.clear_receive();
    }
    if menu_item(ui, tr("清除发送"), "") {
        clear_send(app);
    }
}

fn view_menu(app: &mut XcomApp, ui: &mut Ui) {
    // Layout switches first (the most-used items), then content and appearance
    // submenus; the settings dialog entry closes the menu, the way Options or
    // Preferences close a Tools menu on the desktop.
    let mut changed = false;
    changed |= ui
        .checkbox(&mut app.settings.show_config_panel, tr("配置面板"))
        .changed();
    changed |= ui
        .checkbox(&mut app.settings.show_tool_panel, tr("工具面板"))
        .changed();
    changed |= ui
        .checkbox(&mut app.settings.show_send_area, tr("发送区"))
        .changed();
    if changed {
        app.mark_settings_dirty();
    }

    ui.separator();
    ui.menu_button(tr("编码"), |ui| encoding_menu(app, ui));
    ui.menu_button(tr("主题"), |ui| {
        for mode in ThemeMode::ALL {
            if ui
                .selectable_label(
                    app.settings.theme == mode,
                    format!("{} {}", mode.icon(), tr(mode.label())),
                )
                .clicked()
            {
                app.settings.theme = mode;
                app.mark_settings_dirty();
            }
        }
    });
    ui.menu_button(tr("语言"), |ui| {
        for candidate in Lang::ALL {
            if ui
                .selectable_label(i18n::lang() == candidate, candidate.label())
                .clicked()
            {
                i18n::set_lang(candidate);
                app.settings.lang = Some(candidate);
                app.mark_settings_dirty();
            }
        }
    });

    ui.separator();
    if menu_dialog_item_shortcut(ui, "显示设置", "Ctrl+,") {
        app.show_display_settings = true;
    }
}

/// Receive and transmit encodings, as two short submenus.
fn encoding_menu(app: &mut XcomApp, ui: &mut Ui) {
    ui.menu_button(tr("接收编码"), |ui| {
        if ui
            .selectable_label(app.settings.encoding.is_none(), tr("自动识别"))
            .clicked()
        {
            app.set_encoding(None);
        }
        for id in EncodingId::ALL {
            if ui
                .selectable_label(app.settings.encoding == Some(id), tr(id.label()))
                .clicked()
            {
                app.set_encoding(Some(id));
            }
        }
    });

    ui.menu_button(tr("发送编码"), |ui| {
        if ui
            .selectable_label(app.settings.tx_encoding.is_none(), tr("跟随接收编码"))
            .clicked()
        {
            app.settings.tx_encoding = None;
            app.mark_settings_dirty();
        }
        for id in EncodingId::ALL {
            if ui
                .selectable_label(app.settings.tx_encoding == Some(id), tr(id.label()))
                .clicked()
            {
                app.settings.tx_encoding = Some(id);
                app.mark_settings_dirty();
            }
        }
    });
}

fn tools_menu(app: &mut XcomApp, ui: &mut Ui) {
    // Radio-style: the active tool tab is the one currently selected, so the
    // menu also answers "which tool am I looking at".
    let multi_active = app.side_tab == SideTab::Multi && app.settings.show_tool_panel;
    if ui.selectable_label(multi_active, tr("多条发送")).clicked() {
        app.side_tab = SideTab::Multi;
        app.settings.show_tool_panel = true;
        app.mark_settings_dirty();
    }
    let protocol_active = app.side_tab == SideTab::Protocol && app.settings.show_tool_panel;
    if ui
        .selectable_label(protocol_active, tr("协议传输"))
        .clicked()
    {
        app.side_tab = SideTab::Protocol;
        app.settings.show_tool_panel = true;
        app.mark_settings_dirty();
    }
    ui.separator();
    if menu_dialog_item(ui, "管理自定义波特率") {
        app.settings.show_config_panel = true;
        app.mark_settings_dirty();
        ui.ctx()
            .data_mut(|data| data.insert_temp(custom_baud_manager_id(), true));
    }
    ui.separator();
    // Destructive and irreversible, so it never runs straight from the menu.
    if menu_dialog_item(ui, "恢复默认设置") {
        app.show_reset_confirm = true;
    }
}

fn help_menu(app: &mut XcomApp, ui: &mut Ui) {
    if menu_dialog_item(ui, "快捷键") {
        app.show_shortcuts = true;
    }
    if menu_item(ui, tr("关于 XCOM"), "") {
        app.show_about = true;
    }
}

// ---------------------------------------------------------------------------
// Display settings
// ---------------------------------------------------------------------------

/// The full display form, moved out of the configuration panel.
///
/// View → Display carries the switches used from day to day; this dialog also
/// holds the HEX layout, line spacing and fonts, which need labels and sliders
/// rather than menu rows.
pub(crate) fn display_settings_window(app: &mut XcomApp, ctx: &Context) {
    if !app.show_display_settings {
        return;
    }
    let mut open = true;
    let mut close_requested = false;
    egui::Window::new(tr("显示设置"))
        .id(egui::Id::new("display_settings_dialog"))
        .title_bar(false)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(450.0)
        .show(ctx, |ui| {
            let header = dialog::header(ui, tr("显示设置"), None);
            if header.close_clicked {
                close_requested = true;
            }
            ui.separator();
            // Keep the page inside the window even when many keyword rules
            // push it past the screen height.
            let max_height = (ctx.content_rect().height() - 120.0).clamp(240.0, 600.0);
            egui::ScrollArea::vertical()
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    display_settings_ui(app, ui);
                    ui.separator();
                    highlight_settings_ui(app, ui);
                });
        });
    if !open || close_requested {
        app.show_display_settings = false;
    }
}

// ---------------------------------------------------------------------------
// Keyword highlighting
// ---------------------------------------------------------------------------

/// Add, edit and remove the receive view's keyword rules.
///
/// This section lives inside the display settings page rather than in a dialog
/// of its own; the rules are saved with the rest of the settings, and the log
/// re-reads them every frame so changes also apply to data already on screen.
fn highlight_settings_ui(app: &mut XcomApp, ui: &mut Ui) {
    probe::begin_highlight_table();
    let title = ui.strong(tr("关键字高亮"));
    // The receive view's context menu routes here; bring the section into view
    // on the frame that arrival was requested.
    if app.scroll_to_highlights {
        title.scroll_to_me(Some(egui::Align::Min));
        app.scroll_to_highlights = false;
    }

    let mut settings = app.settings.highlights.clone();
    let mut changed = false;

    changed |= ui
        .checkbox(&mut settings.enabled, tr("启用关键字高亮"))
        .changed();

    // A manual table rather than `Grid`: Grid sizes its first column from the
    // header text, which left a long keyword field clamped to a short cell.
    // The keyword column takes whatever the 450px page leaves after the two
    // fixed columns, so the table never overflows its container.
    const COLOR_COL: f32 = 52.0;
    const ACTION_COL: f32 = 52.0;
    const COL_GAP: f32 = 8.0;
    let body_font = egui::TextStyle::Body.resolve(ui.style());
    let row_height = ui
        .spacing()
        .interact_size
        .y
        .max(ui.ctx().fonts_mut(|fonts| fonts.row_height(&body_font)) + 8.0);
    let rule_count = settings.rules.len();

    let mut remove = None;
    let table = egui::Frame::group(ui.style()).show(ui, |ui| {
        let table_width = ui.available_width();
        let keyword_col = (table_width - COLOR_COL - ACTION_COL - COL_GAP * 2.0).max(200.0);
        ui.set_width(table_width);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = COL_GAP;
            ui.add_sized(
                [keyword_col, row_height],
                egui::Label::new(RichText::new(tr("关键字")).strong()),
            );
            ui.add_sized(
                [COLOR_COL, row_height],
                egui::Label::new(RichText::new(tr("颜色")).strong()),
            );
            ui.add_sized(
                [ACTION_COL, row_height],
                egui::Label::new(RichText::new(tr("操作")).strong()),
            );
        });
        ui.separator();

        for (index, rule) in settings.rules.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = COL_GAP;
                let keyword = ui.add_sized(
                    [keyword_col, row_height],
                    egui::TextEdit::singleline(&mut rule.text)
                        .hint_text(if rule.regex {
                            tr("正则表达式")
                        } else {
                            tr("关键字")
                        })
                        .font(egui::TextStyle::Monospace),
                );
                probe::push_highlight_keyword_rect(keyword.rect);
                changed |= keyword.changed();
                ui.allocate_ui_with_layout(
                    egui::vec2(COLOR_COL, row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        changed |= ui.color_edit_button_srgb(&mut rule.color).changed();
                    },
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(ACTION_COL, row_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let toggle = ui
                            .add_sized(
                                [24.0, row_height],
                                egui::Button::new(egui::RichText::new(".*").monospace())
                                    .selected(rule.regex),
                            )
                            .on_hover_text(tr("按正则表达式匹配"));
                        if toggle.clicked() {
                            rule.regex = !rule.regex;
                            changed = true;
                        }
                        if ui
                            .add_sized([20.0, row_height], egui::Button::new("×"))
                            .on_hover_text(tr("删除"))
                            .clicked()
                        {
                            remove = Some(index);
                        }
                    },
                );
            });
            if rule.regex
                && !rule.text.is_empty()
                && let Err(err) = crate::highlight::validate_regex(&rule.text)
            {
                ui.label(
                    RichText::new(tr("正则无效"))
                        .small()
                        .color(level_color(Level::Error)),
                )
                .on_hover_text(err);
            }
            if index + 1 < rule_count {
                ui.separator();
            }
        }
    });
    probe::set_highlight_table_rect(table.response.rect);
    if let Some(index) = remove {
        settings.rules.remove(index);
        changed = true;
    }
    if settings.rules.is_empty() {
        ui.weak(tr("暂无关键字"));
    }

    ui.horizontal(|ui| {
        let full = settings.rules.len() >= MAX_RULES;
        if ui
            .add_enabled(!full, egui::Button::new(tr("添加关键字")))
            .clicked()
        {
            settings.rules.push(HighlightRule::default());
            changed = true;
        }
        ui.weak(crate::i18n::tr2(
            "{} / {} 条规则",
            settings.rules.len(),
            MAX_RULES,
        ));
    });
    ui.small(tr(
        "默认按字面匹配，区分大小写；点击 .* 后按正则表达式匹配（可用 (?i) 忽略大小写），重叠时较长匹配优先。",
    ));

    if changed {
        settings.normalize();
        app.settings.highlights = settings;
        app.mark_settings_dirty();
    }
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
    let (boundary, cross) = match growth {
        SplitterGrowth::Rightward => (area.left(), area.y_range()),
        SplitterGrowth::Leftward => (area.right(), area.y_range()),
        SplitterGrowth::Upward => (area.bottom(), area.x_range()),
    };
    PendingSplitter {
        id,
        growth,
        boundary,
        cross,
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
    let manager_id = custom_baud_manager_id();
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

/// Every display control, without a container.
///
/// The left panel used to host this; it now lives in a dialog opened from
/// **View → Display settings…**, so the configuration panel can stay focused on
/// the serial connection.
fn display_settings_ui(app: &mut XcomApp, ui: &mut Ui) {
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
        ui.small(tr("以回车换行断帧；未收全的一帧先显示当前时间，收全后定帧"));
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
        app.set_auto_scroll(auto_scroll);
    }

    let mut echo = app.settings.echo_sent;
    if ui
        .checkbox(&mut echo, tr("回显发送"))
        .on_hover_text(tr("把发出的内容显示在接收区，便于确认是否真的发出"))
        .changed()
    {
        app.settings.echo_sent = echo;
        app.mark_settings_dirty();
    }

    ui.horizontal(|ui| {
        ui.label(tr("行距"));
        let mut spacing = app.settings.line_spacing;
        if ui
            .add(
                egui::Slider::new(
                    &mut spacing,
                    crate::settings::LINE_SPACING_MIN..=crate::settings::LINE_SPACING_MAX,
                )
                .fixed_decimals(0),
            )
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

    ui.separator();
    ui.strong(tr("接收区配色"));
    let mut colors = app.settings.log_colors;
    if ui
        .checkbox(&mut colors.enabled, tr("自定义接收区配色"))
        .changed()
    {
        app.settings.log_colors = colors;
        app.mark_settings_dirty();
    }
    if colors.enabled {
        ui.horizontal(|ui| {
            ui.label(tr("字色"));
            if ui.color_edit_button_srgb(&mut colors.foreground).changed() {
                app.settings.log_colors = colors;
                app.mark_settings_dirty();
            }
            ui.label(tr("底色"));
            if ui.color_edit_button_srgb(&mut colors.background).changed() {
                app.settings.log_colors = colors;
                app.mark_settings_dirty();
            }
        });
    }
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
        // Its floor depends on the interface font and on whether the action
        // group needs a second row: a constant minimum let the separator eat
        // into the toolbar when the user dragged it down.
        let min_height = send_area_min_height(app, ui);
        if app.settings.show_send_area {
            let height = app.settings.send_area_height.max(min_height);
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
                current_size: height,
                min: min_height,
                max: SEND_AREA_MAX,
                target: SplitterTarget::SendHeight,
                collapsed: false,
            });
        } else {
            // Collapsed: keep a strip along the bottom edge so the area stays
            // draggable (and clickable) back open.  The stored height is left
            // untouched, so it is restored when the area reopens.
            let area = ui.available_rect_before_wrap();
            app.pending_splitters.push(collapsed_splitter(
                "send_area",
                SplitterGrowth::Upward,
                SplitterTarget::SendHeight,
                area,
                min_height,
                SEND_AREA_MAX,
            ));
        }

        let palette = theme::log_palette(ui.ctx(), app.settings.theme, app.settings.log_colors);
        let rect = ui.available_rect_before_wrap();
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, palette.background);
        let output = draw_log(app, ui, palette);

        // The receive toolbar is gone; its secondary actions live in the log's
        // context menu and report through the view's output.
        if output.toggle_pause {
            if app.show_paused {
                app.resume_display();
            } else {
                app.show_paused = true;
                app.info(tr("显示已暂停，数据仍在后台接收"));
            }
        }
        let mut display = app.settings.display;
        let display_changed = output.toggle_hex
            || output.toggle_markers
            || output.toggle_timestamp
            || output.toggle_wrap;
        if output.toggle_hex {
            display.mode = if display.mode == ViewMode::Hex {
                ViewMode::Text
            } else {
                ViewMode::Hex
            };
        }
        if output.toggle_markers {
            display.markers = !display.markers;
        }
        if output.toggle_timestamp {
            display.timestamp = !display.timestamp;
        }
        if output.toggle_wrap {
            display.wrap = !display.wrap;
        }
        if display_changed {
            app.settings.display = display;
            app.log.set_options(display);
            app.mark_settings_dirty();
        }
        if output.toggle_auto_scroll {
            app.set_auto_scroll(!app.settings.auto_scroll);
        }
        if output.toggle_echo {
            app.settings.echo_sent = !app.settings.echo_sent;
            app.mark_settings_dirty();
        }
        if output.open_highlight_settings {
            app.show_display_settings = true;
            app.scroll_to_highlights = true;
        }
        if output.clear_received {
            app.clear_receive();
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
        clear_send(app);
    }
}

/// Empty the send buffer and reset the session counters.
fn clear_send(app: &mut XcomApp) {
    app.settings.send.buffer.clear();
    app.tx_bytes = 0;
    app.rx_bytes = 0;
    app.mark_settings_dirty();
    app.info(tr("已清除发送区与收发计数"));
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

/// Height of one send-toolbar row at the current interface font.
///
/// Checkboxes and buttons grow with the interface font, so a fixed row height
/// would clip them as soon as the user chose a larger size.
fn send_toolbar_row_height(app: &XcomApp, ui: &Ui) -> f32 {
    28.0_f32.max(
        ui.ctx().fonts_mut(|fonts| {
            fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
        }) + 10.0,
    )
}

/// Width the send action group needs on one row, at the current font.
///
/// The three buttons are laid out at fixed widths; the history combo is at
/// least 68 points but grows with its label, which differs between languages.
fn send_actions_width(ui: &Ui) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let label = ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(tr("历史").to_owned(), font, egui::Color32::WHITE)
            .size()
            .x
    });
    let history = (label
        + ui.spacing().icon_width
        + ui.spacing().icon_spacing
        + 2.0 * ui.spacing().button_padding.x)
        .max(68.0);
    // 发送 + 发送文件 + 清除发送 + 历史, with three 6-point gaps.
    96.0 + 76.0 + 76.0 + history + 18.0
}

/// The smallest send panel that still shows the whole toolbar and an editor.
///
/// The toolbar may occupy one or two rows depending on the labels and the
/// panel width, each row grows with the interface font, and a file
/// transmission adds a progress bar.  A constant minimum cannot cover those
/// cases, so the separator could be dragged down until the toolbar was
/// clipped.  Sum the parts the panel itself will lay out: its frame margins,
/// two toolbar rows, the gap before the editor, the optional progress bar and
/// the editor's own floor.  Reserving both rows is deliberate: the editor takes
/// any extra room on a wide window, and the floor can then never undercut a
/// toolbar that wrapped at run time.
fn send_area_min_height(app: &XcomApp, ui: &Ui) -> f32 {
    let frame = egui::Frame::side_top_panel(ui.style());
    let margin = frame.total_margin().sum();
    let row = send_toolbar_row_height(app, ui);
    let gap = ui.spacing().item_spacing.y;
    let rows = 2.0;
    let progress = if app.file_send.is_some() {
        16.0 + gap
    } else {
        0.0
    };
    // One point of slack: egui rounds the panel's edges independently, and a
    // floor that lands exactly on the boundary can clip the editor's last row.
    (margin.y + rows * row + (rows - 1.0) * gap + gap + progress + SEND_EDITOR_MIN + 1.0)
        .clamp(SEND_AREA_MIN, SEND_AREA_MAX)
}

/// Lay out only the send editor and report the height it took, so a test can
/// observe that dragging the separator really resizes it.
fn send_area(app: &mut XcomApp, ui: &mut Ui) {
    probe::set_send_panel_rect(ui.max_rect());
    probe::begin_send_toolbar();

    // Toolbar row height: tall enough that text/checkboxes are not clipped.
    let btn_height = send_toolbar_row_height(app, ui);
    let mut actions_on_second_row = false;

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

        // The interval is configured before the box is ticked, so it comes
        // first and locks once sending starts.
        let can_edit_interval = app.link.is_open() && !app.settings.send.timed;
        let interval_label = ui.label(tr("定时间隔"));
        probe::push_send_toolbar_rect("interval_label", interval_label.rect);
        let mut period = app.settings.send.period_ms;
        let interval = ui.add_enabled(
            can_edit_interval,
            egui::DragValue::new(&mut period)
                .speed(10.0)
                .range(1..=3_600_000)
                .suffix(" ms"),
        );
        probe::push_send_toolbar_rect("interval", interval.rect);
        if interval.changed() {
            app.settings.send.period_ms = period;
            app.mark_settings_dirty();
        }

        let mut timed = app.settings.send.timed;
        let can_timed = app.link.is_open();
        let timed_control =
            ui.add_enabled(can_timed, egui::Checkbox::new(&mut timed, tr("定时发送")));
        probe::push_send_toolbar_rect("timed", timed_control.rect);
        if timed_control.changed() {
            // A run state: no need to persist it.
            app.settings.send.timed = timed;
        }

        // Decide from the room actually left after the options were laid out,
        // not from a width guess: when the group does not fit, the whole row
        // moves below instead of the first buttons overlapping the options.
        if ui.available_width() >= send_actions_width(ui) + SEND_RIGHT_PAD {
            right_aligned_send_actions(app, ui, btn_height);
        } else {
            actions_on_second_row = true;
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
    let editor_size = egui::vec2(available.x, available.y.max(SEND_EDITOR_MIN));
    let editor = ui.add(
        egui::TextEdit::multiline(&mut app.settings.send.buffer)
            .font(crate::fonts::send_font_id(app.settings.fonts.send_size))
            .desired_width(editor_size.x)
            // One row plus `min_size`: the editor fills the room the panel
            // gives it instead of insisting on four rows and overflowing the
            // panel when the user drags the separator all the way down.
            .desired_rows(1)
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
    // The panel cannot go below the width the multi-send table's row needs of
    // it -- a narrower one would grow a horizontal scroll bar, which is what
    // hides the send buttons.  The width follows the interface font and the
    // language, so it is measured, not a constant.
    let min = RIGHT_PANEL_MIN.max(multi_table_min_width(ui, app.settings.fonts.ui_size));
    let max = (available.width() - CENTER_MIN - SEPARATOR_SLACK).max(min);

    if !app.settings.show_tool_panel {
        app.pending_splitters.push(collapsed_splitter(
            "tool_panel",
            SplitterGrowth::Leftward,
            SplitterTarget::ToolWidth,
            available,
            min,
            max,
        ));
        return;
    }

    let width = app.settings.right_panel_width.clamp(min, max);
    let panel = egui::Panel::right("tool_panel")
        .resizable(false)
        .exact_size(width)
        .show(ui, |ui| {
            probe::set_tool_panel_rect(ui.max_rect());
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

/// Lay out one row of the send status bar, exactly `height` tall.
///
/// The bar's height is what the table's height is derived from, so a row that
/// grew with its contents — a wrapped label, a taller font — would push the bar
/// out of the panel and under the window's status bar.  Content that does not
/// fit is truncated rather than allowed to widen the row.
fn status_row<R>(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    add(&mut row)
}

/// How wide the six columns that do not grow would like to be, and how narrow
/// the widget inside each may get.
///
/// The floors are not taste: egui refuses to lay a text field out narrower than
/// 24 points, and a delay field with its unit needs room of its own.  A column
/// also has to hold its header, which is text and is measured — see
/// [`multi_column_widths`].
const MULTI_FIXED_COLUMNS: [(f32, f32); 6] = [
    (30.0, 16.0), // 启用
    (24.0, 18.0), // #
    (30.0, 16.0), // HEX
    (32.0, 16.0), // 新行
    (58.0, 42.0), // 延迟
    (52.0, 38.0), // 发送
];

/// The narrowest a text column may get, header text aside.
const MULTI_TEXT_MIN: f32 = 32.0;

/// The width a text column wants before the panel is judged roomy enough: a few
/// characters of the entry, wide enough to see what is being typed.
const MULTI_TEXT_COMFORT: f32 = 50.0;

/// The gap between the multi-send table's columns.
const MULTI_COLUMN_GAP: f32 = 5.0;

/// Leave a little room at the right edge: egui lays out each column in a
/// separate centered child and rounds its edges independently.  Without this,
/// accumulated sub-pixel rounding can put the last column a fraction beyond the
/// viewport and egui shows a horizontal scrollbar.
const MULTI_COLUMN_END_GUTTER: f32 = 1.0;

/// The columns of the multi-send table, in one place.
///
/// The order a row lays them out is 启用, `#`, 内容, 备注, HEX, 新行, 延迟, 发送,
/// and the fixed columns are at `0, 1, 4, 5, 6, 7` of it.
const MULTI_FIXED_AT: [usize; 6] = [0, 1, 4, 5, 6, 7];

/// The header text above each column, in row order.
const MULTI_HEADERS: [&str; 8] = ["启用", "#", "内容", "备注", "HEX", "新行", "延迟", ""];

/// What each header measures at the interface font in use, in row order.
///
/// A header wider than its column widens the whole row instead of being
/// clipped, and what a header measures depends on the interface font and on the
/// language — “Content” is twice the width of 内容 — so it cannot be a
/// constant.
fn multi_header_widths(ctx: &egui::Context, ui_size: f32) -> [f32; 8] {
    multi_header_widths_in(ctx, ui_size, i18n::lang())
}

/// What each header measures in one explicit language.
fn multi_header_widths_in(ctx: &egui::Context, ui_size: f32, language: Lang) -> [f32; 8] {
    let font = egui::FontId::proportional(ui_size);
    MULTI_HEADERS.map(|header| {
        ctx.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(
                    i18n::tr_lang(language, header).to_owned(),
                    font.clone(),
                    egui::Color32::WHITE,
                )
                .size()
                .x
        })
    })
}

/// The narrowest each column may be: the widget inside it and the header above.
fn multi_column_floors(headers: [f32; 8]) -> [f32; 8] {
    // A header sits flush in its cell, so it needs a point of its own on top of
    // its text: the cell is as wide as the label and would otherwise round over.
    let header_room = |index: usize| headers[index].ceil() + 1.0;
    let mut floor = [0.0_f32; 8];
    for (slot, index) in MULTI_FIXED_AT.into_iter().enumerate() {
        floor[index] = MULTI_FIXED_COLUMNS[slot].1.max(header_room(index));
    }
    for index in [2, 3] {
        floor[index] = MULTI_TEXT_MIN.max(header_room(index));
    }
    floor
}

/// The narrowest the multi-send table's row can be made.
fn multi_row_floor(headers: [f32; 8]) -> f32 {
    let floor = multi_column_floors(headers);
    let fixed: f32 = MULTI_FIXED_AT.iter().map(|index| floor[*index]).sum();
    fixed + floor[2].max(floor[3]) * 2.0 + MULTI_COLUMN_GAP * 7.0 + MULTI_COLUMN_END_GUTTER
}

/// The narrowest the tool panel can be and still show the whole multi-send
/// table.
///
/// The panel hands its contents its own width less its frame, and draws a
/// separator line inside the edge it resizes from; both come off the table's
/// row.  The panel's floor and the splitter's minimum are taken from here, so
/// neither can leave the table narrower than it can be — at whatever interface
/// font is in use.
///
/// The measurement is taken across *every* supported language, not just the
/// active one, and the widest header wins.  Otherwise the panel resized when
/// the user switched between Chinese and English: the English headers are
/// wider, so the same stored width produced a visibly narrower panel in
/// Chinese.
pub(crate) fn multi_table_min_width(ui: &Ui, ui_size: f32) -> f32 {
    let frame = egui::Frame::side_top_panel(ui.style());
    // The frame's margins, the separator line, and a point to spare so the row
    // does not end up rounding over the edge.
    let insets = frame.total_margin().sum().x + 1.0 + 1.0;
    let headers = Lang::ALL
        .into_iter()
        .map(|language| multi_header_widths_in(ui.ctx(), ui_size, language))
        .fold([0.0_f32; 8], |widest, widths| {
            std::array::from_fn(|index| widest[index].max(widths[index]))
        });
    multi_row_floor(headers) + insets
}

/// Widths for the multi-send table's columns, in the order a row lays them out:
/// 启用, `#`, 内容, 备注, HEX, 新行, 延迟, 发送.
///
/// The eight columns and the seven gaps between them must add up to the table's
/// width and no more: one point over and the table grows a horizontal scroll
/// bar, which puts the send buttons and the delay fields out of reach.  So the
/// two text columns take what the six fixed ones leave, 62/38 and never below
/// [`MULTI_TEXT_MIN`]; when that is not enough for two usable text fields the
/// fixed columns give up room in proportion to what each can spare.
///
/// `headers` is the measured width of each column's header.  A header wider
/// than its column widens the whole row instead of being clipped, and what a
/// header measures depends on the interface font and on the language — “Content”
/// is twice the width of 内容 — so it cannot be a constant.
///
/// The flag is `true` when even the floors are too wide, so the panel is
/// narrower than the table can ever be and a horizontal scroll bar is the
/// honest answer.
fn multi_column_widths(viewport: f32, gap: f32, headers: [f32; 8]) -> ([f32; 8], bool) {
    let gaps = gap * 7.0;
    let floor = multi_column_floors(headers);
    let mut ideal = floor;
    for (slot, index) in MULTI_FIXED_AT.into_iter().enumerate() {
        ideal[index] = MULTI_FIXED_COLUMNS[slot].0.max(floor[index]);
    }

    let fixed_ideal: f32 = MULTI_FIXED_AT.iter().map(|i| ideal[*i]).sum();
    let fixed_floor: f32 = MULTI_FIXED_AT.iter().map(|i| floor[*i]).sum();
    let text_min = floor[2].max(floor[3]);
    let text_comfort = MULTI_TEXT_COMFORT.max(text_min);

    let mut widths = ideal;
    let mut free = viewport - gaps - fixed_ideal;
    if free < 2.0 * text_comfort {
        // Squeeze the fixed columns towards their floors, each giving up the
        // same fraction of the room it has to give.  A panel too narrow for two
        // comfortable text fields is better off with narrower checkboxes and a
        // narrower delay field than with text fields nothing can be read in.
        let slack = (fixed_ideal - fixed_floor).max(1.0);
        let need = (2.0 * text_comfort - free).min(fixed_ideal - fixed_floor);
        for index in MULTI_FIXED_AT {
            widths[index] = ideal[index] - (ideal[index] - floor[index]) * need / slack;
        }
        free = viewport - gaps - MULTI_FIXED_AT.iter().map(|i| widths[*i]).sum::<f32>();
    }

    // Below two usable text columns there is nothing left to squeeze, and the
    // table is wider than its panel by construction.
    let overflows = free < 2.0 * text_min - 0.5;
    let text_floor = if free >= 2.0 * text_comfort {
        text_comfort
    } else {
        text_min
    };
    let content = if overflows {
        text_min
    } else {
        // `free` is at least two text columns wide here, so both fit; the
        // ordering keeps the halves honest when it is only just wide enough.
        (free * 0.62).min(free - text_floor).max(text_floor)
    };
    widths[2] = content;
    widths[3] = if overflows { text_min } else { free - content };

    (widths, overflows)
}

/// Number of entries the table *looks* like it can hold from its allotted
/// height, before egui has laid it out. Used only as the seed on the first
/// frame (or right after a resize); the real count is then read back from the
/// `ScrollArea` viewport and corrects this in one frame (see `multi_panel`).
/// The table has one header row, then `count` entry rows, with `row_gap`
/// between each pair of rows. Shared by production code and tests.
fn multi_page_size(table_height: f32, row_height: f32, row_gap: f32) -> usize {
    if !table_height.is_finite() || !row_height.is_finite() || !row_gap.is_finite() {
        return 1;
    }
    let row_height = row_height.max(1.0);
    let pitch = row_height + row_gap.max(0.0);
    (((table_height.max(row_height) - row_height) / pitch).floor() as usize).max(1)
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
            app.settings.multi.page_count(),
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
            .on_hover_text(tr("按顺序发送整个发送池里所有已勾选且非空的条目"))
            .clicked()
        {
            app.start_multi_run(false, std::time::Instant::now());
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
            if loop_enabled {
                app.start_multi_run(true, std::time::Instant::now());
            } else {
                app.stop_multi_run(Level::Info, tr("已停止循环发送"));
            }
            app.mark_settings_dirty();
        }
    });

    ui.horizontal_wrapped(|ui| {
        ui.label(tr("循环次数"));
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
    let active_colours = theme::active_entry_colors(ui.ctx(), app.settings.theme);
    let mut send_request: Option<usize> = None;
    let mut dirty = false;

    // egui's `Grid` reuses the previous frame's column widths as a *floor*
    // (`child_size.max(prev_width)`), so a table built with it can grow but
    // never shrink -- dragging the splitter back left the columns too wide and
    // pushed the send button off the edge.  Laying the rows out by hand with
    // explicit widths is what makes them track the panel in both directions.
    /// The gap under a table row, and between the send status bar's two rows.
    const ROW_GAP: f32 = 4.0;

    let row_height = 20.0_f32.max(
        ui.ctx().fonts_mut(|fonts| {
            fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
        }) + 4.0,
    );

    // The whole row -- eight columns and the seven gaps between them -- has to
    // fit the table.  A row one point wider grows a horizontal scroll bar,
    // which takes the send buttons out of reach and eats into the bottom of the
    // table, hiding the very entry the bar was scrolled to show.
    let header_widths = multi_header_widths(ui.ctx(), app.settings.fonts.ui_size);
    let table_width = (ui.available_width() - MULTI_COLUMN_END_GUTTER).max(0.0);
    let (columns, columns_overflow) =
        multi_column_widths(table_width, MULTI_COLUMN_GAP, header_widths);
    let [
        enable_w,
        index_w,
        text_width,
        note_width,
        hex_w,
        newline_w,
        delay_w,
        send_w,
    ] = columns;

    // The send status bar is pinned under the table: a rule and one line
    // carrying the loop state, the loop count and the entry in flight.  Its
    // height is fixed, so the table's height -- and the page boundary with it
    // -- does not move when a run starts or stops; text that does not fit is
    // truncated rather than allowed to wrap into a second line.
    let bar_height = row_height + ROW_GAP;
    let status_height = bar_height + ui.spacing().item_spacing.y;

    // The pool is a fixed 100-slot list, so a page is a window onto it.
    // `table_height` is only the upper bound passed to ScrollArea. It is NOT
    // necessarily the final visible viewport height: egui can further constrain
    // the scroll area to the panel's actual clip rect and scrollbar geometry.
    // Therefore the page size is calibrated from `ScrollAreaOutput.inner_rect`
    // below, rather than trusting this pre-layout estimate.
    let mut table_height = (ui.available_height() - status_height).max(row_height);
    if columns_overflow {
        // Reserve the horizontal scrollbar's thickness when it is non-floating.
        table_height = (table_height - ui.spacing().scroll.allocated_width()).max(row_height);
    }

    let page_size_id = egui::Id::new("xcom_multi_page_size");
    // Last frame's page size, measured from the real `ScrollArea` viewport.
    // The panel re-measures every frame and writes the corrected value back, so
    // the count tracks the available height both ways — it grows when there is
    // room for more rows and shrinks when the window or splitter is pulled in.
    let cached_page_size = ui.ctx().data(|data| data.get_temp::<usize>(page_size_id));
    let page_size =
        cached_page_size.unwrap_or_else(|| multi_page_size(table_height, row_height, ROW_GAP));
    app.settings.multi.page_size = page_size;
    app.settings.multi.clamp_page();
    let page = app.settings.multi.page;
    let range = app.settings.multi.page_range();

    // A run spans the whole pool, so its marker belongs on whichever page the
    // entry is currently shown on — and nowhere else, which is why the index
    // must fall inside the visible range.
    let active_index = app
        .multi_run_sent()
        .map(|sent| sent.index)
        .filter(|index| range.contains(index));

    let table =
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .max_height(table_height)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(MULTI_COLUMN_GAP, ROW_GAP);
                probe::begin_multi_headers();

                ui.horizontal(|ui| {
                    for (width, header) in columns.into_iter().zip(MULTI_HEADERS) {
                        let header = ui.add_sized(
                            [width, row_height],
                            egui::Label::new(RichText::new(tr(header)).strong()),
                        );
                        probe::push_multi_header_rect(header.rect);
                    }
                });

                for index in range.clone() {
                    let Some(entry) = app.settings.multi.entry_mut(index) else {
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
                                active_colours.wash,
                            );
                            ui.painter().rect_filled(
                                egui::Rect::from_min_max(
                                    row.min,
                                    egui::pos2(row.min.x + 3.0, row.max.y),
                                ),
                                egui::CornerRadius::ZERO,
                                active_colours.bar,
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
                                egui::TextEdit::singleline(&mut entry.text)
                                    .hint_text(if entry.hex { "01 02 0A" } else { tr("指令") }),
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
                    probe::set_multi_last_row_rect(row_rect);
                }
            });

    // `inner_rect` is the real viewport after egui has applied clipping and
    // scrollbar layout, and `content_size` is what the rows actually took up.
    // Together they let the page size track the available height exactly,
    // without a hardcoded cap or a platform-specific "maximized" flag: if the
    // rows overflowed the viewport, drop as many as spilled; if room for more
    // remains, claim every whole row that fits. Both directions converge in a
    // single frame, so the page neither scrolls unnecessarily nor wastes a row.
    let pitch = row_height + ROW_GAP;
    let viewport_h = table.inner_rect.height();
    let content_h = table.content_size.y;
    let next_page_size = if content_h > viewport_h + 0.5 {
        let spilled = ((content_h - viewport_h) / pitch).ceil() as usize;
        page_size.saturating_sub(spilled.max(1)).max(1)
    } else {
        let room = (viewport_h - content_h).max(0.0);
        page_size + ((room / pitch).floor() as usize)
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(page_size_id, next_page_size));
    if next_page_size != page_size {
        app.settings.multi.page_size = next_page_size;
        app.settings.multi.clamp_page();
        ui.ctx().request_repaint();
    }

    let scrollbars = [
        ui.ctx().read_response(table.id.with(0)).is_some(),
        ui.ctx().read_response(table.id.with(1)).is_some(),
    ];
    probe::set_multi_table(table.inner_rect, table.content_size, scrollbars);
    // ---- send status bar --------------------------------------------------
    // Pinned under the table and always visible, so the table keeps its height
    // as a run starts and stops.  The bar is one row whatever the run: the
    // fixed height is what the table's height is derived from, and a bar that
    // wrapped would move the page boundary under a running loop.  Only
    // read-only state lives here; the loop count is edited with the controls
    // above.
    let status = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), bar_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            // The height is the table's arithmetic, not a hope: whatever the
            // rows do, the region is this tall.
            ui.set_min_height(bar_height);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rule, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), ROW_GAP),
                egui::Sense::hover(),
            );
            ui.painter().hline(
                rule.x_range(),
                rule.center().y,
                ui.visuals().widgets.noninteractive.bg_stroke,
            );

            let running = app.multi_run_active();
            let looping = app.settings.multi.loop_enabled;
            status_row(ui, row_height, |ui| {
                ui.label(tr("循环"));
                if running && looping {
                    ui.label(
                        RichText::new(tr("循环中"))
                            .strong()
                            .color(active_colours.bar),
                    );
                } else {
                    ui.label(RichText::new(tr("未循环")).color(ui.visuals().weak_text_color()));
                }
                ui.separator();
                ui.label(tr("循环次数"));
                match app.settings.multi.loop_count {
                    None => {
                        ui.label(
                            RichText::new(tr("不限次数"))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                    Some(limit) => {
                        ui.add(
                            egui::Label::new(
                                RichText::new(crate::i18n::tr2(
                                    "已完成 {} / {} 次",
                                    app.multi_cycles,
                                    limit,
                                ))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                            )
                            .truncate(),
                        );
                    }
                }

                ui.separator();
                ui.label(tr("当前条目"));
                match app.multi_run_sent() {
                    Some(sent) => {
                        let text = crate::i18n::tr3(
                            "正在发送 #{} ({} / {})",
                            sent.index,
                            sent.ordinal,
                            sent.round_len,
                        );
                        probe::set_multi_status_text(&text);
                        // The entry in flight is the part that gives way when
                        // the panel is too narrow for the whole line.
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).strong().color(active_colours.bar),
                            )
                            .truncate(),
                        );
                    }
                    None => {
                        ui.label(RichText::new(tr("空闲")).color(ui.visuals().weak_text_color()));
                    }
                }
            });
        },
    );
    probe::set_multi_send_status_rect(status.response.rect);

    if dirty {
        app.mark_settings_dirty();
    }
    if let Some(index) = send_request {
        app.send_multi_entry(index, false);
    }

    // ---- numeric keypad shortcuts ----------------------------------------
    if app.settings.multi.keypad_enabled && !ui.ctx().egui_wants_keyboard_input() {
        let base = page * page_size;
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
            app.send_multi_entry(base + index, false);
        }
    }
}

fn protocol_panel(app: &mut XcomApp, ui: &mut Ui) {
    let running = app.protocol.is_running();
    let mut config = app.protocol.config().clone();
    probe::begin_protocol_panel();

    // The panel is narrow, and these are one control per row rather than a
    // packed grid.  A `ComboBox` is itself a horizontal child: on a wrapped row
    // with too little room left, egui hands that child the remaining width and
    // then paints the fixed-width combo past the panel edge instead of wrapping
    // it.  Giving each control a row of its own — and capping the combo to the
    // room actually left — keeps the English labels inside the panel.
    let identity = ui.horizontal_wrapped(|ui| {
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
    });
    probe::push_protocol_row("identity", identity.response.rect);

    let checksum = ui.horizontal_wrapped(|ui| {
        ui.label(tr("校验"));
        let mut kind = config.checksum;
        if combo_capped(
            ui,
            "protocol_checksum",
            &mut kind,
            &ChecksumKind::ALL,
            |c| tr(c.label()).to_owned(),
            SERIAL_COMBO_WIDTH,
        ) {
            config.checksum = kind;
        }
    });
    probe::push_protocol_row("checksum", checksum.response.rect);

    let period = ui.horizontal_wrapped(|ui| {
        ui.label(tr("帧周期"));
        ui.add(
            egui::DragValue::new(&mut config.frame_period_ms)
                .speed(10.0)
                .range(1..=600_000)
                .suffix(" ms"),
        );
    });
    probe::push_protocol_row("period", period.response.rect);

    let retries = ui.horizontal_wrapped(|ui| {
        ui.label(tr("最大重发"));
        ui.add(
            egui::DragValue::new(&mut config.max_retries)
                .speed(1.0)
                .range(0..=255),
        );
    });
    probe::push_protocol_row("retries", retries.response.rect);

    let data_len = ui.horizontal_wrapped(|ui| {
        ui.label(tr("最大数据长度"));
        ui.add(
            egui::DragValue::new(&mut config.max_data_len)
                .speed(1.0)
                .range(1..=255),
        );
    });
    probe::push_protocol_row("data_len", data_len.response.rect);

    let options = ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut config.auto_send, tr("自动发送"));
        ui.checkbox(&mut config.show_raw, tr("显示原始帧"));
        ui.checkbox(&mut config.auto_wrap, tr("自动换行"));
    });
    probe::push_protocol_row("options", options.response.rect);

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

    let actions = ui.horizontal_wrapped(|ui| {
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
    probe::push_protocol_row("actions", actions.response.rect);

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
    let response_header = ui.horizontal(|ui| {
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
    probe::push_protocol_row("response_header", response_header.response.rect);

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
    probe::set_protocol_content_rect(ui.min_rect());
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
    let panel = egui::Panel::bottom("status_bar").show(ui, |ui| {
        // One row, never wrapping: the bar's height is part of the centre
        // column's layout, so anything that can make it taller (a long hint,
        // a growing counter) resizes the receive area underneath it.  Labels
        // that do not fit are clipped instead.
        ui.horizontal(|ui| {
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
            // Auto-detection context, moved here from the Encoding submenu so
            // the menu is only choices.
            if app.settings.encoding.is_none() {
                ui.separator();
                ui.label(
                    RichText::new(crate::i18n::tr1("已分析 {} 字节", app.auto.total()))
                        .small()
                        .monospace()
                        .color(ui.visuals().weak_text_color()),
                );
            }

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
    probe::set_bottom_status_rect(panel.response.rect);
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
// Reset confirmation / keyboard shortcuts
// ---------------------------------------------------------------------------

/// Settings for writing the receive stream to disk while it arrives.
///
/// Logging itself is a session state (like timed send), so this dialog only
/// owns the directory and the file-name pattern; the app never starts writing
/// on launch.
pub(crate) fn log_file_window(app: &mut XcomApp, ctx: &Context) {
    if !app.show_log_file {
        return;
    }
    let mut open = true;
    let mut close_requested = false;
    egui::Window::new(tr("日志文件"))
        .id(egui::Id::new("log_file_dialog"))
        .title_bar(false)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(470.0)
        .show(ctx, |ui| {
            let header = dialog::header(ui, tr("日志文件"), None);
            if header.close_clicked {
                close_requested = true;
            }
            ui.separator();

            let mut enabled = app.log_file.is_active();
            if ui
                .checkbox(&mut enabled, tr("启用实时保存（接收数据）"))
                .changed()
            {
                app.set_log_file_enabled(enabled);
            }
            match app.log_file.path() {
                Some(path) => {
                    ui.label(
                        RichText::new(crate::i18n::tr1("正在写入: {}", path.display()))
                            .small()
                            .color(level_color(Level::Success)),
                    );
                }
                None => {
                    ui.small(tr("未启用：不会创建文件。"));
                }
            }

            ui.add_space(8.0);
            ui.label(RichText::new(tr("保存目录")).strong());
            ui.horizontal(|ui| {
                let mut directory = app.settings.log_file.directory.clone();
                let width = (ui.available_width() - 76.0).max(80.0);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut directory)
                        .hint_text(tr("留空则使用配置目录下的 logs"))
                        .desired_width(width),
                );
                if response.changed() {
                    app.settings.log_file.directory = directory;
                    app.mark_settings_dirty();
                }
                if ui.button(tr("选择…")).clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    app.settings.log_file.directory = path.display().to_string();
                    app.mark_settings_dirty();
                }
            });
            if app.settings.log_file.directory.trim().is_empty() {
                let directory = crate::logfile::resolve_directory("");
                ui.small(crate::i18n::tr1("默认目录: {}", directory.display()));
            }

            ui.add_space(8.0);
            ui.label(RichText::new(tr("文件名格式")).strong());
            let mut pattern = app.settings.log_file.pattern.clone();
            let response = ui.add(
                egui::TextEdit::singleline(&mut pattern)
                    .hint_text(crate::logfile::DEFAULT_PATTERN)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
            if response.changed() {
                app.settings.log_file.pattern = pattern;
                app.mark_settings_dirty();
            }
            let preview =
                crate::logfile::file_name_for(&app.settings.log_file.pattern, chrono::Local::now());
            ui.small(crate::i18n::tr1("文件名示例: {}", preview));
            ui.add_space(4.0);
            ui.small(tr(
                "格式符：%Y 年（4 位）、%y 年（2 位）、%M 月、%D/%d 日、%h/%H 时、%m 分、%s/%S 秒、%f 毫秒、%% 百分号。",
            ));
            ui.small(tr(
                "文件名在启用时展开一次；同名文件会追加写入；修改目录或格式在下次启用时生效。",
            ));
        });
    if !open || close_requested {
        app.show_log_file = false;
    }
}

/// "Restore defaults" is destructive and irreversible, so the menu item only
/// asks; the reset itself happens after an explicit confirmation here.
pub(crate) fn reset_confirm_window(app: &mut XcomApp, ctx: &Context) {
    if !app.show_reset_confirm {
        return;
    }
    let mut open = true;
    let mut close_requested = false;
    egui::Window::new(tr("恢复默认设置"))
        .id(egui::Id::new("reset_confirm_dialog"))
        .title_bar(false)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let header = dialog::header(ui, tr("恢复默认设置"), None);
            if header.close_clicked {
                close_requested = true;
            }
            ui.separator();
            ui.label(tr(
                "将清除全部设置（串口参数、界面布局、主题与语言等），此操作无法撤销。",
            ));
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let confirm = egui::Button::new(
                    RichText::new(tr("确认恢复")).color(level_color(Level::Warning)),
                );
                if ui.add(confirm).clicked() {
                    app.apply_default_settings();
                    close_requested = true;
                }
                if ui.button(tr("取消")).clicked() {
                    close_requested = true;
                }
            });
        });
    if !open || close_requested {
        app.show_reset_confirm = false;
    }
}

/// The keyboard map for every shortcut the app handles, so users never have to
/// discover them by reading the source.
pub(crate) fn shortcuts_window(app: &mut XcomApp, ctx: &Context) {
    if !app.show_shortcuts {
        return;
    }
    let mut open = true;
    let mut close_requested = false;
    egui::Window::new(tr("快捷键"))
        .id(egui::Id::new("shortcuts_dialog"))
        .title_bar(false)
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            let header = dialog::header(ui, tr("快捷键"), None);
            if header.close_clicked {
                close_requested = true;
            }
            ui.separator();
            egui::Grid::new("shortcuts_grid")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    let rows = [
                        ("Ctrl+,", tr("显示设置")),
                        ("Ctrl+O", tr("打开/关闭串口")),
                        ("Ctrl+Enter", tr("发送")),
                        ("Ctrl+S", tr("保存接收数据")),
                        ("Ctrl+Shift+S", tr("保存原始数据")),
                        ("Ctrl+C", tr("复制选中")),
                        ("Ctrl+A", tr("全选")),
                        ("F5", tr("刷新串口列表")),
                        ("Ctrl+Q", tr("退出")),
                    ];
                    for (keys, what) in rows {
                        ui.label(RichText::new(keys).monospace());
                        ui.label(what);
                        ui.end_row();
                    }
                });
        });
    if !open || close_requested {
        app.show_shortcuts = false;
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
pub(crate) fn save_raw_log(app: &mut XcomApp) {
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
pub(crate) fn save_log(app: &mut XcomApp) {
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
    use super::{
        MULTI_COLUMN_GAP, MULTI_FIXED_AT, MULTI_FIXED_COLUMNS, MULTI_TEXT_MIN, multi_column_floors,
        multi_column_widths, multi_page_size, multi_row_floor,
    };

    /// What the headers measure at the default interface font, in 中文.
    const ZH_HEADERS: [f32; 8] = [26.8, 7.0, 26.8, 26.8, 24.1, 26.8, 26.8, 0.0];
    /// The same in English, which is a great deal wider.
    const EN_HEADERS: [f32; 8] = [39.0, 7.0, 49.0, 30.0, 24.1, 47.0, 34.0, 0.0];

    /// Everything a row lays out: the eight columns and the seven gaps.
    fn row_width(widths: [f32; 8]) -> f32 {
        widths.iter().sum::<f32>() + MULTI_COLUMN_GAP * 7.0
    }

    #[test]
    fn the_columns_fill_the_panel_exactly() {
        // The whole point: a row one point wider than the table grows the
        // horizontal scroll bar that hides the send buttons.
        for headers in [ZH_HEADERS, EN_HEADERS] {
            for viewport in [363.0f32, 420.0, 560.0, 700.0, 1200.0] {
                let (widths, overflows) = multi_column_widths(viewport, MULTI_COLUMN_GAP, headers);
                assert!(!overflows, "{viewport}: the table should still fit");
                let used = row_width(widths);
                assert!(
                    (used - viewport).abs() <= 0.5,
                    "{viewport}: the row is {used} wide, not {viewport}"
                );
            }
        }
    }

    /// A header wider than its column widens the whole row, so every column
    /// has to hold its own header whatever the language.
    #[test]
    fn every_column_holds_its_header() {
        for headers in [ZH_HEADERS, EN_HEADERS] {
            let (widths, _) = multi_column_widths(363.0, MULTI_COLUMN_GAP, headers);
            for index in 0..8 {
                assert!(
                    widths[index] + 1.0 >= headers[index],
                    "column {index} is {} wide, its header needs {}",
                    widths[index],
                    headers[index]
                );
            }
        }
    }

    /// The English headers are wider, so they eat into the text columns rather
    /// than pushing the row past the panel.
    #[test]
    fn the_headers_come_out_of_the_text_columns() {
        let (zh, _) = multi_column_widths(363.0, MULTI_COLUMN_GAP, ZH_HEADERS);
        let (en, _) = multi_column_widths(363.0, MULTI_COLUMN_GAP, EN_HEADERS);
        assert!(
            en[2] < zh[2],
            "the English 内容 column should be narrower: {en:?} vs {zh:?}"
        );
        assert_eq!(row_width(en), row_width(zh), "both rows fill the panel");
    }

    #[test]
    fn the_text_columns_grow_with_the_panel() {
        let (narrow, _) = multi_column_widths(340.0, MULTI_COLUMN_GAP, ZH_HEADERS);
        let (wide, _) = multi_column_widths(700.0, MULTI_COLUMN_GAP, ZH_HEADERS);
        assert!(wide[2] > narrow[2], "narrow {narrow:?} wide {wide:?}");
        assert!(wide[3] > narrow[3], "narrow {narrow:?} wide {wide:?}");
    }

    #[test]
    fn the_fixed_columns_stay_between_their_floors_and_their_widths() {
        for viewport in [260.0f32, 280.0, 300.0, 340.0, 500.0, 900.0] {
            let (widths, _) = multi_column_widths(viewport, MULTI_COLUMN_GAP, ZH_HEADERS);
            for (slot, index) in MULTI_FIXED_AT.into_iter().enumerate() {
                let (ideal, floor) = MULTI_FIXED_COLUMNS[slot];
                let floor = floor.max(ZH_HEADERS[index].ceil() + 1.0);
                let ideal = ideal.max(floor);
                let width = widths[index];
                assert!(
                    width <= ideal + 0.01 && width >= floor - 0.01,
                    "{viewport}: column {index} is {width}, wanted {floor}..={ideal}"
                );
            }
        }
    }

    /// The whole reason the panel has a minimum: the narrowest it can be is the
    /// narrowest the table can be, so a panel at its floor shows every column.
    #[test]
    fn the_panel_floor_is_the_tables_floor() {
        for headers in [ZH_HEADERS, EN_HEADERS] {
            let floor = multi_row_floor(headers);
            let (widths, overflows) = multi_column_widths(floor, MULTI_COLUMN_GAP, headers);
            assert!(
                !overflows,
                "a panel at the table's own floor must still fit it: {widths:?}"
            );
            assert!(
                (row_width(widths) - floor).abs() <= 0.5,
                "the row is {} wide where the floor is {floor}",
                row_width(widths)
            );
        }
    }

    /// A column is never narrower than its header.
    #[test]
    fn the_floor_holds_every_header() {
        for headers in [ZH_HEADERS, EN_HEADERS] {
            let floors = multi_column_floors(headers);
            for index in 0..8 {
                assert!(
                    floors[index] >= headers[index].ceil(),
                    "column {index} floor {} is below its header {}",
                    floors[index],
                    headers[index]
                );
            }
        }
    }

    /// Squeezing gives up no more than the room the fixed columns have to give,
    /// and a table that cannot fit says so rather than piling columns on top of
    /// each other.
    #[test]
    fn a_table_that_cannot_fit_asks_for_a_scroll_bar() {
        let (widths, overflows) = multi_column_widths(120.0, MULTI_COLUMN_GAP, ZH_HEADERS);
        assert!(overflows);
        assert!(widths[2] >= MULTI_TEXT_MIN && widths[3] >= MULTI_TEXT_MIN);
        assert!(
            row_width(widths) > 120.0,
            "a table that cannot fit must be allowed to overflow: {widths:?}"
        );
    }

    #[test]
    fn nothing_goes_negative_when_the_panel_is_tiny() {
        for viewport in [0.0f32, 40.0, 120.0, 200.0] {
            let (widths, _) = multi_column_widths(viewport, MULTI_COLUMN_GAP, ZH_HEADERS);
            assert!(
                widths.iter().all(|width| *width >= 0.0),
                "{viewport}: {widths:?}"
            );
        }
    }
    #[test]
    fn page_size_fits_header_and_all_rows_without_rounding_up() {
        let row = 24.0;
        let gap = 4.0;
        // Header + three rows + three gaps fits exactly; a fourth row does not.
        assert_eq!(multi_page_size(row + 3.0 * (row + gap), row, gap), 3);
        assert_eq!(multi_page_size(row + 3.0 * (row + gap) - 0.1, row, gap), 2);
    }

    #[test]
    fn page_size_never_becomes_zero_for_tiny_or_invalid_viewports() {
        assert_eq!(multi_page_size(0.0, 24.0, 4.0), 1);
        assert_eq!(multi_page_size(f32::NAN, 24.0, 4.0), 1);
        assert_eq!(multi_page_size(100.0, f32::INFINITY, 4.0), 1);
    }

    #[test]
    fn page_size_follows_the_viewport_with_no_hard_cap() {
        // No fixed ceiling: a viewport tall enough for forty rows reports
        // forty, so a tall window never wastes vertical space. A window tall
        // enough for the whole pool reports all one hundred at once.
        assert_eq!(multi_page_size(24.0 + 40.0 * 28.0, 24.0, 4.0), 40);
        assert_eq!(multi_page_size(24.0 + 100.0 * 28.0, 24.0, 4.0), 100);
    }
}
