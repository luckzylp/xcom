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
        static SERIAL_COMBO_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static SEND_TOOLBAR_RECTS: RefCell<Vec<(&'static str, egui::Rect)>> =
            const { RefCell::new(Vec::new()) };
        static MULTI_HEADER_RECTS: RefCell<Vec<egui::Rect>> = const { RefCell::new(Vec::new()) };
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
    LeftWidth,
    RightWidth,
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
    /// The panel's size before this frame, used as the drag origin.
    pub current_size: f32,
    pub min: f32,
    pub max: f32,
    pub target: SplitterTarget,
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
        const GRAB: f32 = 6.0;
        let rect = spec.growth.band(spec.boundary, spec.cross, GRAB);
        let response = ui.interact(
            rect,
            egui::Id::new(("splitter", spec.id)),
            egui::Sense::click_and_drag(),
        );
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(spec.growth.cursor());
        }

        // The size is tracked as a delta from where the drag started, not as an
        // absolute distance from a fixed edge.  The edge moves as the panel
        // resizes, so an absolute reading is recomputed differently on every
        // frame of the gesture -- the panel overshoots and then creeps back.
        let state_id = egui::Id::new(("splitter-drag", spec.id));
        let current = spec.current_size;

        // Record the origin when the button goes down, not when egui decides a
        // drag has begun: the threshold means those are different frames, and
        // using the later one would make the first delta zero.
        if response.is_pointer_button_down_on()
            && ui.data(|data| data.get_temp::<(f32, egui::Pos2)>(state_id).is_none())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            ui.data_mut(|data| data.insert_temp(state_id, (current, pointer)));
        }

        if response.dragged() {
            let origin = ui.data(|data| data.get_temp::<(f32, egui::Pos2)>(state_id));
            if let (Some((start_size, start_pointer)), Some(pointer)) =
                (origin, response.interact_pointer_pos())
            {
                let size = (start_size + spec.growth.growth_delta(pointer, start_pointer))
                    .clamp(spec.min, spec.max);
                match spec.target {
                    SplitterTarget::LeftWidth => app.settings.left_panel_width = size,
                    SplitterTarget::RightWidth => app.settings.right_panel_width = size,
                    SplitterTarget::SendHeight => app.settings.send_area_height = size,
                }
                app.mark_settings_dirty();
            }
        }

        if response.drag_stopped() || !response.is_pointer_button_down_on() {
            ui.data_mut(|data| data.remove::<(f32, egui::Pos2)>(state_id));
        }
    }
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
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
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
        });
    changed
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
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("主题"));
            let mut theme_mode = app.settings.theme;
            if combo(ui, "theme_mode", &mut theme_mode, &ThemeMode::ALL, |m| {
                format!("{} {}", m.icon(), tr(m.label()))
            }) {
                app.settings.theme = theme_mode;
                app.mark_settings_dirty();
            }

            ui.separator();
            ui.label(tr("语言"));
            let mut language = i18n::lang();
            egui::ComboBox::from_id_salt("language")
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

            ui.separator();
            ui.checkbox(&mut app.show_side_panel, tr("显示工具面板"));

            ui.separator();
            let mut log_colors = app.settings.log_colors;
            if ui
                .checkbox(&mut log_colors.enabled, tr("自定义接收区配色"))
                .changed()
            {
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
            if ui.button(tr("恢复默认设置")).clicked() {
                app.settings = crate::settings::Settings::default();
                app.log
                    .set_limits(app.settings.max_rows, app.settings.max_raw);
                app.log.set_options(app.settings.display);
                app.mark_settings_dirty();
                app.info(tr("已恢复默认设置"));
            }
            if ui.button(tr("关于")).clicked() {
                app.show_about = true;
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Left configuration panel
// ---------------------------------------------------------------------------

pub(crate) fn left_panel(app: &mut XcomApp, ui: &mut Ui) {
    let width = app.settings.left_panel_width;
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
        current_size: app.settings.left_panel_width,
        min: LEFT_PANEL_MIN,
        max: LEFT_PANEL_MAX,
        target: SplitterTarget::LeftWidth,
    });
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
                        let manage = row_ui
                            .add_sized(
                                [manager_width, baud_combo_rect.height()],
                                egui::Button::new("+"),
                            )
                            .on_hover_text(tr("管理自定义波特率"));
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
    let popup = egui::Window::new(tr("自定义波特率"))
        .id(manager_id)
        .default_width(300.0)
        .collapsible(false)
        .resizable(false)
        .open(&mut manager_open)
        .show(&ctx, |ui| {
            let can_add = app.settings.serial.can_add_custom_baud(pending_baud);
            ui.horizontal(|ui| {
                let input = ui.add_sized(
                    [176.0, 26.0],
                    egui::DragValue::new(&mut pending_baud)
                        .speed(100.0)
                        .range(CUSTOM_BAUD_RANGE.0..=CUSTOM_BAUD_RANGE.1),
                );
                probe::set_custom_baud_input_rect(input.rect);
                let add = ui.add_enabled(can_add, egui::Button::new(tr("添加")));
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
            ui.separator();
            ui.label(tr("已保存的自定义波特率"));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    let rates = app.settings.serial.custom_bauds.clone();
                    if rates.is_empty() {
                        ui.weak(tr("暂无自定义波特率"));
                    }
                    for rate in rates {
                        ui.horizontal(|ui| {
                            ui.label(rate.to_string());
                            ui.weak("bps");
                            let delete = ui
                                .add_sized([26.0, 24.0], egui::Button::new("−"))
                                .on_hover_text(tr("删除"));
                            probe::push_custom_baud_delete_rect(rate, delete.rect);
                            if delete.clicked() {
                                remove_baud = Some(rate);
                            }
                        });
                    }
                });
        });
    if let Some(popup) = popup {
        probe::set_custom_baud_window_rect(popup.response.rect);
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

fn send_actions(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
    ui.spacing_mut().interact_size.y = btn_height;

    // Rightmost: 发送 (drawn first in right_to_left)
    let send_btn = ui.add_sized(
        [96.0, btn_height],
        egui::Button::new(RichText::new(tr("发送 (Ctrl+Enter)")).strong()),
    );
    probe::push_send_toolbar_rect("send", send_btn.rect);
    if send_btn.clicked() {
        app.send_once();
    }

    // 发送文件 / 终止发送
    let file_button = if app.file_send.is_some() {
        ui.add_sized([76.0, btn_height], egui::Button::new(tr("终止发送")))
    } else {
        ui.add_sized([76.0, btn_height], egui::Button::new(tr("发送文件")))
    };
    probe::push_send_toolbar_rect("file", file_button.rect);
    if file_button.clicked() {
        if app.file_send.is_some() {
            app.abort_file_send();
        } else if let Some(path) = pick_open_file(tr("选择要发送的文件")) {
            app.start_file_send(path);
        }
    }

    // 清除发送
    let clear_button = ui.add_sized([76.0, btn_height], egui::Button::new(tr("清除发送")));
    probe::push_send_toolbar_rect("clear", clear_button.rect);
    if clear_button.clicked() {
        app.settings.send.buffer.clear();
        app.tx_bytes = 0;
        app.rx_bytes = 0;
        app.mark_settings_dirty();
        app.info(tr("已清除发送区与收发计数"));
    }

    // 历史 (leftmost of the button group)
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

fn right_aligned_send_actions(app: &mut XcomApp, ui: &mut Ui, btn_height: f32) {
    let avail_w = (ui.available_width() - SEND_RIGHT_PAD).max(0.0);
    ui.allocate_ui_with_layout(
        egui::vec2(avail_w, btn_height),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| send_actions(app, ui, btn_height),
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
            right_aligned_send_actions(app, ui, btn_height);
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

pub(crate) fn side_panel(app: &mut XcomApp, ui: &mut Ui) {
    let width = app.settings.right_panel_width;
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
        current_size: app.settings.right_panel_width,
        min: RIGHT_PANEL_MIN,
        max: RIGHT_PANEL_MAX,
        target: SplitterTarget::RightWidth,
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

fn multi_panel(app: &mut XcomApp, ui: &mut Ui) {
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
                app.stop_multi_run(tr("已停止循环发送"));
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
                let stripe = index % 2 == 1;
                ui.horizontal(|ui| {
                    if stripe {
                        let row = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), row_height),
                        );
                        ui.painter().rect_filled(
                            row,
                            egui::CornerRadius::ZERO,
                            ui.visuals().faint_bg_color,
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
    egui::Window::new(tr("关于 XCOM"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
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
    if !open {
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
