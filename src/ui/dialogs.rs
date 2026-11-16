//! Application dialogs: about, shortcuts, fonts, receive colours, keyword
//! highlighting, log files, custom baud rates and the reset confirmation.
//!
//! Every dialog is opened through the window's overlay layer, so it gets the
//! component system's focus trap, Escape dismissal and focus restoration.  The
//! controls inside read the retained application entity through a weak handle;
//! a change updates the settings and the dialog layer redraws.

use gpui_kit::AppContext as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{Dialog, DialogClose, DialogFooter},
    h_flex,
    input::Input,
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, DragMoveEvent, Empty, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, Point, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, WeakEntity, Window, div, point, px,
};

use crate::app::{Action, Choice, XcomApp};
use crate::i18n::tr;

fn footer() -> DialogFooter {
    DialogFooter::new()
        .justify_end()
        .gap_2()
        .child(DialogClose::new().trigger(|button| button.label(tr("关闭")).outline()))
}

/// The highlight table's columns, in row order.  The keyword column takes what
/// is left.  The colour column is wide enough for a full `#RRGGBB` value, and
/// the action column for the `.*` toggle plus the delete button.
const HL_INDEX_W: f32 = 30.0;
const HL_ENABLE_W: f32 = 34.0;
const HL_COLOR_W: f32 = 88.0;
const HL_ACTION_W: f32 = 60.0;
const HL_COL_GAP: f32 = 8.0;

/// The sizes these windows open at.  They are main's egui `default_size`
/// values, so the migrated windows keep the same footprint: the settings
/// windows are as wide as egui made them, and the two windows that hold a
/// table or a rate list keep egui's fixed height instead of shrinking onto
/// their content.
const FONT_WINDOW_W: f32 = 520.0;
const LOG_COLORS_WINDOW_W: f32 = 440.0;
const LOG_FILE_WINDOW_W: f32 = 480.0;
const HIGHLIGHT_WINDOW_W: f32 = 800.0;
const HIGHLIGHT_WINDOW_H: f32 = 600.0;
const CUSTOM_BAUD_WINDOW_W: f32 = 320.0;
const CUSTOM_BAUD_WINDOW_H: f32 = 340.0;

fn hl_header(text: impl Into<SharedString>, width: f32, cx: &gpui_kit::App) -> AnyElement {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_sm()
        .font_bold()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

fn hl_header_flex(text: impl Into<SharedString>, cx: &gpui_kit::App) -> AnyElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .text_sm()
        .font_bold()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_sm().child(SharedString::from(label)))
        .child(control)
}

/// A close control that dismisses the open dialog.
fn close_control() -> impl IntoElement {
    Button::new("dialog-close")
        .icon(gpui_kit::component::IconName::WindowClose)
        .ghost()
        .small()
        .tooltip(tr("关闭"))
        .on_click(|_, window, cx| window.close_all_dialogs(cx))
}

/// A dialog header that follows the egui windows: a small close control on
/// whichever side the desktop puts its window buttons (see ADR 0001), the
/// optional glyph, then the title.  The whole row drags the dialog, the way
/// an egui window's title bar moves the window.
///
/// gpui's `Dialog` always draws its close button on the right.  The egui
/// version followed the desktop's `button-layout` instead, so the built-in
/// button is turned off and this header draws its own.
fn header(
    weak: WeakEntity<XcomApp>,
    layout: gpui_kit::WindowButtonLayout,
    title: impl Into<SharedString>,
    icon: Option<gpui_kit::component::IconName>,
    cx: &gpui_kit::App,
) -> AnyElement {
    let close_on_left = layout
        .left
        .iter()
        .flatten()
        .any(|button| matches!(button, gpui_kit::WindowButton::Close));
    let mut row = h_flex()
        .id("dialog-header")
        .test_support()
        .w_full()
        .items_center()
        .gap_2()
        // Dragging starts like any gpui drag, after the pointer leaves the
        // click threshold, so pressing the close control still clicks it.
        .on_mouse_down(MouseButton::Left, {
            let weak = weak.clone();
            move |event, _, cx| {
                if let Some(entity) = weak.upgrade() {
                    entity.update(cx, |app, _| app.dialog_drag.begin(event.position));
                }
            }
        })
        .on_drag(DialogDragged, |_, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| DialogDragged)
        })
        .on_drag_move({
            let weak = weak.clone();
            move |event: &DragMoveEvent<DialogDragged>, window, cx| {
                if let Some(entity) = weak.upgrade() {
                    entity.update(cx, |app, cx| {
                        app.dialog_drag.drag(event, window);
                        cx.notify();
                    });
                }
            }
        });
    if close_on_left {
        row = row.child(close_control());
    }
    if let Some(icon) = icon {
        row = row.child(
            gpui_kit::component::Icon::new(icon)
                .size_4()
                .text_color(cx.theme().foreground),
        );
    }
    row = row.child(div().flex_1().child(title.into()));
    if !close_on_left {
        row = row.child(close_control());
    }
    row.into_any_element()
}

fn onoff(
    weak: WeakEntity<XcomApp>,
    checked: bool,
    label: &'static str,
    apply: fn(&mut XcomApp, bool),
) -> impl IntoElement {
    Switch::new(SharedString::from(label))
        .checked(checked)
        .label(label)
        .on_change(move |next, _, cx| {
            let next = *next;
            let _ = weak.update(cx, |this, cx| {
                apply(this, next);
                this.mark_settings_dirty();
                cx.notify();
            });
        })
}

/// Where a dragged dialog sits, and the drag in progress.
///
/// The dialog layer keeps its surfaces centred; a drag records how far the
/// user pulled the surface away from that resting place, and the surface is
/// shifted by that offset through margins.  The layer's popup stays where it
/// was, so focus trapping, the overlay and dismissal keep working unchanged.
#[derive(Default)]
pub(crate) struct DialogDrag {
    /// How far the open dialog sits from its centred resting place.
    pub(crate) offset: Point<Pixels>,
    /// The pointer position and the offset when the current drag started.
    grab: Option<DragGrab>,
}

#[derive(Clone, Copy)]
struct DragGrab {
    mouse: Point<Pixels>,
    offset: Point<Pixels>,
}

/// The payload that marks a drag by a dialog's title row.
#[derive(Clone)]
struct DialogDragged;

impl Render for DialogDragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl DialogDrag {
    /// A drag started at `position`: later moves continue from here.
    fn begin(&mut self, position: Point<Pixels>) {
        self.grab = Some(DragGrab {
            mouse: position,
            offset: self.offset,
        });
    }

    /// Pulls the surface along the pointer, keeping the title row reachable
    /// the way an egui window is: the row never leaves the viewport, even
    /// when the dialog below it is taller.
    fn drag(&mut self, event: &DragMoveEvent<DialogDragged>, window: &mut Window) {
        let Some(grab) = self.grab else { return };
        let candidate = grab.offset + (event.event.position - grab.mouse);

        // The painted header bounds already carry the applied offset, so the
        // shift that lands the candidate inside the viewport is the clamped
        // one.  8 px keeps the row clear of the window edge.
        let bounds = event.bounds;
        let view = window.viewport_size();
        let row = f32::from(bounds.size.height);
        let margin = 8.0;
        let left = (f32::from(bounds.origin.x) + f32::from(candidate.x - self.offset.x))
            .max(margin)
            .min(f32::from(view.width) - f32::from(bounds.size.width) - margin);
        let top = (f32::from(bounds.origin.y) + f32::from(candidate.y - self.offset.y))
            .max(margin)
            .min(f32::from(view.height) - row - margin);
        self.offset += point(
            px(left - f32::from(bounds.origin.x)),
            px(top - f32::from(bounds.origin.y)),
        );
    }
}

/// Shifts the dialog surface to where the user dragged it.  The offset was
/// clamped while dragging, so it is applied as-is.
fn dragged(dialog: Dialog, offset: Point<Pixels>) -> Dialog {
    dialog
        .ml(px(f32::from(offset.x)))
        .mt(px(f32::from(offset.y)))
}

impl XcomApp {
    pub(crate) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        let version = env!("CARGO_PKG_VERSION");
        let layout = self.button_layout;
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            dragged(
                dialog
                    .close_button(false)
                    .title(header(weak.clone(), layout, tr("关于 XCOM"), None, cx))
                    .width(px(520.0))
                    .child(
                        v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_lg()
                                .font_bold()
                                .child(SharedString::from(tr("XCOM 串口调试助手"))),
                        )
                        .child(div().text_sm().child(SharedString::from(format!(
                            "{} {version}",
                            tr("版本")
                        ))))
                        .child(div().child(SharedString::from(tr(
                            "使用 Rust + gpui-kit 开发的跨平台串口调试工具，可在 Windows / Linux / macOS 上运行。",
                        ))))
                        .child(div().text_sm().child(SharedString::from(tr("功能："))))
                        .child(div().text_sm().child(SharedString::from(tr(
                            "• 串口参数全面可配（自定义波特率、1/1.5/2 停止位）",
                        ))))
                        .child(div().text_sm().child(SharedString::from(tr(
                            "• 十六进制 / 文本收发，时间戳断帧",
                        ))))
                        .child(div().text_sm().child(SharedString::from(tr(
                            "• 编码自动识别与手动指定",
                        ))))
                        .child(div().text_sm().child(SharedString::from(tr(
                            "• 单条 / 多条 / 协议传输三种发送方式",
                        ))))
                        .child(div().text_sm().child(SharedString::from(tr(
                            "• 黑色与白色主题",
                        ))))
                        .child(div().text_sm().child(SharedString::from(tr(
                            "本软件为开源实现，界面不含任何广告。",
                        )))),
                ),
                app.dialog_drag.offset,
            )
                .footer(footer())
        });
    }

    pub(crate) fn open_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        let layout = self.button_layout;
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let rows = [
                (tr("发送"), "Ctrl+Enter"),
                (tr("打开 / 关闭串口"), "Ctrl+O"),
                (tr("刷新串口列表"), "F5"),
                (tr("保存接收数据"), "Ctrl+S"),
                (tr("保存原始数据"), "Ctrl+Shift+S"),
                (tr("复制选中"), "Ctrl+C"),
                (tr("全选"), "Ctrl+A"),
                (tr("退出"), "Ctrl+Q"),
            ];
            dragged(
                dialog
                    .close_button(false)
                    .title(header(weak.clone(), layout, tr("快捷键"), None, cx))
                    .width(px(420.0))
                    .child(
                        v_flex()
                            .gap_2()
                            .children(rows.into_iter().map(|(label, key)| {
                                h_flex()
                                    .justify_between()
                                    .child(div().child(SharedString::from(label)))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(SharedString::from(key)),
                                    )
                            })),
                    ),
                app.dialog_drag.offset,
            )
            .footer(footer())
        });
    }

    pub(crate) fn open_font_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        let weak = cx.entity().downgrade();
        let layout = self.button_layout;
        let families = crate::fonts::available_families();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let fonts = app.settings.fonts.clone();
            let family_items = |selected: &Option<String>, kind: fn(Option<String>) -> Choice| {
                let mut items = vec![(tr("系统默认").to_owned(), kind(None), selected.is_none())];
                for family in &families {
                    items.push((
                        family.clone(),
                        kind(Some(family.clone())),
                        selected.as_deref() == Some(family.as_str()),
                    ));
                }
                items
            };
            let size_items = |selected: f32, min: u32, max: u32, kind: fn(f32) -> Choice| {
                (min..=max)
                    .map(|size| {
                        (
                            format!("{size} pt"),
                            kind(size as f32),
                            (selected - size as f32).abs() < 0.01,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            dragged(
                dialog
                    .close_button(false)
                    .title(header(weak.clone(), layout, tr("字体设置"), None, cx))
                    .width(px(FONT_WINDOW_W))
                    .child(
                        v_flex()
                            .gap_3()
                            .child(field(
                                tr("界面字体"),
                                super::panels::choice_button(
                                    "font-ui-family",
                                    fonts
                                        .ui_family
                                        .clone()
                                        .unwrap_or_else(|| tr("系统默认").to_owned()),
                                    family_items(&fonts.ui_family, Choice::UiFont),
                                    weak.clone(),
                                ),
                            ))
                            .child(field(
                                tr("界面字号"),
                                super::panels::choice_button(
                                    "font-ui-size",
                                    format!("{} pt", fonts.ui_size),
                                    size_items(fonts.ui_size, 9, 24, Choice::UiSize),
                                    weak.clone(),
                                ),
                            ))
                            .child(field(
                                tr("接收区字体"),
                                super::panels::choice_button(
                                    "font-receive-family",
                                    fonts
                                        .receive_family
                                        .clone()
                                        .unwrap_or_else(|| tr("系统等宽").to_owned()),
                                    family_items(&fonts.receive_family, Choice::ReceiveFont),
                                    weak.clone(),
                                ),
                            ))
                            .child(field(
                                tr("接收区字号"),
                                super::panels::choice_button(
                                    "font-receive-size",
                                    format!("{} pt", fonts.receive_size),
                                    size_items(fonts.receive_size, 8, 32, Choice::ReceiveSize),
                                    weak.clone(),
                                ),
                            ))
                            .child(field(
                                tr("发送区字体"),
                                super::panels::choice_button(
                                    "font-send-family",
                                    fonts
                                        .send_family
                                        .clone()
                                        .unwrap_or_else(|| tr("系统等宽").to_owned()),
                                    family_items(&fonts.send_family, Choice::SendFont),
                                    weak.clone(),
                                ),
                            ))
                            .child(field(
                                tr("发送区字号"),
                                super::panels::choice_button(
                                    "font-send-size",
                                    format!("{} pt", fonts.send_size),
                                    size_items(fonts.send_size, 8, 32, Choice::SendSize),
                                    weak.clone(),
                                ),
                            ))
                            .child(field(
                                tr("行距"),
                                super::panels::choice_button(
                                    "line-spacing",
                                    format!("{:.0} pt", app.settings.line_spacing),
                                    (0..=40)
                                        .map(|value| {
                                            (
                                                format!("{value} pt"),
                                                Choice::LineSpacing(value as f32),
                                                (app.settings.line_spacing - value as f32).abs()
                                                    < 0.01,
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                    weak.clone(),
                                ),
                            ))
                            // egui spells out what 0 means and that changes apply
                            // to the text areas right away.
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(tr("行距 0 表示使用字体默认行高。"))),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(tr(
                                        "字体修改会立即应用到对应的文本区。",
                                    ))),
                            ),
                    ),
                app.dialog_drag.offset,
            )
        });
    }

    pub(crate) fn open_log_colors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        let weak = cx.entity().downgrade();
        let layout = self.button_layout;
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let colors = app.settings.log_colors;
            let palette = crate::theme::log_palette(cx, app.settings.theme, colors);
            dragged(
                dialog
                    .close_button(false)
                    .title(header(weak.clone(), layout, tr("接收区配色"), None, cx))
                    .width(px(LOG_COLORS_WINDOW_W))
                    .child(
                        v_flex()
                            .gap_3()
                            .child(onoff(
                                weak.clone(),
                                colors.enabled,
                                tr("自定义接收区配色"),
                                |this, value| this.settings.log_colors.enabled = value,
                            ))
                            // egui puts both colour pickers on one row, labelled
                            // 字色 / 底色; the hex field stands in for the picker.
                            .child(
                                h_flex()
                                    .gap_3()
                                    .items_center()
                                    .child(
                                        div().w(px(120.0)).child(field(
                                            tr("字色"),
                                            Input::new(&app.inputs.log_fg),
                                        )),
                                    )
                                    .child(
                                        div().w(px(120.0)).child(field(
                                            tr("底色"),
                                            Input::new(&app.inputs.log_bg),
                                        )),
                                    ),
                            )
                            .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
                            .child(div().text_sm().child(SharedString::from(tr("预览"))))
                            .child(
                                div()
                                    .p_3()
                                    .rounded(cx.theme().radius)
                                    .bg(palette.background)
                                    .text_color(palette.foreground)
                                    .child(SharedString::from(tr("接收区示例"))),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(tr(
                                        "接收的数据以 -> 开头，发送的数据以 <- 开头",
                                    ))),
                            ),
                    ),
                app.dialog_drag.offset,
            )
        });
    }

    pub(crate) fn open_log_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        let weak = cx.entity().downgrade();
        let layout = self.button_layout;
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let active = app.log_file.is_active();
            let path = app
                .log_file
                .path()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            dragged(
                dialog
                    .close_button(false)
                    .title(header(weak.clone(), layout, tr("日志文件"), None, cx))
                    .width(px(LOG_FILE_WINDOW_W))
                    .child(
                        v_flex()
                        .gap_3()
                        .child(field(tr("保存目录"), Input::new(&app.inputs.log_dir)))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(tr("留空则使用配置目录下的 logs"))),
                        )
                        .child(field(tr("文件名格式"), Input::new(&app.inputs.log_pattern)))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(tr(
                                    "格式符：%Y 年（4 位）、%y 年（2 位）、%M 月、%D 日、%h 时、%m 分、%s/%S 秒、%f 毫秒、%H 串口名称、%% 百分号。",
                                ))),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(tr(
                                    "文件名在启用时展开一次；同名文件会追加写入；修改目录或格式在下次启用时生效。",
                                ))),
                        )
                        .when(!path.is_empty(), |this| {
                            this.child(
                                div().text_sm().child(SharedString::from(format!(
                                    "{} {path}",
                                    tr("当前文件:")
                                ))),
                            )
                        })
                        .child(
                            div()
                                .text_sm()
                                .child(SharedString::from(tr("启用实时保存（接收数据）"))),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("log-start")
                                        .label(tr("开始记录"))
                                        .primary()
                                        .disabled(active)
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                super::panels::run_action(
                                                    weak.clone(),
                                                    Action::StartLogFile,
                                                    window,
                                                    cx,
                                                )
                                            }
                                        }),
                                )
                                .child(
                                    Button::new("log-stop")
                                        .label(tr("停止记录"))
                                        .outline()
                                        .disabled(!active)
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                super::panels::run_action(
                                                    weak.clone(),
                                                    Action::StopLogFile,
                                                    window,
                                                    cx,
                                                )
                                            }
                                        }),
                                ),
                        )
                        .when(!active, |this| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(tr("未启用：不会创建文件。"))),
                            )
                        }),
                    ),
                app.dialog_drag.offset,
            )
        });
    }

    pub(crate) fn open_custom_baud(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        let weak = cx.entity().downgrade();
        let layout = self.button_layout;
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let rates = app.settings.serial.custom_bauds.clone();
            let input = app.inputs.custom_baud.clone();
            // The add-row hint mirrors egui: it reads the pending value so the
            // reason the button is disabled is always spelled out.
            let pending = input.read(cx).value().trim().parse::<u32>().ok();
            let can_add = pending.is_some_and(|rate| app.settings.serial.can_add_custom_baud(rate));
            let hint = if rates.len() >= crate::serial::config::MAX_CUSTOM_BAUDS {
                tr("自定义波特率列表已满")
            } else {
                tr("该波特率已存在或为标准值")
            };
            let mono = cx.theme().mono_font_family.clone();
            dragged(
                dialog
                    .close_button(false)
                    .title(header(
                        weak.clone(),
                        layout,
                        tr("自定义波特率"),
                        Some(gpui_kit::component::IconName::Settings2),
                        cx,
                    ))
                    .width(px(CUSTOM_BAUD_WINDOW_W))
                    .h(px(CUSTOM_BAUD_WINDOW_H))
                    .child(
                        v_flex()
                            .gap_2()
                            .child(div().text_sm().child(SharedString::from(tr("添加波特率"))))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(div().flex_1().min_w(px(0.0)).child(Input::new(&input)))
                                    .child(Button::new("baud-add").label(tr("添加")).on_click({
                                        let weak = weak.clone();
                                        let input = input.clone();
                                        move |_, _, cx| {
                                            if let Ok(rate) =
                                                input.read(cx).value().trim().parse::<u32>()
                                            {
                                                let _ = weak.update(cx, |this, cx| {
                                                    if this.settings.serial.add_custom_baud(rate) {
                                                        this.mark_settings_dirty();
                                                        cx.notify();
                                                    }
                                                });
                                            }
                                        }
                                    })),
                            )
                            .when(!can_add, |this| {
                                this.child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(hint)),
                                )
                            })
                            .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
                            .child(
                                div()
                                    .text_sm()
                                    .child(SharedString::from(tr("已保存的自定义波特率"))),
                            )
                            .child(if rates.is_empty() {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(SharedString::from(tr("暂无自定义波特率")))
                                    .into_any_element()
                            } else {
                                v_flex()
                                    .gap_1()
                                    .children(rates.into_iter().map(|rate| {
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_family(mono.clone())
                                                    .child(SharedString::from(rate.to_string())),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(SharedString::from("bps")),
                                            )
                                            .child(div().flex_1())
                                            .child(
                                                Button::new(("baud-remove", rate))
                                                    .label("−")
                                                    .tooltip(tr("删除"))
                                                    .ghost()
                                                    .small()
                                                    .on_click({
                                                        let weak = weak.clone();
                                                        move |_, _, cx| {
                                                            let _ = weak.update(cx, |this, cx| {
                                                                this.settings
                                                                    .serial
                                                                    .remove_custom_baud(rate);
                                                                this.mark_settings_dirty();
                                                                cx.notify();
                                                            });
                                                        }
                                                    }),
                                            )
                                    }))
                                    .into_any_element()
                            }),
                    ),
                app.dialog_drag.offset,
            )
        });
    }

    pub(crate) fn open_highlight(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_drag = DialogDrag::default();
        self.rebuild_highlight_inputs(window, cx);
        let weak = cx.entity().downgrade();
        let layout = self.button_layout;
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let rules = app.settings.highlights.rules.clone();
            dragged(
                dialog
                    .close_button(false)
                    .title(header(weak.clone(), layout, tr("关键字高亮"), None, cx))
                    .width(px(HIGHLIGHT_WINDOW_W))
                    .h(px(HIGHLIGHT_WINDOW_H))
                    .child(
                        v_flex()
                        .gap_2()
                        .child(
                            v_flex()
                                // 表头：与行共用同一组列宽（egui 的 #/启用/关键字/颜色/操作）
                                .child(
                                    h_flex()
                                        .id("hl-headers")
                                        .test_support()
                                        .w_full()
                                        .gap(px(HL_COL_GAP))
                                        .items_center()
                                        .child(hl_header("#", HL_INDEX_W, cx))
                                        .child(hl_header(tr("启用"), HL_ENABLE_W, cx))
                                        .child(hl_header_flex(tr("关键字"), cx))
                                        .child(hl_header(tr("颜色"), HL_COLOR_W, cx))
                                        .child(hl_header(tr("操作"), HL_ACTION_W, cx)),
                                )
                                .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
                                .when(rules.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(SharedString::from(tr("暂无关键字"))),
                                    )
                                })
                                .children(rules.iter().enumerate().flat_map(
                                    |(index, rule)| {
                                        let inputs = &app.highlight_inputs[index];
                                        let invalid = rule.regex
                                            && crate::highlight::validate_regex(&rule.text)
                                                .is_err();
                                        let rule_row = v_flex()
                                            .gap_1()
                                            .child(
                                            h_flex()
                                                .w_full()
                                                .gap(px(HL_COL_GAP))
                                                .items_center()
                                                .child(
                                                    div()
                                                        .w(px(HL_INDEX_W))
                                                        .flex_shrink_0()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(SharedString::from(format!(
                                                            "#{}",
                                                            index + 1
                                                        ))),
                                                )
                                                .child(
                                                    div().w(px(HL_ENABLE_W)).flex_shrink_0().child(
                                                        Checkbox::new(("hl-enabled", index))
                                                            .checked(rule.enabled)
                                                            .tooltip(tr("启用此规则"))
                                                            .on_change({
                                                                let weak = weak.clone();
                                                                move |next, _, cx| {
                                                                    let next = *next;
                                                                    let _ = weak.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            if let Some(rule) = this
                                                                                .settings
                                                                                .highlights
                                                                                .rules
                                                                                .get_mut(index)
                                                                            {
                                                                                rule.enabled = next;
                                                                                this.mark_settings_dirty();
                                                                            }
                                                                            cx.notify();
                                                                        },
                                                                    );
                                                                }
                                                            }),
                                                    ),
                                                )
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .min_w(px(0.0))
                                                        .child(Input::new(&inputs.text)),
                                                )
                                                .child(
                                                    div().w(px(HL_COLOR_W)).flex_shrink_0().child(
                                                        Input::new(&inputs.color).w_full(),
                                                    ),
                                                )
                                                .child(
                                                    h_flex()
                                                        .w(px(HL_ACTION_W))
                                                        .flex_shrink_0()
                                                        .gap_0p5()
                                                        .child(
                                                            Button::new(("hl-regex", index))
                                                                .label(".*")
                                                                .tooltip(tr("按正则表达式匹配"))
                                                                .when(rule.regex, |button| {
                                                                    button.primary()
                                                                })
                                                                .when(!rule.regex, |button| {
                                                                    button.ghost()
                                                                })
                                                                .small()
                                                                .on_click({
                                                                    let weak = weak.clone();
                                                                    move |_, _, cx| {
                                                                        let _ = weak.update(
                                                                            cx,
                                                                            |this, cx| {
                                                                                if let Some(rule) = this
                                                                                    .settings
                                                                                    .highlights
                                                                                    .rules
                                                                                    .get_mut(index)
                                                                                {
                                                                                    rule.regex = !rule.regex;
                                                                                    this.mark_settings_dirty();
                                                                                }
                                                                                cx.notify();
                                                                            },
                                                                        );
                                                                    }
                                                                }),
                                                        )
                                                        .child(
                                                            Button::new(("hl-remove", index))
                                                                .label("×")
                                                                .tooltip(tr("删除"))
                                                                .ghost()
                                                                .small()
                                                                .on_click({
                                                                    let weak = weak.clone();
                                                                    move |_, window, cx| {
                                                                        super::panels::run_action(
                                                                            weak.clone(),
                                                                            Action::HighlightRemove(index),
                                                                            window,
                                                                            cx,
                                                                        )
                                                                    }
                                                                }),
                                                        ),
                                                ),
                                        );
                                        let row = rule_row
                                            .when(invalid, |this| {
                                                this.child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().danger)
                                                        .child(SharedString::from(tr("正则无效"))),
                                                )
                                            })
                                            .into_any_element();
                                        // egui 在两条规则之间画一条分隔线，最后一行后面没有。
                                        if index + 1 < rules.len() {
                                            vec![
                                                row,
                                                div()
                                                    .h(px(1.0))
                                                    .w_full()
                                                    .bg(cx.theme().border)
                                                    .into_any_element(),
                                            ]
                                        } else {
                                            vec![row]
                                        }
                                    },
                                ))
                                .into_any_element(),
                        )
                        // egui 把添加按钮和规则数放在表下面，帮助文字压尾。
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    Button::new("hl-add")
                                        .label(tr("添加关键字"))
                                        .small()
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                super::panels::run_action(
                                                    weak.clone(),
                                                    Action::HighlightAdd,
                                                    window,
                                                    cx,
                                                )
                                            }
                                        }),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(crate::i18n::tr1(
                                            "{} 条规则",
                                            rules.len(),
                                        ))),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(tr(
                                    "勾选行首的启用开关后规则才生效；默认按字面匹配、区分大小写；点击 .* 后按正则表达式匹配（可用 (?i) 忽略大小写），重叠时较长匹配优先。",
                                ))),
                        ),
                    ),
                app.dialog_drag.offset,
            )
        });
    }

    pub(crate) fn open_reset_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            alert
                .title(tr("恢复默认设置？"))
                .description(tr("所有串口、显示、发送与协议配置都会恢复为默认值。"))
                .confirm()
                .ok_text(tr("恢复"))
                .ok_variant(ButtonVariant::Danger)
                .on_ok({
                    let weak = weak.clone();
                    move |_, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            this.settings = crate::settings::Settings::default();
                            this.needs_reset = true;
                            cx.notify();
                        });
                        true
                    }
                })
        });
    }
}
