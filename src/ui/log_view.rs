//! The receive view: a virtualized, selectable log.
//!
//! Only the rows intersecting the viewport are shaped and painted, so the view
//! costs O(visible) regardless of how much data has scrolled past.  The shaped
//! line for each visible row is kept for the current frame so pointer hit
//! testing uses the real glyph positions, including wide CJK glyphs and any
//! proportional font the user selected.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::{
    AnyElement, Bounds, ClipboardItem, Context, Font, Hsla, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Pixels, Point, ShapedLine, SharedString,
    StatefulInteractiveElement as _, Styled as _, TextAlign, TextRun, Window, canvas, div, fill,
    point, px, size,
};

use crate::app::{LOG_CONTEXT, XcomApp};
use crate::highlight::Highlighter;
use crate::i18n::tr;
use crate::logstore::{LogStore, RowKind, ViewMode};
use crate::theme::LogPalette;

/// A caret position: a row id plus a byte offset within that row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Caret {
    /// Stable row id, so the caret survives scrollback trimming.
    row: u64,
    /// UTF-8 byte offset inside the row.
    col: usize,
}

/// An ordered text selection.
#[derive(Clone, Copy, Debug)]
struct Selection {
    anchor: Caret,
    head: Caret,
}

impl Selection {
    fn ordered(&self) -> (Caret, Caret) {
        if (self.anchor.row, self.anchor.col) <= (self.head.row, self.head.col) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Persistent view state (selection, drag anchor and scroll position).
pub struct LogViewState {
    selection: Option<Selection>,
    drag_anchor: Option<Caret>,
    /// Whether the view followed the newest row on the previous frame.
    following: bool,
    /// Compiled keyword rules, rebuilt only when the settings change.
    highlighter: Highlighter,
    pub(crate) scroll: gpui_kit::ScrollHandle,
    /// Shaped lines for the rows painted this frame, keyed by stable row id.
    lines: HashMap<u64, ShapedLine>,
    row_height: Pixels,
    char_width: Pixels,
    /// Whether the last frame reached the bottom, used to detect a user scroll.
    at_bottom: bool,
    /// Set for the frame in which the view asked to scroll to the bottom.
    scrolling_to_bottom: bool,
}

impl Default for LogViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl LogViewState {
    pub fn new() -> Self {
        Self {
            selection: None,
            drag_anchor: None,
            following: true,
            highlighter: Highlighter::default(),
            scroll: gpui_kit::ScrollHandle::new(),
            lines: HashMap::new(),
            row_height: px(18.0),
            char_width: px(8.0),
            at_bottom: true,
            scrolling_to_bottom: false,
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.drag_anchor = None;
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some_and(|s| !s.is_empty())
    }

    pub fn select_all(&mut self, store: &LogStore) {
        if store.row_count() == 0 {
            return;
        }
        let first = Caret {
            row: store.row_id(0),
            col: 0,
        };
        let last_index = store.row_count() - 1;
        let last = Caret {
            row: store.row_id(last_index),
            col: store.row(last_index).len(),
        };
        self.selection = Some(Selection {
            anchor: first,
            head: last,
        });
    }

    pub fn selected_text(&self, store: &LogStore) -> Option<String> {
        let selection = self.selection?;
        if selection.is_empty() || store.row_count() == 0 {
            return None;
        }
        let (start, end) = selection.ordered();
        let first_id = store.first_row_id();
        let last_id = store.row_id(store.row_count() - 1);

        let start_index = start.row.max(first_id).saturating_sub(first_id) as usize;
        let end_index = (end.row.min(last_id).saturating_sub(first_id)) as usize;
        if start_index > end_index || start_index >= store.row_count() {
            return None;
        }

        let mut out = String::new();
        for index in start_index..=end_index {
            let row = store.row(index);
            let boundary = |value: usize| {
                let mut value = value.min(row.len());
                while !row.is_char_boundary(value) {
                    value -= 1;
                }
                value
            };
            let from = if index == start_index {
                boundary(start.col)
            } else {
                0
            };
            let to = if index == end_index {
                boundary(end.col)
            } else {
                row.len()
            };
            if to > from {
                out.push_str(&row[from..to]);
            }
            if index != end_index {
                out.push('\n');
            }
        }
        if out.is_empty() { None } else { Some(out) }
    }

    fn selection_for_row(&self, row_id: u64, len: usize) -> Option<(usize, usize)> {
        let selection = self.selection?;
        if selection.is_empty() {
            return None;
        }
        let (start, end) = selection.ordered();
        if row_id < start.row || row_id > end.row {
            return None;
        }
        let from = if row_id == start.row {
            start.col.min(len)
        } else {
            0
        };
        let to = if row_id == end.row {
            end.col.min(len)
        } else {
            len
        };
        Some((from, to))
    }

    fn set_caret(&mut self, caret: Caret, dragging: bool) {
        if dragging {
            self.selection = Some(Selection {
                anchor: self.drag_anchor.unwrap_or(caret),
                head: caret,
            });
        } else {
            self.selection = Some(Selection {
                anchor: caret,
                head: caret,
            });
        }
    }
}

/// Build the text runs for one row: marker, timestamp, then data with keyword
/// highlights applied only to the data part.
fn row_runs(
    text: &str,
    prefix: (usize, usize),
    kind: RowKind,
    font: &Font,
    color: Hsla,
    palette: LogPalette,
    highlights: &Highlighter,
) -> Vec<TextRun> {
    let clamp = |mut len: usize| {
        len = len.min(text.len());
        while !text.is_char_boundary(len) {
            len -= 1;
        }
        len
    };
    let mark_len = clamp(prefix.0);
    let stamp_end = clamp(prefix.0 + prefix.1);
    let marker_color = match kind {
        RowKind::Received => Hsla::from(palette.received_marker),
        RowKind::Sent => Hsla::from(palette.sent_marker),
    };
    let timestamp = Hsla::from(palette.timestamp);

    let mut runs = Vec::new();
    let mut push = |slice: &str, color: Hsla| {
        if !slice.is_empty() {
            runs.push(TextRun {
                len: slice.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
    };
    push(&text[..mark_len], marker_color);
    push(&text[mark_len..stamp_end], timestamp);

    let data = &text[stamp_end..];
    if highlights.is_active() {
        let mut last = 0;
        for found in highlights.matches(data) {
            if found.start > last {
                push(&data[last..found.start], color);
            }
            let rgb = highlights.color(found.rule);
            push(
                &data[found.start..found.end],
                Hsla::from(gpui_kit::rgb(
                    (rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32,
                )),
            );
            last = found.end;
        }
        if last < data.len() {
            push(&data[last..], color);
        }
    } else {
        push(data, color);
    }
    runs
}

impl XcomApp {
    /// Draw the receive log and handle its pointer and keyboard interaction.
    pub(crate) fn render_log(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let palette = crate::theme::log_palette(cx, self.settings.theme, self.settings.log_colors);
        let font = crate::fonts::receive_font(&self.settings.fonts, &cx.theme().mono_font_family);
        let font_size = px(self.settings.fonts.receive_size);
        let sample = window.text_system().shape_line(
            SharedString::from("M"),
            font_size,
            &[TextRun {
                len: 1,
                font: font.clone(),
                color: Hsla::from(palette.foreground),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let char_width = sample.width();
        let row_height = sample.ascent + sample.descent + px(self.settings.line_spacing);
        self.log_view.char_width = char_width;
        self.log_view.row_height = row_height;
        self.log_view.highlighter.sync(&self.settings.highlights);

        let viewport = self.log_view.scroll.bounds();
        if self.settings.display.wrap && char_width > px(0.0) && viewport.size.width > px(0.0) {
            let cols = (viewport.size.width / char_width).floor().max(8.0) as usize;
            if self.update_wrap_width(cols) {
                // One extra frame shows the re-wrapped rows; the next call
                // sees the same width and stops, so this cannot loop.
                cx.notify();
            }
        }

        let total_rows = self.log.row_count();
        let content_width = if self.settings.display.wrap {
            viewport.size.width.max(px(1.0))
        } else {
            (self.log.max_columns() as f32 * char_width + px(12.0)).max(viewport.size.width)
        };
        let content_height = total_rows as f32 * row_height;
        let offset = self.log_view.scroll.offset();
        let max_offset = self.log_view.scroll.max_offset();
        let at_bottom = max_offset.y <= px(0.0) || offset.y >= max_offset.y - px(2.0);

        let follow = self.settings.auto_scroll && !self.log_view.has_selection();
        if follow {
            if self.log_view.following || at_bottom || !self.log_view.scrolling_to_bottom {
                self.log_view.scroll.scroll_to_bottom();
                self.log_view.scrolling_to_bottom = true;
            } else if !at_bottom {
                // The user scrolled away from the end while following: stop
                // pulling them back until they return to the bottom.
                self.log_view.following = false;
            }
        } else {
            self.log_view.following = false;
            self.log_view.scrolling_to_bottom = false;
        }
        self.log_view.at_bottom = at_bottom;
        if at_bottom {
            self.log_view.following = true;
        }

        let first = ((offset.y.max(px(0.0)) / row_height).floor() as usize).min(total_rows);
        let last = (((offset.y.max(px(0.0)) + viewport.size.height) / row_height).ceil() as usize
            + 1)
        .min(total_rows);

        self.log_view.lines.clear();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(last.saturating_sub(first));
        for index in first..last {
            let text = SharedString::from(self.log.row(index));
            let row_id = self.log.row_id(index);
            let prefix = self.log.row_prefix(index);
            let kind = self.log.row_kind(index);
            let runs = row_runs(
                &text,
                prefix,
                kind,
                &font,
                Hsla::from(palette.foreground),
                palette,
                &self.log_view.highlighter,
            );
            let line = window
                .text_system()
                .shape_line(text.clone(), font_size, &runs, None);
            let selection = self.log_view.selection_for_row(row_id, text.len());
            self.log_view.lines.insert(row_id, line.clone());
            let line = Arc::new(line);
            let selection_color = Hsla::from(palette.selection);
            let row = div()
                .id(("log-row", row_id))
                .w(content_width)
                .h(row_height)
                .child(
                    canvas(
                        move |_bounds, _window, _cx| (),
                        move |bounds, _, window, _cx| {
                            if let Some((from, to)) = selection
                                && to > from
                            {
                                let x0 = line.x_for_index(from);
                                let x1 = if to >= line.len() {
                                    bounds.size.width
                                } else {
                                    line.x_for_index(to).max(line.x_for_index(from) + px(1.0))
                                };
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(bounds.left() + x0, bounds.top()),
                                        size(x1 - x0, bounds.size.height),
                                    ),
                                    selection_color,
                                ));
                            }
                            let _ = line.paint(
                                point(bounds.left(), bounds.top()),
                                bounds.size.height,
                                TextAlign::Left,
                                None,
                                window,
                                _cx,
                            );
                        },
                    )
                    .size_full(),
                );
            rows.push(row.into_any_element());
        }

        let weak = cx.entity().downgrade();
        let context_weak = weak.clone();
        let has_selection = self.log_view.has_selection();
        let selected_text = self.log_view.selected_text(&self.log);
        let paused = self.show_paused;
        let display = self.settings.display;
        let echo_sent = self.settings.echo_sent;
        let auto_scroll = self.settings.auto_scroll;

        let empty = total_rows == 0;
        let mut body = div()
            .id("log-scroll")
            .relative()
            .size_full()
            .overflow_scroll()
            .track_scroll(&self.log_view.scroll)
            .bg(palette.background)
            .text_color(palette.foreground)
            .track_focus(&self.log_focus)
            .key_context(LOG_CONTEXT)
            .on_action(cx.listener(|this, _: &crate::app::CopyLog, _, cx| {
                this.copy_log_selection(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::app::SelectAllLog, _, cx| {
                this.select_all_log();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                    this.log_focus.focus(window, cx);
                    if let Some(caret) = this.log_caret_at(event.position) {
                        this.log_view.drag_anchor = Some(caret);
                        this.log_view.set_caret(caret, false);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui_kit::MouseMoveEvent, _window, cx| {
                    if event.dragging()
                        && let Some(caret) = this.log_caret_at(event.position)
                    {
                        this.log_view.set_caret(caret, true);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &gpui_kit::MouseUpEvent, _window, cx| {
                    this.log_view.drag_anchor = None;
                    cx.notify();
                }),
            )
            .context_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                let copy = PopupMenuItem::new(tr("复制选中")).disabled(!has_selection);
                menu = menu.item(copy.on_click({
                    let weak = context_weak.clone();
                    let text = selected_text.clone();
                    move |_, _, cx| {
                        if let Some(text) = text.as_deref() {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
                        }
                        let _ = weak.update(cx, |this, cx| {
                            this.info(tr("已复制选中内容到剪贴板"));
                            cx.notify();
                        });
                    }
                }));
                menu = menu.item(PopupMenuItem::new(tr("全选")).on_click({
                    let weak = context_weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.select_all_log();
                            cx.notify();
                        });
                    }
                }));
                menu = menu.separator();
                menu = menu.item(
                    PopupMenuItem::new(if paused {
                        tr("继续显示")
                    } else {
                        tr("暂停显示")
                    })
                    .on_click({
                        let weak = context_weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if this.show_paused {
                                    this.resume_display();
                                } else {
                                    this.show_paused = true;
                                }
                                cx.notify();
                            });
                        }
                    }),
                );
                menu = menu.item(
                    PopupMenuItem::new(tr("16进制显示"))
                        .checked(display.mode == ViewMode::Hex)
                        .on_click({
                            let weak = context_weak.clone();
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    let mut options = this.settings.display;
                                    options.mode = if options.mode == ViewMode::Hex {
                                        ViewMode::Text
                                    } else {
                                        ViewMode::Hex
                                    };
                                    this.set_display_options(options);
                                    cx.notify();
                                });
                            }
                        }),
                );
                menu = menu.item(
                    PopupMenuItem::new(tr("回显发送"))
                        .checked(echo_sent)
                        .on_click({
                            let weak = context_weak.clone();
                            move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.settings.echo_sent = !this.settings.echo_sent;
                                    this.mark_settings_dirty();
                                    cx.notify();
                                });
                            }
                        }),
                );
                menu = menu.separator();
                for (label, checked, toggle) in [
                    (tr("方向标记"), display.markers, 0usize),
                    (tr("时间戳"), display.timestamp, 1),
                    (tr("自动换行"), display.wrap, 2),
                    (tr("自动滚动到最新"), auto_scroll, 3),
                ] {
                    menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click({
                        let weak = context_weak.clone();
                        move |_, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                match toggle {
                                    0 => {
                                        let mut options = this.settings.display;
                                        options.markers = !options.markers;
                                        this.set_display_options(options);
                                    }
                                    1 => {
                                        let mut options = this.settings.display;
                                        options.timestamp = !options.timestamp;
                                        this.set_display_options(options);
                                    }
                                    2 => {
                                        let mut options = this.settings.display;
                                        options.wrap = !options.wrap;
                                        this.set_display_options(options);
                                    }
                                    _ => {
                                        let value = !this.settings.auto_scroll;
                                        this.set_auto_scroll(value);
                                    }
                                }
                                cx.notify();
                            });
                        }
                    }));
                }
                menu = menu.separator();
                menu = menu.item(
                    PopupMenuItem::new(format!("{}…", tr("关键字高亮"))).on_click({
                        let weak = context_weak.clone();
                        move |_, window, cx| {
                            super::panels::run_action(
                                weak.clone(),
                                crate::app::Action::ShowHighlight,
                                window,
                                cx,
                            );
                        }
                    }),
                );
                menu = menu.separator();
                menu.item(PopupMenuItem::new(tr("清除接收")).on_click({
                    let weak = context_weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.clear_receive();
                            cx.notify();
                        });
                    }
                }))
            });

        if empty {
            body = body.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .text_color(Hsla::from(palette.timestamp))
                            .child(tr("暂无数据")),
                    ),
            );
        } else {
            let top_spacer = div().w(content_width).h(row_height * first as f32);
            let bottom_spacer = div()
                .w(content_width)
                .h(row_height * total_rows.saturating_sub(last) as f32);
            body = body.child(
                div()
                    .w(content_width)
                    .h(content_height)
                    .flex()
                    .flex_col()
                    .child(top_spacer)
                    .children(rows)
                    .child(bottom_spacer),
            );
        }
        if viewport.size.height <= px(0.0) {
            // The scroll area has no resolved bounds on its first frame; ask
            // for one more frame so the visible rows can be painted.
            cx.notify();
        }
        body.into_any_element()
    }

    /// Resolve a window position to a row/byte caret in the receive log.
    fn log_caret_at(&self, position: Point<Pixels>) -> Option<Caret> {
        let bounds = self.log_view.scroll.bounds();
        if bounds.size.height <= px(0.0) || self.log.row_count() == 0 {
            return None;
        }
        let offset = self.log_view.scroll.offset();
        let content_y = position.y - bounds.top() + offset.y;
        let row = (content_y / self.log_view.row_height).floor().max(0.0) as usize;
        let row = row.min(self.log.row_count() - 1);
        let row_id = self.log.row_id(row);
        let line = self.log_view.lines.get(&row_id);
        let col = match line {
            Some(line) => {
                let local_x = position.x - bounds.left() + offset.x;
                let index = line.closest_index_for_x(local_x.max(px(0.0)));
                let text = self.log.row(row);
                let mut index = index.min(text.len());
                while !text.is_char_boundary(index) {
                    index -= 1;
                }
                index
            }
            None => 0,
        };
        Some(Caret { row: row_id, col })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logstore::DisplayOptions;

    fn plain_log() -> LogStore {
        let mut log = LogStore::new(encoding_rs::UTF_8);
        log.set_options(DisplayOptions {
            markers: false,
            ..Default::default()
        });
        log
    }

    #[test]
    fn select_all_copies_every_row_in_order() {
        let mut log = plain_log();
        log.append(b"hello\nworld\n", 0);
        let mut view = LogViewState::new();
        assert!(view.selected_text(&log).is_none());
        view.select_all(&log);
        assert_eq!(view.selected_text(&log).as_deref(), Some("hello\nworld"));
    }

    #[test]
    fn selecting_a_partial_row_reads_only_that_range() {
        let mut log = plain_log();
        log.append(b"abcdef\n", 0);
        let mut view = LogViewState::new();
        view.selection = Some(Selection {
            anchor: Caret {
                row: log.row_id(0),
                col: 1,
            },
            head: Caret {
                row: log.row_id(0),
                col: 4,
            },
        });
        assert_eq!(view.selected_text(&log).as_deref(), Some("bcd"));
    }
}
