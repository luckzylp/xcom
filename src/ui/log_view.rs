//! The receive view: a virtualized, selectable log.
//!
//! Only the rows intersecting the viewport are laid out and painted, so the
//! view costs O(visible) regardless of how much data has scrolled past.  Text
//! selection is implemented on top of egui's galley hit-testing, which gives
//! correct behaviour for wide (CJK) glyphs without any手工 column math.

use std::sync::Arc;

use eframe::egui::text::CCursor;
use eframe::egui::{self, Color32, FontId, Galley, Pos2, Rect, ScrollArea, Sense, Ui, Vec2};

use crate::logstore::{LogStore, RowKind};
use crate::theme::LogPalette;

/// A caret position: a row id plus a character index within that row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Caret {
    /// Stable row id, so the caret survives scrollback trimming.
    row: u64,
    col: usize,
}

/// An ordered text selection.
#[derive(Clone, Copy, Debug)]
struct Selection {
    anchor: Caret,
    head: Caret,
}

impl Selection {
    /// The selection ordered so `start <= end`.
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

/// Persistent view state (selection and drag anchor).
#[derive(Default)]
pub struct LogViewState {
    selection: Option<Selection>,
    /// Caret captured when a drag started.
    drag_anchor: Option<Caret>,
}

impl LogViewState {
    /// Drop the current selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.drag_anchor = None;
    }

    /// Whether anything is selected.
    pub fn has_selection(&self) -> bool {
        self.selection.is_some_and(|s| !s.is_empty())
    }

    /// Select every retained row.
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
            col: store.row(last_index).chars().count(),
        };
        self.selection = Some(Selection {
            anchor: first,
            head: last,
        });
    }

    /// The selected text, if any.
    pub fn selected_text(&self, store: &LogStore) -> Option<String> {
        let selection = self.selection?;
        if selection.is_empty() || store.row_count() == 0 {
            return None;
        }
        let (start, end) = selection.ordered();
        let first_id = store.first_row_id();
        let last_id = store.row_id(store.row_count() - 1);

        // Clamp into the rows that are still retained.
        let start_index = start.row.max(first_id).saturating_sub(first_id) as usize;
        let end_index = (end.row.min(last_id).saturating_sub(first_id)) as usize;
        if start_index > end_index || start_index >= store.row_count() {
            return None;
        }

        let mut out = String::new();
        for index in start_index..=end_index {
            let row = store.row(index);
            let chars = row.chars().count();
            let from = if index == start_index {
                start.col.min(chars)
            } else {
                0
            };
            let to = if index == end_index {
                end.col.min(chars)
            } else {
                chars
            };
            if to > from {
                out.extend(row.chars().skip(from).take(to - from));
            }
            if index != end_index {
                out.push('\n');
            }
        }
        if out.is_empty() { None } else { Some(out) }
    }
}

/// How the view should behave this frame.
pub struct LogViewOptions {
    pub auto_scroll: bool,
    pub font: FontId,
    pub line_spacing: f32,
}

/// Actions the surrounding UI should perform, requested from the view.
#[derive(Default)]
pub struct LogViewOutput {
    /// The selection was copied to the clipboard by the view itself.
    pub copied: bool,
    /// A selection exists and could be copied.
    pub has_selection: bool,
}

/// Everything needed to hit-test and paint one visible row.
struct VisibleRow {
    index: usize,
    /// Stable id, used by carets so they survive trimming.
    row_id: u64,
    galley: Arc<Galley>,
    origin: Pos2,
    rect: Rect,
    /// Colour this row is drawn in, which the galley bakes in.
    color: Color32,
}

/// Draw the log and handle interaction.
pub fn show(
    ui: &mut Ui,
    store: &LogStore,
    palette: LogPalette,
    state: &mut LogViewState,
    options: &LogViewOptions,
) -> LogViewOutput {
    let font = options.font.clone();
    let row_height = ui.ctx().fonts_mut(|f| f.row_height(&font)) + options.line_spacing;
    let char_width = ui.ctx().fonts_mut(|f| f.glyph_width(&font, 'M'));
    let total_rows = store.row_count();

    let mut output = LogViewOutput::default();

    if total_rows == 0 {
        // Still draw the (empty) background so the theme colour shows through.
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size_before_wrap(), Sense::click_and_drag());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, palette.background);
        let _ = response;
        return output;
    }

    let scroll = ScrollArea::both()
        .auto_shrink([false, false])
        .stick_to_bottom(options.auto_scroll && !state.has_selection());

    let clip_rect = ui.clip_rect();
    let result = scroll.show(ui, |ui| {
        let content_width =
            (store.max_columns() as f32 * char_width + 12.0).max(ui.available_width());
        let content_height = total_rows as f32 * row_height;
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(content_width, content_height),
            Sense::click_and_drag(),
        );

        // Only these rows are laid out this frame.
        let viewport = clip_rect.intersect(rect);
        let first = if viewport.height() <= 0.0 {
            0
        } else {
            (((viewport.top() - rect.top()) / row_height)
                .floor()
                .max(0.0) as usize)
                .min(total_rows)
        };
        let last = if viewport.height() <= 0.0 {
            0
        } else {
            ((((viewport.bottom() - rect.top()) / row_height).ceil() as usize) + 1).min(total_rows)
        };

        let mut visible: Vec<VisibleRow> = Vec::with_capacity(last.saturating_sub(first));
        for index in first..last {
            let text = store.row(index).to_owned();
            // Echoed transmissions are tinted so they are not mistaken for
            // data from the device.
            let color = match store.row_kind(index) {
                RowKind::Received => palette.foreground,
                RowKind::Sent => palette.sent,
            };
            let galley = ui
                .ctx()
                .fonts_mut(|f| f.layout_no_wrap(text, font.clone(), color));
            let origin = Pos2::new(rect.left(), rect.top() + index as f32 * row_height);
            let row_rect = Rect::from_min_size(
                origin,
                Vec2::new(content_width.max(rect.width()), row_height),
            );
            visible.push(VisibleRow {
                index,
                row_id: store.row_id(index),
                galley,
                origin,
                rect: row_rect,
                color,
            });
        }

        // ---- interaction (before painting so the highlight is current) ----
        let pointer = response.interact_pointer_pos();
        if response.drag_started() {
            if let Some(pos) = pointer {
                if let Some(caret) = hit_test(&visible, rect, row_height, pos) {
                    state.drag_anchor = Some(caret);
                    state.selection = Some(Selection {
                        anchor: caret,
                        head: caret,
                    });
                }
            }
        } else if response.dragged() {
            if let (Some(pos), Some(anchor)) = (pointer, state.drag_anchor) {
                if let Some(caret) = hit_test_clamped(&visible, rect, row_height, pos) {
                    state.selection = Some(Selection {
                        anchor,
                        head: caret,
                    });
                }
            }
        } else if response.clicked() {
            // A plain click collapses the selection to a caret.
            if let Some(pos) = pointer {
                if let Some(caret) = hit_test(&visible, rect, row_height, pos) {
                    state.selection = Some(Selection {
                        anchor: caret,
                        head: caret,
                    });
                }
            }
            state.drag_anchor = None;
        } else if response.drag_stopped() {
            state.drag_anchor = None;
        }

        // ---- painting ------------------------------------------------------
        let painter = ui.painter_at(viewport);
        painter.rect_filled(viewport, egui::CornerRadius::ZERO, palette.background);

        let selection = state.selection.filter(|s| !s.is_empty());
        for row in &visible {
            if let Some(sel) = selection {
                paint_selection(&painter, row, sel, palette);
            }
            painter.galley(row.origin, row.galley.clone(), row.color);
        }

        output.has_selection = state.has_selection();
    });

    // Clipboard copy is handled here so the shortcut works while the log has
    // focus, regardless of which panel is active.
    let copy_pressed =
        ui.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.command && !i.modifiers.shift);
    if copy_pressed && state.has_selection() {
        if let Some(text) = state.selected_text(store) {
            ui.ctx().copy_text(text);
            output.copied = true;
        }
    }

    let select_all = ui.input(|i| i.key_pressed(egui::Key::A) && i.modifiers.command);
    if select_all {
        state.select_all(store);
    }

    let _ = result;
    output
}

/// Paint the highlight rectangles for one row.
fn paint_selection(
    painter: &egui::Painter,
    row: &VisibleRow,
    selection: Selection,
    palette: LogPalette,
) {
    let row_id = row.row_id;
    let (start, end) = selection.ordered();
    if row_id < start.row || row_id > end.row {
        return;
    }
    let chars = row.galley.text().chars().count();
    let from = if row_id == start.row {
        start.col.min(chars)
    } else {
        0
    };
    let to = if row_id == end.row {
        end.col.min(chars)
    } else {
        chars
    };

    // A selection that covers the newline also needs a marker at the row end.
    let (x0, x1) = if to > from {
        let a = row.galley.pos_from_cursor(CCursor::new(from));
        let b = row.galley.pos_from_cursor(CCursor::new(to));
        (a.left(), b.left().max(b.right()))
    } else if row_id == start.row && row_id == end.row {
        // Empty range on this row: draw a caret so a click is visible.
        let a = row.galley.pos_from_cursor(CCursor::new(from));
        (a.left(), a.left() + 1.0)
    } else {
        let end_of_row = row.galley.pos_from_cursor(CCursor::new(chars));
        (end_of_row.right(), end_of_row.right() + 4.0)
    };

    let rect = Rect::from_min_max(
        Pos2::new(row.origin.x + x0, row.rect.top()),
        Pos2::new(row.origin.x + x1, row.rect.bottom()),
    );
    painter.rect_filled(rect, egui::CornerRadius::ZERO, palette.selection);
}

/// Map a pointer position to a caret within the visible rows.
fn hit_test(visible: &[VisibleRow], content: Rect, row_height: f32, pos: Pos2) -> Option<Caret> {
    if visible.is_empty() {
        return None;
    }
    let offset = ((pos.y - content.top()) / row_height).floor();
    let index = offset.max(0.0) as usize;
    // Clamp into the rendered window rather than the whole log.
    let index = index.clamp(visible[0].index, visible[visible.len() - 1].index);
    caret_in_row(visible, index, pos)
}

/// Like [`hit_test`] but tolerant of a pointer dragged outside the rows, so a
/// drag can extend the selection past the viewport edge.
fn hit_test_clamped(
    visible: &[VisibleRow],
    content: Rect,
    row_height: f32,
    pos: Pos2,
) -> Option<Caret> {
    if visible.is_empty() {
        return None;
    }
    let clamped_y = pos.y.clamp(
        content.top(),
        content.top() + (visible[visible.len() - 1].index as f32 + 1.0) * row_height - 1.0,
    );
    let clamped = Pos2::new(pos.x, clamped_y);
    hit_test(visible, content, row_height, clamped)
}

/// Resolve a caret inside a specific row, given an absolute pointer position.
fn caret_in_row(visible: &[VisibleRow], index: usize, pos: Pos2) -> Option<Caret> {
    let row = visible.iter().find(|r| r.index == index)?;
    let local = Vec2::new(pos.x - row.origin.x, pos.y - row.origin.y);
    let cursor = row.galley.cursor_from_pos(local);
    Some(Caret {
        row: row.row_id,
        col: cursor.index.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::EncodingId;
    use encoding_rs::UTF_8;

    fn store_with(lines: &[&str]) -> LogStore {
        let mut store = LogStore::new(UTF_8);
        for line in lines {
            store.append(format!("{line}\r\n").as_bytes(), 0);
        }
        store
    }

    #[test]
    fn selection_of_a_single_row() {
        let store = store_with(&["hello", "world"]);
        let state = LogViewState {
            selection: Some(Selection {
                anchor: Caret {
                    row: store.row_id(0),
                    col: 1,
                },
                head: Caret {
                    row: store.row_id(0),
                    col: 4,
                },
            }),
            ..Default::default()
        };
        assert_eq!(state.selected_text(&store).unwrap(), "ell");
    }

    #[test]
    fn selection_across_rows_includes_newlines() {
        let store = store_with(&["abc", "def", "ghi"]);
        let state = LogViewState {
            selection: Some(Selection {
                anchor: Caret {
                    row: store.row_id(0),
                    col: 1,
                },
                head: Caret {
                    row: store.row_id(2),
                    col: 2,
                },
            }),
            ..Default::default()
        };
        assert_eq!(state.selected_text(&store).unwrap(), "bc\ndef\ngh");
    }

    #[test]
    fn reversed_selection_reads_in_document_order() {
        let store = store_with(&["abc", "def"]);
        let state = LogViewState {
            selection: Some(Selection {
                anchor: Caret {
                    row: store.row_id(1),
                    col: 3,
                },
                head: Caret {
                    row: store.row_id(0),
                    col: 0,
                },
            }),
            ..Default::default()
        };
        assert_eq!(state.selected_text(&store).unwrap(), "abc\ndef");
    }

    #[test]
    fn empty_selection_yields_nothing() {
        let store = store_with(&["abc"]);
        let state = LogViewState {
            selection: Some(Selection {
                anchor: Caret {
                    row: store.row_id(0),
                    col: 1,
                },
                head: Caret {
                    row: store.row_id(0),
                    col: 1,
                },
            }),
            ..Default::default()
        };
        assert!(state.selected_text(&store).is_none());
        assert!(!state.has_selection());
    }

    #[test]
    fn select_all_covers_every_row() {
        let store = store_with(&["one", "two", "three"]);
        let mut state = LogViewState::default();
        state.select_all(&store);
        assert_eq!(state.selected_text(&store).unwrap(), "one\ntwo\nthree");
    }

    #[test]
    fn select_all_on_empty_log_is_harmless() {
        let store = LogStore::new(UTF_8);
        let mut state = LogViewState::default();
        state.select_all(&store);
        assert!(state.selected_text(&store).is_none());
    }

    #[test]
    fn selection_survives_scrollback_trimming() {
        let mut store = LogStore::new(UTF_8);
        store.set_limits(1024, 64 * 1024);
        for i in 0..3000 {
            store.append(format!("line{i}\r\n").as_bytes(), 0);
        }
        let mut state = LogViewState::default();
        state.select_all(&store);
        // Every retained row must be selected; nothing may panic on the ids
        // that have already been trimmed away.
        let text = state.selected_text(&store).unwrap();
        assert!(text.starts_with("line"));
        assert_eq!(text.lines().count(), store.row_count());
    }

    #[test]
    fn select_all_ignores_rows_trimmed_since_selection() {
        let mut store = LogStore::new(UTF_8);
        store.set_limits(1024, 64 * 1024);
        for i in 0..3000 {
            store.append(format!("line{i}\r\n").as_bytes(), 0);
        }
        let mut state = LogViewState::default();
        state.select_all(&store);
        // Trim further and re-read: the stale anchor must clamp, not panic.
        for i in 0..2000 {
            store.append(format!("more{i}\r\n").as_bytes(), 0);
        }
        let text = state.selected_text(&store).unwrap();
        assert!(!text.is_empty());
    }

    #[test]
    fn unicode_columns_are_counted_in_chars_not_bytes() {
        let store = store_with(&["中文abc"]);
        let state = LogViewState {
            selection: Some(Selection {
                anchor: Caret {
                    row: store.row_id(0),
                    col: 0,
                },
                head: Caret {
                    row: store.row_id(0),
                    col: 3,
                },
            }),
            ..Default::default()
        };
        assert_eq!(state.selected_text(&store).unwrap(), "中文a");
    }

    #[test]
    fn log_store_reports_widest_row() {
        let store = store_with(&["ab", "abcdef", "abc"]);
        assert_eq!(store.max_columns(), 6);
        let _ = EncodingId::Utf8;
    }
}
