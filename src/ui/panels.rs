//! Every visible panel of the application.
//!
//! The shell is the same as the original tool: one header row carrying the menu
//! bar and, on Linux, the window controls; configuration
//! on the left, receive log above the send area in the middle, the multi-send /
//! protocol panel on the right, and a status bar along the bottom.

use gpui_kit::MouseButton;
use gpui_kit::base::InteractiveElementExt as _;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::IconName;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::Button,
    button::ButtonRounded,
    button::ButtonVariants as _,
    checkbox::Checkbox,
    h_flex,
    input::{Input, Textarea},
    menu::PopupMenuItem,
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, Window, div, px, relative,
};

use crate::app::{Action, Choice, Level, SideTab, XcomApp};
use crate::i18n::tr;

/// Height of the window header: the menu bar, and on Linux the window
/// controls, share this one row.
const HEADER_HEIGHT: f32 = 34.0;

/// Compose the window header.
///
/// The window controls share the menu row rather than sitting on a strip of
/// their own, and they follow the desktop's `button-layout`: many themes put
/// them on the left, where they are then the first thing in the row.  The
/// space between the menus and the trailing controls drags the window.
fn header_bar(app: &mut XcomApp, window: &mut Window, cx: &mut Context<XcomApp>) -> AnyElement {
    let decorated = draws_own_title_bar(window);
    let (leading, trailing) = if decorated {
        let maximized = window.is_maximized();
        let layout = app.button_layout;
        log::debug!(
            "header: own title bar, left={:?} right={:?}",
            layout.left,
            layout.right
        );
        let leading: Vec<AnyElement> = layout
            .left
            .into_iter()
            .enumerate()
            .filter_map(|(index, button)| window_control(button?, index, "left", maximized))
            .collect();
        let trailing: Vec<AnyElement> = layout
            .right
            .into_iter()
            .enumerate()
            .filter_map(|(index, button)| window_control(button?, index, "right", maximized))
            .collect();
        (leading, trailing)
    } else {
        (Vec::new(), Vec::new())
    };

    // Without a self-drawn title bar the row simply ends after the menus.
    let middle = if decorated {
        title_drag(cx)
    } else {
        div().flex_1().into_any_element()
    };

    h_flex()
        .id("header-bar")
        .test_support()
        .w_full()
        .h(px(HEADER_HEIGHT))
        .items_center()
        .flex_shrink_0()
        .border_b_1()
        .border_color(if decorated {
            cx.theme().title_bar_border
        } else {
            cx.theme().border
        })
        .bg(cx.theme().title_bar)
        // The controls sit flush against the window edge the way a headerbar
        // draws them; the menu group keeps its own inset.
        .children(leading)
        .child(menu_bar(app, cx))
        .child(middle)
        .children(trailing)
        .into_any_element()
}

/// Whether this window has to draw its own title bar, and with it the window
/// controls.
///
/// Linux reports it through the decoration mode: when the compositor or the
/// window manager draws the frame it already offers the controls, and drawing
/// them again would duplicate them.  Windows always answers `Server`, so it is
/// decided by the window options instead: `main.rs` asks for a transparent
/// title bar, which strips the native caption, and this header takes its
/// place.  macOS keeps its traffic lights, so nothing is drawn there.
fn draws_own_title_bar(window: &Window) -> bool {
    #[cfg(target_os = "linux")]
    {
        matches!(
            window.window_decorations(),
            gpui_kit::Decorations::Client { .. }
        )
    }
    #[cfg(target_os = "windows")]
    {
        let _ = window;
        true
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = window;
        false
    }
}

/// The draggable part of the header: the window title, plus the gestures a
/// title bar is expected to offer.
fn title_drag(cx: &mut Context<XcomApp>) -> AnyElement {
    div()
        .id("title-drag")
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .px_3()
        // Windows drags and double-click-maximises a client area that is
        // marked as a caption; the other platforms use the handlers below.
        .window_control_area(gpui_kit::WindowControlArea::Drag)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.title_dragging = true),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.title_dragging = false),
        )
        .on_mouse_move(cx.listener(|this, _, window, _| {
            if this.title_dragging {
                this.title_dragging = false;
                window.start_window_move();
            }
        }))
        .on_double_click(|_, window, _| window.zoom_window())
        .on_mouse_down(MouseButton::Right, |event, window, _| {
            window.show_window_menu(event.position)
        })
        .test_support()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(SharedString::from(crate::i18n::window_title())),
        )
        .into_any_element()
}

/// Read the desktop's preferred window-button placement.
///
/// GPUI's platform reports GNOME's layout once the portal/D-Bus settings have
/// loaded; reading gsettings here as well means the very first frame already
/// has the controls on the right side instead of flashing them on the right
/// and then moving them.
pub(crate) fn read_button_layout() -> gpui_kit::WindowButtonLayout {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let text = text.trim().trim_matches('\'').trim_matches('"');
            if let Ok(layout) = gpui_kit::WindowButtonLayout::parse(text)
                && layout
                    .left
                    .iter()
                    .chain(layout.right.iter())
                    .any(|button| matches!(button, Some(gpui_kit::WindowButton::Close)))
            {
                return layout;
            }
        }
    }
    gpui_kit::WindowButtonLayout {
        left: [None; gpui_kit::MAX_BUTTONS_PER_SIDE],
        right: [
            Some(gpui_kit::WindowButton::Minimize),
            Some(gpui_kit::WindowButton::Maximize),
            Some(gpui_kit::WindowButton::Close),
        ],
    }
}

/// One window control of the header.
///
/// The controls fill the header's height and keep their width on both sides of
/// the menu group, so a left-hand layout starts with them at the window edge.
fn window_control(
    button: gpui_kit::WindowButton,
    index: usize,
    side: &'static str,
    maximized: bool,
) -> Option<AnyElement> {
    use gpui_kit::WindowButton;
    let (icon, label) = match button {
        WindowButton::Minimize => (IconName::WindowMinimize, tr("最小化")),
        WindowButton::Maximize if maximized => (IconName::WindowRestore, tr("还原")),
        WindowButton::Maximize => (IconName::WindowMaximize, tr("最大化")),
        WindowButton::Close => (IconName::WindowClose, tr("关闭")),
    };
    Some(
        Button::new(SharedString::from(format!(
            "window-{side}-{index}-{}",
            button.id()
        )))
        .ghost()
        .icon(icon)
        .tooltip(label)
        .w(px(36.0))
        .h_full()
        .flex_shrink_0()
        .rounded(ButtonRounded::None)
        .on_click(move |_, window, cx| match button {
            WindowButton::Minimize => window.minimize_window(),
            WindowButton::Maximize => window.zoom_window(),
            WindowButton::Close => {
                window.remove_window();
                cx.quit();
            }
        })
        .into_any_element(),
    )
}

fn edge_strip(
    id: &'static str,
    label: &'static str,
    left: bool,
    cx: &mut Context<XcomApp>,
    split: Entity<gpui_kit::component::resizable::ResizableState>,
) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .w(px(14.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().sidebar)
        .text_color(cx.theme().muted_foreground)
        .cursor_pointer()
        .when(left, |this| {
            this.border_r_1().border_color(cx.theme().border)
        })
        .when(!left, |this| {
            this.border_l_1().border_color(cx.theme().border)
        })
        .on_click(cx.listener(move |this, _, window, cx| {
            if left {
                this.settings.show_config_panel = true;
                this.settings.left_panel_width = this.settings.left_panel_width.max(240.0);
            } else {
                this.settings.show_tool_panel = true;
                this.settings.right_panel_width = this.settings.right_panel_width.max(260.0);
            }
            split.update(cx, |state, cx| {
                let sizes: Vec<f32> = state.sizes().iter().map(|s| f32::from(*s)).collect();
                if sizes.len() < 3 {
                    return;
                }
                let total: f32 = sizes.iter().sum();
                if left {
                    state.resize_panel(0, px(240.0), window, cx);
                } else {
                    let center = (total - sizes[0] - 260.0).max(1.0);
                    state.resize_panel(1, px(center), window, cx);
                }
            });
            this.mark_settings_dirty();
            cx.notify();
        }))
        .child(div().text_sm().child(SharedString::from(label)))
        .into_any_element()
}

fn send_strip(
    cx: &mut Context<XcomApp>,
    split: Entity<gpui_kit::component::resizable::ResizableState>,
) -> AnyElement {
    div()
        .id("send-strip")
        .test_support()
        .w_full()
        .h(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        .text_color(cx.theme().muted_foreground)
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, window, cx| {
            this.settings.show_send_area = true;
            this.settings.send_area_height = this.settings.send_area_height.max(120.0);
            split.update(cx, |state, cx| {
                let sizes: Vec<f32> = state.sizes().iter().map(|s| f32::from(*s)).collect();
                if sizes.len() < 2 {
                    return;
                }
                let total: f32 = sizes.iter().sum();
                let log = (total - 120.0).max(1.0);
                state.resize_panel(0, px(log), window, cx);
            });
            this.mark_settings_dirty();
            cx.notify();
        }))
        .child(div().text_sm().child(SharedString::from(tr("发送区"))))
        .into_any_element()
}

pub fn render(app: &mut XcomApp, window: &mut Window, cx: &mut Context<XcomApp>) -> AnyElement {
    use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};

    let theme = cx.theme().clone();
    let weak = cx.entity().downgrade();
    let left_width = px(app.settings.left_panel_width);
    let right_width = px(app.settings.right_panel_width);
    let send_height = px(app.settings.send_area_height);
    let show_config = app.settings.show_config_panel;
    let show_tool = app.settings.show_tool_panel;
    let show_send = app.settings.show_send_area;
    let main_split = app.main_split.clone();
    let center_split = app.center_split.clone();

    let mut center_column = v_flex().flex_1().min_w(px(0.0));
    let center = v_resizable("center-column")
        .with_state(&center_split)
        .child(resizable_panel().child(app.render_log(window, cx)))
        .child(
            resizable_panel()
                .visible(show_send)
                .size(send_height)
                .size_range(px(1.0)..px(560.0))
                .fixed()
                .child(send_area(app, window, cx)),
        )
        .on_resize({
            let weak = weak.clone();
            move |state, _window, cx| {
                let sizes = state.read(cx).sizes();
                let Some(height) = sizes.get(1).copied() else {
                    return;
                };
                let height = f32::from(height);
                let _ = weak.update(cx, |this, cx| {
                    if this.settings.show_send_area {
                        if height < 100.0 {
                            this.settings.show_send_area = false;
                            this.mark_settings_dirty();
                            cx.notify();
                        } else if height > 120.0
                            && (this.settings.send_area_height - height).abs() > 0.5
                        {
                            this.settings.send_area_height = height;
                            this.mark_settings_dirty();
                        }
                    }
                });
            }
        });
    center_column = center_column.child(center);
    if !show_send {
        center_column = center_column.child(send_strip(cx, center_split.clone()));
    }

    let body_group = h_resizable("main-columns")
        .with_state(&main_split)
        .child(
            resizable_panel()
                .visible(show_config)
                .size(left_width)
                .size_range(px(1.0)..px(460.0))
                .fixed()
                .child(left_panel(app, cx)),
        )
        .child(resizable_panel().child(center_column))
        .child(
            resizable_panel()
                .visible(show_tool)
                .size(right_width)
                .size_range(
                    // The floor is the table's own minimum width as well as the
                    // stored setting: a narrower panel pushes the delete column
                    // past the header row.
                    px(crate::settings::RIGHT_PANEL_MIN.max(multi_table_min_width()))
                        ..px(crate::settings::RIGHT_PANEL_MAX),
                )
                .fixed()
                .child(tool_panel(app, window, cx)),
        )
        .on_resize({
            let weak = weak.clone();
            move |state, _window, cx| {
                let sizes = state.read(cx).sizes().to_vec();
                let _ = weak.update(cx, |this, cx| {
                    if let Some(width) = sizes.first().copied() {
                        let width = f32::from(width);
                        if this.settings.show_config_panel {
                            if width < 120.0 {
                                this.settings.show_config_panel = false;
                                this.mark_settings_dirty();
                                cx.notify();
                            } else if width > 140.0
                                && (this.settings.left_panel_width - width).abs() > 0.5
                            {
                                this.settings.left_panel_width = width;
                                this.mark_settings_dirty();
                            }
                        }
                    }
                    if let Some(width) = sizes.get(2).copied() {
                        let width = f32::from(width);
                        // The tool panel has a hard floor: the multi-send table
                        // needs that much width, and the egui version refuses to
                        // go below it rather than growing a horizontal scroll
                        // bar.  Visibility is a menu toggle, not a drag gesture.
                        if this.settings.show_tool_panel
                            && width >= crate::settings::RIGHT_PANEL_MIN
                            && (this.settings.right_panel_width - width).abs() > 0.5
                        {
                            this.settings.right_panel_width = width;
                            this.mark_settings_dirty();
                        }
                    }
                });
            }
        });

    let mut body = h_flex().items_stretch().flex_1().min_h(px(0.0));
    if !show_config {
        body = body.child(edge_strip(
            "config-strip",
            "›",
            true,
            cx,
            main_split.clone(),
        ));
    }
    body = body.child(body_group);
    if !show_tool {
        body = body.child(edge_strip("tool-strip", "‹", false, cx, main_split.clone()));
    }

    v_flex()
        .size_full()
        .bg(theme.background)
        .text_color(theme.foreground)
        .child(header_bar(app, window, cx))
        .child(body)
        .child(status_bar(app, cx))
        .into_any_element()
}
pub(crate) fn run_action(
    weak: WeakEntity<XcomApp>,
    action: Action,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(entity) = weak.upgrade() else {
        return;
    };
    entity.update(cx, |this, cx| {
        this.apply_action(action, window, cx);
        cx.notify();
    });
}

fn action_item(
    label: impl Into<SharedString>,
    action: Action,
    weak: WeakEntity<XcomApp>,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        run_action(weak.clone(), action, window, cx);
    })
}

fn check_item(
    label: impl Into<SharedString>,
    checked: bool,
    action: Action,
    weak: WeakEntity<XcomApp>,
) -> PopupMenuItem {
    action_item(label, action, weak).checked(checked)
}

fn choice_item(
    label: impl Into<SharedString>,
    choice: Choice,
    checked: bool,
    weak: WeakEntity<XcomApp>,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .checked(checked)
        .on_click(move |_, _, cx| {
            let choice = choice.clone();
            let _ = weak.update(cx, |this, cx| this.apply_choice(choice, cx));
        })
}

fn port_item(
    text: String,
    choice: Choice,
    checked: bool,
    detail: Option<String>,
    weak: WeakEntity<XcomApp>,
) -> PopupMenuItem {
    match detail {
        // A port with metadata gets a second, muted line.  (gpui menu items
        // carry no tooltip API; only buttons and checkboxes do.)
        Some(detail) => PopupMenuItem::element(move |_, cx| {
            v_flex()
                .gap_0p5()
                .child(div().child(SharedString::from(text.clone())))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(detail.clone())),
                )
        })
        .checked(checked)
        .on_click(move |_, _, cx| {
            let choice = choice.clone();
            let _ = weak.update(cx, |this, cx| this.apply_choice(choice, cx));
        }),
        None => PopupMenuItem::new(text)
            .checked(checked)
            .on_click(move |_, _, cx| {
                let choice = choice.clone();
                let _ = weak.update(cx, |this, cx| this.apply_choice(choice, cx));
            }),
    }
}

/// A dropdown button for the port list.
///
/// `detail` is the selected port's extra description (product name, VID/PID,
/// …), shown as the trigger's tooltip.
/// egui 串口网格的列宽：下拉框固定 140px（SERIAL_COMBO_WIDTH），
/// 标签列放下最宽的 `DTR / RTS`。
const SERIAL_COMBO_WIDTH: f32 = 140.0;
const SERIAL_LABEL_WIDTH: f32 = 64.0;

fn port_choice_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    current_detail: Option<String>,
    items: Vec<(String, Choice, bool, Option<String>)>,
    disabled: bool,
    weak: WeakEntity<XcomApp>,
) -> impl IntoElement {
    let mut trigger = Button::new(id).label(label).disabled(disabled);
    if let Some(detail) = current_detail {
        trigger = trigger.tooltip(detail);
    }
    // egui 的 ComboBox：选中文字靠左，下拉箭头贴右。选择器铺满外层容器
    // （容器限制在 72..140 之间，随面板宽度收缩）。
    trigger
        .w_full()
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _window, _cx| {
            let mut menu = menu;
            if items.is_empty() {
                // egui shows a weak label rather than an empty popup.
                return menu.item(PopupMenuItem::new(tr("未检测到串口")).disabled(true));
            }
            for (text, choice, checked, detail) in items.clone() {
                menu = menu.item(port_item(text, choice, checked, detail, weak.clone()));
            }
            menu
        })
}

pub(crate) fn choice_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    items: Vec<(String, Choice, bool)>,
    disabled: bool,
    weak: WeakEntity<XcomApp>,
) -> impl IntoElement {
    Button::new(id)
        .label(label)
        .disabled(disabled)
        // egui 的 ComboBox：选中文字靠左，下拉箭头贴右。
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _window, _cx| {
            let mut menu = menu;
            for (text, choice, checked) in items.clone() {
                menu = menu.item(choice_item(text, choice, checked, weak.clone()));
            }
            menu
        })
}

/// The width of a combo-style dropdown button.
enum ComboWidth {
    /// Fill the parent, which caps the width (the port/baud rows).
    Fill,
    /// A fixed pixel width (egui's `SERIAL_COMBO_WIDTH` dropdowns).
    Fixed(f32),
}

/// A combo-style dropdown button with egui's ComboBox look: the selected
/// text sits at the left edge and the caret hugs the right.
fn combo_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    items: Vec<(String, Choice, bool)>,
    disabled: bool,
    width: ComboWidth,
    weak: WeakEntity<XcomApp>,
) -> impl IntoElement {
    let mut button = Button::new(id).label(label).disabled(disabled);
    button = match width {
        ComboWidth::Fill => button.w_full(),
        ComboWidth::Fixed(w) => button.w(px(w)),
    };
    button
        .dropdown_caret(true)
        .dropdown_menu(move |menu, _window, _cx| {
            let mut menu = menu;
            for (text, choice, checked) in items.clone() {
                menu = menu.item(choice_item(text, choice, checked, weak.clone()));
            }
            menu
        })
}

fn menu_bar(app: &mut XcomApp, cx: &mut Context<XcomApp>) -> AnyElement {
    let weak = cx.entity().downgrade();
    let is_open = app.link.is_open();
    let has_selection = app.log_view.has_selection();
    let display = app.settings.display;
    let auto_scroll = app.settings.auto_scroll;
    let echo_sent = app.settings.echo_sent;
    let paused = app.show_paused;
    let show_config = app.settings.show_config_panel;
    let show_tool = app.settings.show_tool_panel;
    let show_send = app.settings.show_send_area;
    let encoding = app.settings.encoding;
    let tx_encoding = app.settings.tx_encoding;
    let theme = app.settings.theme;
    let lang = crate::i18n::lang();
    let log_active = app.log_file.is_active();

    let file_weak = weak.clone();
    let edit_weak = weak.clone();
    let view_weak = weak.clone();
    let tools_weak = weak.clone();
    let help_weak = weak.clone();

    h_flex()
        .h_full()
        .px_2()
        .gap_1()
        .items_center()
        .flex_shrink_0()
        .child(
            Button::new("menu-file")
                .ghost()
                .label(tr("文件"))
                .dropdown_menu(move |menu, _window, _cx| {
                    let weak = file_weak.clone();
                    menu.item(action_item(
                        if is_open {
                            tr("关闭串口")
                        } else {
                            tr("打开串口")
                        },
                        Action::TogglePort,
                        weak.clone(),
                    ))
                    .item(action_item(
                        tr("刷新串口列表"),
                        Action::RefreshPorts,
                        weak.clone(),
                    ))
                    .separator()
                    .item(action_item(
                        tr("日志文件…"),
                        Action::ShowLogFile,
                        weak.clone(),
                    ))
                    .item(action_item(
                        tr("保存接收数据…"),
                        Action::SaveReceived,
                        weak.clone(),
                    ))
                    .item(action_item(
                        tr("保存原始数据…"),
                        Action::SaveRaw,
                        weak.clone(),
                    ))
                    .separator()
                    .item(action_item(tr("退出"), Action::Quit, weak.clone()))
                }),
        )
        .child(
            Button::new("menu-edit")
                .ghost()
                .label(tr("编辑"))
                .dropdown_menu(move |menu, _window, _cx| {
                    let weak = edit_weak.clone();
                    menu.item({
                        let item = action_item(tr("复制选中"), Action::CopySelection, weak.clone());
                        item.disabled(!has_selection)
                    })
                    .item(action_item(tr("全选"), Action::SelectAll, weak.clone()))
                    .separator()
                    .item(action_item(
                        tr("清除接收"),
                        Action::ClearReceive,
                        weak.clone(),
                    ))
                    .item(action_item(tr("清除发送"), Action::ClearSend, weak.clone()))
                    .separator()
                    .item(action_item(
                        tr("恢复默认设置…"),
                        Action::ResetSettings,
                        weak.clone(),
                    ))
                    .item(action_item(
                        tr("快捷键…"),
                        Action::ShowShortcuts,
                        weak.clone(),
                    ))
                }),
        )
        .child(
            Button::new("menu-view")
                .ghost()
                .label(tr("视图"))
                .dropdown_menu(move |menu, window, cx| {
                    let panels_weak = view_weak.clone();
                    let display_weak = view_weak.clone();
                    let encoding_weak = view_weak.clone();
                    let theme_weak = view_weak.clone();
                    let language_weak = view_weak.clone();
                    menu.submenu(tr("面板"), window, cx, move |menu, _, _| {
                        menu.item(check_item(
                            tr("配置面板"),
                            show_config,
                            Action::ToggleConfigPanel,
                            panels_weak.clone(),
                        ))
                        .item(check_item(
                            tr("工具面板"),
                            show_tool,
                            Action::ToggleToolPanel,
                            panels_weak.clone(),
                        ))
                        .item(check_item(
                            tr("发送区"),
                            show_send,
                            Action::ToggleSendArea,
                            panels_weak.clone(),
                        ))
                    })
                    .submenu(tr("显示"), window, cx, move |menu, window, cx| {
                        let style_weak = display_weak.clone();
                        let row_weak = display_weak.clone();
                        menu.item(check_item(
                            tr("文本"),
                            display.mode == crate::logstore::ViewMode::Text,
                            Action::SetTextMode,
                            display_weak.clone(),
                        ))
                        .item(check_item(
                            tr("16进制显示"),
                            display.mode == crate::logstore::ViewMode::Hex,
                            Action::SetHexMode,
                            display_weak.clone(),
                        ))
                        // HEX 布局只在 16 进制模式下有意义（egui 也是这么收的）。
                        .when(display.mode == crate::logstore::ViewMode::Hex, |menu| {
                            menu.submenu(tr("HEX 布局"), window, cx, move |menu, window, cx| {
                                // The builder runs per open, so the handle for
                                // the nested menu is cloned here, not moved in.
                                let row_weak = row_weak.clone();
                                menu.item(check_item(
                                    tr("连续"),
                                    display.hex_style == crate::logstore::HexStyle::Stream,
                                    Action::SetHexStyle(crate::logstore::HexStyle::Stream),
                                    style_weak.clone(),
                                ))
                                .item(check_item(
                                    tr("16 字节/行"),
                                    display.hex_style == crate::logstore::HexStyle::Dump,
                                    Action::SetHexStyle(crate::logstore::HexStyle::Dump),
                                    style_weak.clone(),
                                ))
                                .separator()
                                .item(check_item(
                                    tr("大写"),
                                    display.upper_hex,
                                    Action::ToggleUpperHex,
                                    style_weak.clone(),
                                ))
                                .separator()
                                .submenu(
                                    tr("每行字节"),
                                    window,
                                    cx,
                                    move |menu, _, _| {
                                        let mut menu = menu;
                                        for count in [8usize, 16, 24, 32, 64] {
                                            menu = menu.item(check_item(
                                                format!("{count}"),
                                                display.hex_bytes_per_row == count,
                                                Action::SetHexPerRow(count),
                                                row_weak.clone(),
                                            ));
                                        }
                                        menu
                                    },
                                )
                            })
                        })
                        .item(check_item(
                            tr("暂停显示"),
                            paused,
                            Action::TogglePause,
                            display_weak.clone(),
                        ))
                        .item(check_item(
                            tr("回显发送"),
                            echo_sent,
                            Action::ToggleEcho,
                            display_weak.clone(),
                        ))
                        .separator()
                        .item(check_item(
                            tr("方向标记"),
                            display.markers,
                            Action::ToggleMarkers,
                            display_weak.clone(),
                        ))
                        .item(check_item(
                            tr("时间戳"),
                            display.timestamp,
                            Action::ToggleTimestamp,
                            display_weak.clone(),
                        ))
                        .item(check_item(
                            tr("自动换行"),
                            display.wrap,
                            Action::ToggleWrap,
                            display_weak.clone(),
                        ))
                        .item(check_item(
                            tr("自动滚动到最新"),
                            auto_scroll,
                            Action::ToggleAutoScroll,
                            display_weak.clone(),
                        ))
                        .separator()
                        .item(action_item(
                            tr("字体设置…"),
                            Action::ShowFontSettings,
                            display_weak.clone(),
                        ))
                        .item(action_item(
                            tr("接收区配色…"),
                            Action::ShowLogColors,
                            display_weak.clone(),
                        ))
                        .item(action_item(
                            tr("关键字高亮…"),
                            Action::ShowHighlight,
                            display_weak.clone(),
                        ))
                    })
                    .submenu(tr("编码"), window, cx, move |menu, window, cx| {
                        let mut menu = menu.item(choice_item(
                            tr("自动识别"),
                            Choice::Encoding(None),
                            encoding.is_none(),
                            encoding_weak.clone(),
                        ));
                        for id in crate::encoding::EncodingId::ALL {
                            menu = menu.item(choice_item(
                                id.label(),
                                Choice::Encoding(Some(id)),
                                encoding == Some(id),
                                encoding_weak.clone(),
                            ));
                        }
                        let tx_weak = encoding_weak.clone();
                        menu.separator().submenu(
                            tr("发送编码"),
                            window,
                            cx,
                            move |menu, _, _| {
                                let mut menu = menu.item(choice_item(
                                    tr("跟随接收编码"),
                                    Choice::TxEncoding(None),
                                    tx_encoding.is_none(),
                                    tx_weak.clone(),
                                ));
                                for id in crate::encoding::EncodingId::ALL {
                                    menu = menu.item(choice_item(
                                        id.label(),
                                        Choice::TxEncoding(Some(id)),
                                        tx_encoding == Some(id),
                                        tx_weak.clone(),
                                    ));
                                }
                                menu
                            },
                        )
                    })
                    .submenu(tr("主题"), window, cx, move |menu, _, _| {
                        let mut menu = menu;
                        for mode in crate::theme::ThemeMode::ALL {
                            menu = menu.item(choice_item(
                                mode.label(),
                                Choice::Theme(mode),
                                theme == mode,
                                theme_weak.clone(),
                            ));
                        }
                        menu
                    })
                    .submenu(tr("语言"), window, cx, move |menu, _, _| {
                        let mut menu = menu;
                        for language in crate::i18n::Lang::ALL {
                            menu = menu.item(choice_item(
                                language.label(),
                                Choice::Language(language),
                                lang == language,
                                language_weak.clone(),
                            ));
                        }
                        menu
                    })
                }),
        )
        .child(
            Button::new("menu-tools")
                .ghost()
                .label(tr("工具"))
                .dropdown_menu(move |menu, _window, _cx| {
                    let weak = tools_weak.clone();
                    menu.item(action_item(
                        tr("自定义波特率…"),
                        Action::ShowCustomBaud,
                        weak.clone(),
                    ))
                    .item(action_item(tr("发送文件…"), Action::SendFile, weak.clone()))
                    .separator()
                    .item(if log_active {
                        action_item(tr("停止日志记录"), Action::StopLogFile, weak.clone())
                    } else {
                        action_item(tr("开始日志记录"), Action::StartLogFile, weak.clone())
                    })
                    .item(action_item(
                        tr("日志文件设置…"),
                        Action::ShowLogFile,
                        weak.clone(),
                    ))
                }),
        )
        .child(
            Button::new("menu-help")
                .ghost()
                .label(tr("帮助"))
                .dropdown_menu(move |menu, _window, _cx| {
                    let weak = help_weak.clone();
                    menu.item(action_item(
                        tr("快捷键…"),
                        Action::ShowShortcuts,
                        weak.clone(),
                    ))
                    .item(action_item(
                        tr("关于 XCOM…"),
                        Action::ShowAbout,
                        weak.clone(),
                    ))
                }),
        )
        .into_any_element()
}

fn left_panel(app: &mut XcomApp, cx: &mut Context<XcomApp>) -> AnyElement {
    let is_open = app.link.is_open();
    let weak = cx.entity().downgrade();
    let current_port = app.settings.serial.port.clone();
    let ports = app.ports.ports().to_vec();
    let mut port_items: Vec<(String, Choice, bool, Option<String>)> = ports
        .iter()
        .map(|port| {
            (
                port.display(),
                Choice::Port(port.name.clone()),
                port.name == current_port,
                port.detail(),
            )
        })
        .collect();
    if !ports.iter().any(|port| port.name == current_port) && !current_port.is_empty() {
        port_items.push((
            current_port.clone(),
            Choice::Port(current_port.clone()),
            true,
            None,
        ));
    }
    // The trigger always has a label; the popup shows 未检测到串口 when the
    // list is empty, so no placeholder row is added here.
    // The trigger shows the selected port's detail on hover.
    let current_detail = ports
        .iter()
        .find(|port| port.name == current_port)
        .and_then(|port| port.detail());
    let baud_items: Vec<(String, Choice, bool)> = app
        .settings
        .serial
        .baud_choices()
        .into_iter()
        .map(|rate| {
            let label = if crate::serial::config::STANDARD_BAUDS.contains(&rate) {
                rate.to_string()
            } else {
                format!("{rate}{}", tr("（自定义）"))
            };
            (label, Choice::Baud(rate), rate == app.settings.serial.baud)
        })
        .collect();
    let data_bits: Vec<(String, Choice, bool)> = crate::serial::DataBitsSel::ALL
        .into_iter()
        .map(|value| {
            (
                value.label().to_owned(),
                Choice::DataBits(value),
                value == app.settings.serial.data_bits,
            )
        })
        .collect();
    let parity: Vec<(String, Choice, bool)> = crate::serial::ParitySel::ALL
        .into_iter()
        .map(|value| {
            (
                tr(value.label()).to_owned(),
                Choice::Parity(value),
                value == app.settings.serial.parity,
            )
        })
        .collect();
    let stop_bits: Vec<(String, Choice, bool)> = crate::serial::StopBitsSel::ALL
        .into_iter()
        .map(|value| {
            (
                value.label().to_owned(),
                Choice::StopBits(value),
                value == app.settings.serial.stop_bits,
            )
        })
        .collect();
    let flow: Vec<(String, Choice, bool)> = crate::serial::FlowSel::ALL
        .into_iter()
        .map(|value| {
            (
                tr(value.label()).to_owned(),
                Choice::Flow(value),
                value == app.settings.serial.flow,
            )
        })
        .collect();
    let port_label = ports
        .iter()
        .find(|port| port.name == current_port)
        .map(|port| port.display())
        .unwrap_or_else(|| {
            if current_port.is_empty() {
                tr("（无可用串口）").to_owned()
            } else {
                current_port.clone()
            }
        });
    let dtr = app.settings.serial.dtr;
    let rts = app.settings.serial.rts;
    let summary = app.settings.serial.summary();
    // egui 的波特率选择器：非标准值显示 "（自定义）" 后缀。
    let baud_label = if crate::serial::config::STANDARD_BAUDS.contains(&app.settings.serial.baud) {
        format!("{}", app.settings.serial.baud)
    } else {
        format!("{}{}", app.settings.serial.baud, tr("（自定义）"))
    };
    // egui 版把打开按钮限制在 96..168 之间并相对串口区中心左移 16px：
    // 撑满整个宽度的按钮会被读成分隔线而不是控件。
    let open_button_width = (app.settings.left_panel_width - 24.0).clamp(96.0, 168.0);
    let serial_section_open = app.serial_section_open;
    let stats_section_open = app.stats_section_open;

    v_flex()
        .id("left-panel-scroll")
        .w_full()
        .h_full()
        .gap_2()
        .p_3()
        .overflow_y_scroll()
        .border_r_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        // 串口设置区：可折叠表头（egui 的 CollapsingHeader，默认展开），
        // 折叠状态只存在内存里，不写配置。
        .child(
            h_flex()
                .id("serial-section")
                .test_support()
                .w_full()
                .gap_1()
                .items_center()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.serial_section_open = !this.serial_section_open;
                    cx.notify();
                }))
                .child(disclosure(serial_section_open, cx))
                .child(section_title(tr("串口设置"), cx)),
        )
        .when(serial_section_open, |this| {
            // egui 的 serial_grid：两列（标签列 + 控件列），列间距 8、行间距 6。
            // 控件是固定 140px 的紧凑下拉框（SERIAL_COMBO_WIDTH），紧跟在
            // 标签列右侧，不拉伸到面板边缘；打开串口后参数选择器禁用。
            this.child(
                v_flex()
                    .id("serial-grid")
                    .test_support()
                    .w_full()
                    .mt(px(2.0))
                    .gap(px(6.0))
                    // 串口：选择器（72..140 随面板收缩）+ 相邻的刷新按钮。
                    // egui 版串口打开时只禁用选择器，刷新按钮保持可用。
                    .child(serial_row(
                        tr("串口"),
                        h_flex()
                            .w_full()
                            .gap(px(8.0))
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(72.0))
                                    .max_w(px(SERIAL_COMBO_WIDTH))
                                    .child(port_choice_button(
                                        "port",
                                        port_label,
                                        current_detail,
                                        port_items,
                                        is_open,
                                        weak.clone(),
                                    )),
                            )
                            .child(
                                Button::new("refresh-ports")
                                    .icon(IconName::RefreshCw)
                                    .tooltip(tr("刷新"))
                                    .small()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.ports.poll();
                                        this.info(tr("已刷新串口列表"));
                                        cx.notify();
                                    })),
                            )
                            .into_any_element(),
                        cx,
                    ))
                    // 波特率：选择器 + 相邻的自定义波特率按钮（egui 的 tune 图标）。
                    .child(serial_row(
                        tr("波特率"),
                        h_flex()
                            .w_full()
                            .gap(px(8.0))
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(72.0))
                                    .max_w(px(SERIAL_COMBO_WIDTH))
                                    .child(combo_button(
                                        "baud",
                                        baud_label,
                                        baud_items,
                                        is_open,
                                        ComboWidth::Fill,
                                        weak.clone(),
                                    )),
                            )
                            .child(
                                Button::new("custom-baud")
                                    .icon(IconName::Settings2)
                                    .tooltip(tr("自定义波特率"))
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_custom_baud(window, cx);
                                    })),
                            )
                            .into_any_element(),
                        cx,
                    ))
                    .child(serial_row(
                        tr("数据位"),
                        combo_button(
                            "data-bits",
                            app.settings.serial.data_bits.label(),
                            data_bits,
                            is_open,
                            ComboWidth::Fixed(SERIAL_COMBO_WIDTH),
                            weak.clone(),
                        )
                        .into_any_element(),
                        cx,
                    ))
                    .child(serial_row(
                        tr("停止位"),
                        combo_button(
                            "stop-bits",
                            app.settings.serial.stop_bits.label(),
                            stop_bits,
                            is_open,
                            ComboWidth::Fixed(SERIAL_COMBO_WIDTH),
                            weak.clone(),
                        )
                        .into_any_element(),
                        cx,
                    ))
                    .child(serial_row(
                        tr("校验位"),
                        combo_button(
                            "parity",
                            tr(app.settings.serial.parity.label()),
                            parity,
                            is_open,
                            ComboWidth::Fixed(SERIAL_COMBO_WIDTH),
                            weak.clone(),
                        )
                        .into_any_element(),
                        cx,
                    ))
                    .child(serial_row(
                        tr("流控"),
                        combo_button(
                            "flow",
                            tr(app.settings.serial.flow.label()),
                            flow,
                            is_open,
                            ComboWidth::Fixed(SERIAL_COMBO_WIDTH),
                            weak.clone(),
                        )
                        .into_any_element(),
                        cx,
                    ))
                    // DTR / RTS 在 egui 里是一对复选框，串口打开时仍然可用
                    // （改动即时下发 set_dtr / set_rts）。
                    .child(serial_row(
                        tr("DTR / RTS"),
                        h_flex()
                            .gap_2()
                            .child(Checkbox::new("dtr").checked(dtr).label("DTR").on_change(
                                cx.listener(|this, value, _, cx| {
                                    this.settings.serial.dtr = *value;
                                    this.apply_line_state(cx);
                                    cx.notify();
                                }),
                            ))
                            .child(Checkbox::new("rts").checked(rts).label("RTS").on_change(
                                cx.listener(|this, value, _, cx| {
                                    this.settings.serial.rts = *value;
                                    this.apply_line_state(cx);
                                    cx.notify();
                                }),
                            ))
                            .into_any_element(),
                        cx,
                    ))
                    // 打开按钮：egui 在网格后加 4px 间距；按钮宽度限制在
                    // 96..168 并整体左移 16px（右外边距是两倍偏移量，flex 的
                    // 居中会把外边距算进去）。
                    .child(
                        h_flex()
                            .id("serial-open-row")
                            .test_support()
                            .w_full()
                            .justify_center()
                            .mt(px(4.0))
                            .child(
                                div().w(px(open_button_width)).mr(px(32.0)).child(
                                    Button::new("toggle-port")
                                        .label(if is_open {
                                            tr("关闭串口")
                                        } else {
                                            tr("打开串口")
                                        })
                                        .when(is_open, |button| button.danger())
                                        .when(!is_open, |button| button.primary())
                                        .w_full()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.toggle_port();
                                            cx.notify();
                                        })),
                                ),
                            ),
                    )
                    // egui 的 ui.small：两行说明文字比正文小一号。
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(crate::i18n::tr1("当前: {}", summary))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(tr(
                                "提示: 串口打开时无法修改通信参数，需先关闭串口",
                            ))),
                    ),
            )
        })
        // 统计区同样可折叠：egui 版在这里是第二个 CollapsingHeader。
        .child(
            h_flex()
                .id("stats-section")
                .test_support()
                .w_full()
                .gap_1()
                .items_center()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stats_section_open = !this.stats_section_open;
                    cx.notify();
                }))
                .child(disclosure(stats_section_open, cx))
                .child(section_title(tr("统计"), cx)),
        )
        .when(stats_section_open, |this| {
            this.child(
                v_flex()
                    .id("stats-body")
                    .test_support()
                    .w_full()
                    .gap_2()
                    .child(info_row(tr("发送字节"), &app.tx_bytes.to_string(), cx))
                    .child(info_row(tr("接收字节"), &app.rx_bytes.to_string(), cx))
                    .child(info_row(
                        tr("显示行数"),
                        &app.log.row_count().to_string(),
                        cx,
                    ))
                    .child(info_row(
                        tr("缓存字节"),
                        &app.log.retained_bytes().to_string(),
                        cx,
                    ))
                    .child(field_row(
                        tr("最大保留行数"),
                        Input::new(&app.inputs.max_rows)
                            .w(px(96.0))
                            .into_any_element(),
                        cx,
                    ))
                    .child(field_row(
                        tr("最大缓存 (MB)"),
                        Input::new(&app.inputs.max_raw_mb)
                            .w(px(64.0))
                            .into_any_element(),
                        cx,
                    )),
            )
        })
        .into_any_element()
}

fn field_row(label: &'static str, control: AnyElement, cx: &mut Context<XcomApp>) -> AnyElement {
    h_flex()
        .w_full()
        .justify_between()
        .gap_2()
        .child(
            div()
                .text_sm()
                .flex_shrink_0()
                .child(SharedString::from(label)),
        )
        .child(div().flex_1().min_w(px(0.0)).child(control))
        .text_color(cx.theme().foreground)
        .into_any_element()
}

/// egui 的 serial_grid 行：固定宽度的标签列 + 紧跟其后的控件。
///
/// egui 的 `Grid::new("serial_grid")` 把标签放进自动宽度的第一列（最宽的
/// 是 `DTR / RTS`），控件列左对齐、不拉伸到面板边缘 —— 与 [`field_row`]
/// 的“标签靠左、控件撑满”排布不同。
fn serial_row(label: &'static str, control: AnyElement, cx: &Context<XcomApp>) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .w(px(SERIAL_LABEL_WIDTH))
                .flex_shrink_0()
                .text_sm()
                .child(SharedString::from(label)),
        )
        .child(div().flex_1().min_w(px(0.0)).child(control))
        .text_color(cx.theme().foreground)
        .into_any_element()
}

fn send_area(app: &mut XcomApp, _window: &mut Window, cx: &mut Context<XcomApp>) -> AnyElement {
    let open = app.link.is_open();
    let send = app.settings.send.clone();
    let weak = cx.entity().downgrade();
    let history = app.settings.send.history.clone();
    let history_weak = weak.clone();
    let mono = cx.theme().mono_font_family.clone();
    let text_font = crate::fonts::send_font(&app.settings.fonts, &mono);
    let file_progress = app
        .file_send
        .as_ref()
        .map(|send| (send.progress(), send.remaining(), send.path.clone()));

    // 工具行分成两组：左侧选项、右侧操作。操作组是一个整体 —— 放得下就靠右，
    // 放不下就整组换到下一行并左对齐（与 egui 版一致），而不是把组里的按钮
    // 拆散到两行。
    let mut options = h_flex()
        .gap_2()
        .items_center()
        .flex_shrink_0()
        .child(
            div()
                .text_sm()
                .font_bold()
                .child(SharedString::from(tr("发送区"))),
        )
        .child(div().w(px(1.0)).h(px(16.0)).bg(cx.theme().border))
        .child(
            Checkbox::new("send-hex")
                .label(tr("16进制发送"))
                .checked(send.hex)
                .on_change(cx.listener(|this, value, _, cx| {
                    this.settings.send.hex = *value;
                    this.mark_settings_dirty();
                    cx.notify();
                })),
        )
        .child(
            Checkbox::new("send-newline")
                .label(tr("发送新行"))
                .checked(send.append_newline)
                .on_change(cx.listener(|this, value, _, cx| {
                    this.settings.send.append_newline = *value;
                    this.mark_settings_dirty();
                    cx.notify();
                })),
        );
    if send.append_newline {
        options = options
            .child(
                Checkbox::new("send-cr")
                    .label("CR")
                    .checked(send.newline_cr)
                    .on_change(cx.listener(|this, value, _, cx| {
                        this.settings.send.newline_cr = *value;
                        this.mark_settings_dirty();
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("send-lf")
                    .label("LF")
                    .checked(send.newline_lf)
                    .on_change(cx.listener(|this, value, _, cx| {
                        this.settings.send.newline_lf = *value;
                        this.mark_settings_dirty();
                        cx.notify();
                    })),
            );
    }
    options = options
        .child(div().text_sm().child(SharedString::from(tr("定时间隔"))))
        .child(
            Input::new(&app.inputs.send_period)
                .w(px(88.0))
                .disabled(send.timed),
        )
        .child(
            Switch::new("timed")
                .label(tr("定时发送"))
                .checked(send.timed)
                .disabled(!open)
                .on_change(cx.listener(|this, value, _, cx| {
                    if this.link.is_open() {
                        this.settings.send.timed = *value;
                        this.last_timed_send = None;
                    } else if *value {
                        this.warn(tr("串口未打开"));
                    }
                    cx.notify();
                })),
        );

    let mut actions = h_flex()
        .id("send-actions")
        .test_support()
        .gap_2()
        .items_center()
        .flex_shrink_0()
        .child(
            Button::new("send")
                .label(tr("发送"))
                .primary()
                .disabled(!open)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.send_once();
                    cx.notify();
                })),
        )
        .child(
            Button::new("clear-send")
                .label(tr("清除发送"))
                .outline()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::ClearSend, window, cx);
                })),
        )
        .child(
            Button::new("history")
                .label(tr("历史"))
                .outline()
                .dropdown_menu(move |menu, _window, _cx| {
                    let mut menu = menu;
                    if history.is_empty() {
                        menu = menu.item(PopupMenuItem::new(tr("暂无历史")).disabled(true));
                    }
                    for (index, entry) in history.clone().into_iter().enumerate() {
                        let preview: String = entry.chars().take(40).collect();
                        menu = menu.item(PopupMenuItem::new(preview).on_click({
                            let weak = history_weak.clone();
                            move |_, window, cx| {
                                run_action(weak.clone(), Action::UseHistory(index), window, cx);
                            }
                        }));
                    }
                    menu
                }),
        )
        .child(
            Button::new("send-file")
                .label(tr("发送文件"))
                .outline()
                .disabled(!open)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::SendFile, window, cx);
                })),
        );
    if file_progress.is_some() {
        actions = actions.child(
            Button::new("abort-file")
                .label(tr("终止发送"))
                .danger()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::AbortFileSend, window, cx);
                })),
        );
    }

    // 左侧选项、右侧操作：操作组固定在行的右边缘，选项放不下时只在左侧
    // 内部换行，右边缘不会因此跑偏。
    let toolbar = h_flex()
        .id("send-toolbar")
        .test_support()
        .w_full()
        .gap_2()
        .items_center()
        .child(div().flex_1().min_w(px(0.0)).child(options.flex_wrap()))
        .child(actions);

    let mut area = v_flex()
        .id("send-area")
        .test_support()
        // 可调整面板是行方向的 flex 容器：不写 w_full 时整个发送区会按内容
        // 收缩，右边留出一大条空白（面板的 size_full 只负责自身）。
        .w_full()
        .h_full()
        .gap_2()
        .p_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(toolbar)
        // 文本框要占满剩下的高度，而不是停在自己那一行的高度上。
        .child(
            div()
                .id("send-input-box")
                .test_support()
                .flex_1()
                .min_h(px(56.0))
                .min_w(px(0.0))
                .child(
                    Textarea::new(&app.send_input)
                        .h(relative(1.0))
                        .font_family(text_font.family.clone())
                        .text_size(px(app.settings.fonts.send_size)),
                ),
        );

    if let Some((ratio, remaining, path)) = file_progress {
        area = area.child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    gpui_kit::component::progress::Progress::new("file-send-progress")
                        .value(ratio)
                        .flex_1(),
                )
                .child(div().text_sm().child(SharedString::from(crate::i18n::tr2(
                    "{} · 剩余 {} 字节",
                    path,
                    remaining,
                )))),
        );
    }
    area.into_any_element()
}

fn tool_panel(app: &mut XcomApp, window: &mut Window, cx: &mut Context<XcomApp>) -> AnyElement {
    v_flex()
        .id("tool-panel")
        .test_support()
        .w_full()
        .h_full()
        .border_l_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().sidebar)
        .child(
            h_flex()
                .p_2()
                .gap_1()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(tab_button(
                    "tab-multi",
                    tr("多条发送"),
                    app.side_tab == SideTab::Multi,
                    cx,
                    |this| this.side_tab = SideTab::Multi,
                ))
                .child(tab_button(
                    "tab-protocol",
                    tr("协议传输"),
                    app.side_tab == SideTab::Protocol,
                    cx,
                    |this| this.side_tab = SideTab::Protocol,
                )),
        )
        .child(match app.side_tab {
            SideTab::Multi => multi_panel(app, window, cx),
            SideTab::Protocol => protocol_panel(app, window, cx),
        })
        .into_any_element()
}

/// Drag payload for reordering a multi-send row.
#[derive(Clone)]
struct DragEntry(usize);

impl gpui_kit::Render for DragEntry {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded(cx.theme().radius)
            .bg(cx.theme().accent)
            .text_color(cx.theme().accent_foreground)
            .child(SharedString::from(format!("#{}", self.0)))
    }
}

/// The multi-send table's columns, in row order.
///
/// The fixed columns are laid out at these widths and the two text columns
/// (内容 / 备注) share what is left 62/38, which is how the egui version sizes
/// them; the header above a column uses the same number so the two never
/// drift apart.
const MULTI_ENABLE_W: f32 = 30.0;
const MULTI_INDEX_W: f32 = 24.0;
const MULTI_HEX_W: f32 = 30.0;
const MULTI_NEWLINE_W: f32 = 32.0;
const MULTI_DELAY_W: f32 = 58.0;
const MULTI_SEND_W: f32 = 52.0;
const MULTI_DELETE_W: f32 = 24.0;
const MULTI_COLUMN_GAP: f32 = 5.0;
const MULTI_TEXT_MIN: f32 = 32.0;
/// Header text per column; the last two carry no header.
const MULTI_HEADERS: [&str; 9] = ["启用", "#", "内容", "备注", "HEX", "新行", "延迟", "", ""];
const MULTI_ROW_HEIGHT: f32 = 24.0;

/// The narrowest the multi-send table's row can be and still show every
/// column: the seven fixed columns, the eight gaps between them, both text
/// columns at their floor, and the row's own horizontal padding.
///
/// A panel narrower than this pushes the delete button past the header row,
/// which is what the egui version prevents by measuring the table first and
/// giving the panel that width as its floor.
pub(crate) fn multi_table_min_width() -> f32 {
    let fixed = MULTI_ENABLE_W
        + MULTI_INDEX_W
        + MULTI_HEX_W
        + MULTI_NEWLINE_W
        + MULTI_DELAY_W
        + MULTI_SEND_W
        + MULTI_DELETE_W;
    fixed + MULTI_COLUMN_GAP * 8.0 + MULTI_TEXT_MIN * 2.0 + 8.0
}

/// One fixed-width header cell.
fn multi_header_cell(text: &'static str, width: f32, cx: &Context<XcomApp>) -> AnyElement {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_sm()
        .font_bold()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(tr(text)))
        .into_any_element()
}

/// One header cell that shares the remaining width with its sibling.
fn multi_header_cell_flex(text: &'static str, grow: f32, cx: &Context<XcomApp>) -> AnyElement {
    div()
        .flex_grow(grow)
        .flex_basis(px(0.0))
        .min_w(px(MULTI_TEXT_MIN))
        .text_sm()
        .font_bold()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(tr(text)))
        .into_any_element()
}

fn multi_panel(app: &mut XcomApp, _window: &mut Window, cx: &mut Context<XcomApp>) -> AnyElement {
    let running = app.multi_run_active();
    let open = app.link.is_open();
    let entries = app.settings.multi.entries.clone();
    let entries_len = entries.len();
    let loop_count = app.settings.multi.loop_count;
    let loop_enabled = app.settings.multi.loop_enabled;
    let cycles = app.multi_cycles;
    let sent = app.multi_run_sent();
    let countdown = app.multi_delay_countdown(std::time::Instant::now());
    let active = crate::theme::active_entry_colors(cx, app.settings.theme);
    let newline_cr = app.settings.multi.newline_cr;
    let newline_lf = app.settings.multi.newline_lf;
    let all_newline = !entries.is_empty() && entries.iter().all(|entry| entry.newline);
    let weak = cx.entity().downgrade();
    let mut toolbar = h_flex().gap_1().flex_wrap();

    toolbar = toolbar
        .child(
            Button::new("multi-add")
                .label(tr("添加条目"))
                .tooltip(tr("在列表末尾添加一条空条目"))
                .small()
                .outline()
                .disabled(running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiAdd, window, cx);
                })),
        )
        .child(
            Button::new("multi-import")
                .label(tr("导入条目"))
                .small()
                .outline()
                .disabled(running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiImport, window, cx);
                })),
        )
        .child(
            Button::new("multi-export")
                .label(tr("导出条目"))
                .small()
                .outline()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiExport, window, cx);
                })),
        )
        .child(
            Button::new("multi-send-checked")
                .label(tr("发送已勾选"))
                .tooltip(tr("按顺序发送整个列表里所有已勾选且非空的条目"))
                .small()
                .primary()
                .disabled(!open || running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiSendChecked, window, cx);
                })),
        )
        .child(
            Switch::new("multi-loop")
                .label(tr("循环发送"))
                .checked(loop_enabled)
                .disabled(!open)
                .on_change(cx.listener(|this, value, _, cx| {
                    if this.multi_run_active() {
                        this.stop_multi_run(Level::Info, tr("已停止循环发送"));
                    } else if *value && this.link.is_open() {
                        this.settings.multi.loop_enabled = true;
                        this.start_multi_run(true, std::time::Instant::now());
                    } else if *value {
                        this.warn(tr("串口未打开"));
                    }
                    cx.notify();
                })),
        );

    let loop_row = h_flex()
        .gap_2()
        .items_center()
        .flex_wrap()
        .child(div().text_sm().child(SharedString::from(tr("循环次数"))))
        .child(
            Input::new(&app.inputs.loop_count)
                .w(px(72.0))
                .disabled(loop_count.is_none()),
        )
        .child(
            Switch::new("multi-forever")
                .label(tr("无限循环"))
                .checked(loop_count.is_none())
                .on_change(cx.listener(|this, value, window, cx| {
                    let next = if *value {
                        None
                    } else {
                        Some(this.settings.multi.loop_count.unwrap_or(10))
                    };
                    this.settings.multi.loop_count = next;
                    let text = next.map(|n| n.to_string()).unwrap_or_default();
                    this.inputs
                        .loop_count
                        .update(cx, |state, cx| state.set_value(text, window, cx));
                    if !this.multi_run_active() {
                        this.multi_cycles = 0;
                    }
                    this.mark_settings_dirty();
                    cx.notify();
                })),
        )
        .child(div().text_sm().child(SharedString::from(tr("多条换行"))))
        .child(
            Checkbox::new("multi-cr")
                .label("CR")
                .checked(newline_cr)
                .on_change(cx.listener(|this, value, _, cx| {
                    this.settings.multi.newline_cr = *value;
                    this.mark_settings_dirty();
                    cx.notify();
                })),
        )
        .child(
            Checkbox::new("multi-lf")
                .label("LF")
                .checked(newline_lf)
                .on_change(cx.listener(|this, value, _, cx| {
                    this.settings.multi.newline_lf = *value;
                    this.mark_settings_dirty();
                    cx.notify();
                })),
        )
        .child(
            Checkbox::new("multi-newline-all")
                .label(tr("全部加新行"))
                .checked(all_newline)
                .disabled(entries.is_empty() || running)
                .on_change(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiNewlineAll, window, cx);
                })),
        );

    let mut rows: Vec<AnyElement> = Vec::with_capacity(entries_len);
    for index in 0..entries_len {
        let entry = &entries[index];
        let inputs = &app.multi_inputs[index];
        let is_active = sent.is_some_and(|sent| sent.index == index);
        let row = h_flex()
            .id(("multi-drop", index))
            .on_drop({
                let weak = weak.clone();
                move |drag: &DragEntry, window, cx| {
                    run_action(
                        weak.clone(),
                        Action::MultiMove {
                            from: drag.0,
                            to: index,
                        },
                        window,
                        cx,
                    );
                }
            })
            .w_full()
            .h(px(MULTI_ROW_HEIGHT))
            .px_1()
            .gap(px(MULTI_COLUMN_GAP))
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            // 当前正在发送的行：整行淡色 + 左侧实心竖条（画在斑马纹之上）。
            .when(is_active, |this| {
                this.bg(active.wash).border_l_2().border_color(active.bar)
            })
            .when(!is_active && index % 2 == 1, |this| {
                this.bg(cx.theme().muted.opacity(0.25))
            })
            // 启用
            .child(
                div().w(px(MULTI_ENABLE_W)).flex_shrink_0().child(
                    Checkbox::new(("multi-enabled", index))
                        .checked(entry.enabled)
                        .on_change(cx.listener(move |this, value, _, cx| {
                            if let Some(entry) = this.settings.multi.entries.get_mut(index) {
                                entry.enabled = *value;
                                this.mark_settings_dirty();
                            }
                            cx.notify();
                        })),
                ),
            )
            // #
            .child(if running {
                div()
                    .w(px(MULTI_INDEX_W))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(index.to_string()))
                    .into_any_element()
            } else {
                div()
                    .id(("multi-drag", index))
                    .w(px(MULTI_INDEX_W))
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(index.to_string()))
                    .on_drag(DragEntry(index), |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .into_any_element()
            })
            // 内容 / 备注：两列平分剩余宽度，62 / 38
            .child(
                div()
                    .flex_grow(62.0)
                    .flex_basis(px(0.0))
                    .min_w(px(MULTI_TEXT_MIN))
                    .child(Input::new(&inputs.text).w_full()),
            )
            .child(
                div()
                    .flex_grow(38.0)
                    .flex_basis(px(0.0))
                    .min_w(px(MULTI_TEXT_MIN))
                    .child(Input::new(&inputs.note).w_full()),
            )
            // HEX
            .child(
                div().w(px(MULTI_HEX_W)).flex_shrink_0().child(
                    Checkbox::new(("multi-hex", index))
                        .checked(entry.hex)
                        .on_change(cx.listener(move |this, value, _, cx| {
                            if let Some(entry) = this.settings.multi.entries.get_mut(index) {
                                entry.hex = *value;
                                this.mark_settings_dirty();
                            }
                            cx.notify();
                        })),
                ),
            )
            // 新行
            .child(
                div().w(px(MULTI_NEWLINE_W)).flex_shrink_0().child(
                    Checkbox::new(("multi-newline", index))
                        .checked(entry.newline)
                        .on_change(cx.listener(move |this, value, _, cx| {
                            if let Some(entry) = this.settings.multi.entries.get_mut(index) {
                                entry.newline = *value;
                                this.mark_settings_dirty();
                            }
                            cx.notify();
                        })),
                ),
            )
            // 延迟（倒计时进度条画在输入框下方）
            .child(
                v_flex()
                    .w(px(MULTI_DELAY_W))
                    .flex_shrink_0()
                    .gap(px(1.0))
                    .child(Input::new(&inputs.delay).w_full())
                    .when(
                        countdown.is_some_and(|countdown| countdown.index == index),
                        |this| {
                            let remaining = countdown.map_or(0.0, |c| c.remaining);
                            this.child(
                                div()
                                    .h(px(3.0))
                                    .w_full()
                                    .rounded_full()
                                    .bg(active.bar.opacity(0.3))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(remaining))
                                            .rounded_full()
                                            .bg(active.bar),
                                    ),
                            )
                        },
                    ),
            )
            // 发送
            .child(
                div().w(px(MULTI_SEND_W)).flex_shrink_0().child(
                    Button::new(("multi-send", index))
                        .label(tr("发送"))
                        .tooltip(if entry.newline {
                            tr("发送此条目（含新行）")
                        } else {
                            tr("发送此条目（不含新行）")
                        })
                        .small()
                        .w_full()
                        .disabled(!open)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.apply_action(Action::MultiSendOne(index), window, cx);
                        })),
                ),
            )
            // 删除
            .child(
                div().w(px(MULTI_DELETE_W)).flex_shrink_0().child(
                    Button::new(("multi-delete", index))
                        .label("×")
                        .tooltip(tr("删除条目"))
                        .small()
                        .w_full()
                        .ghost()
                        .disabled(running)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.apply_action(Action::MultiRemove(index), window, cx);
                        })),
                ),
            );
        rows.push(row.into_any_element());
    }

    let status = h_flex()
        .gap_2()
        .p_2()
        .text_sm()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(SharedString::from(if running && loop_enabled {
            tr("循环中")
        } else {
            tr("未循环")
        }))
        .child(SharedString::from(match loop_count {
            None => tr("不限次数").to_owned(),
            Some(limit) => crate::i18n::tr2("已完成 {} / {} 次", cycles, limit),
        }))
        .child(SharedString::from(match sent {
            Some(sent) => crate::i18n::tr3(
                "正在发送 #{} ({} / {})",
                sent.index,
                sent.ordinal,
                sent.round_len,
            ),
            None => tr("空闲").to_owned(),
        }));

    v_flex()
        .flex_1()
        .min_h(px(0.0))
        .child(div().p_2().child(toolbar))
        .child(div().px_2().pb_2().child(loop_row))
        .child(
            div()
                .id("multi-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                // 表头与行共用同一组宽度，两者不会走样。
                .child(
                    h_flex()
                        .id("multi-headers")
                        .test_support()
                        .w_full()
                        .px_1()
                        .gap(px(MULTI_COLUMN_GAP))
                        .h(px(MULTI_ROW_HEIGHT))
                        .items_center()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(multi_header_cell(MULTI_HEADERS[0], MULTI_ENABLE_W, cx))
                        .child(multi_header_cell(MULTI_HEADERS[1], MULTI_INDEX_W, cx))
                        .child(multi_header_cell_flex(MULTI_HEADERS[2], 62.0, cx))
                        .child(multi_header_cell_flex(MULTI_HEADERS[3], 38.0, cx))
                        .child(multi_header_cell(MULTI_HEADERS[4], MULTI_HEX_W, cx))
                        .child(multi_header_cell(MULTI_HEADERS[5], MULTI_NEWLINE_W, cx))
                        .child(multi_header_cell(MULTI_HEADERS[6], MULTI_DELAY_W, cx))
                        .child(div().w(px(MULTI_SEND_W)).flex_shrink_0())
                        .child(div().w(px(MULTI_DELETE_W)).flex_shrink_0()),
                )
                .children(rows)
                .when(entries_len == 0, |this| {
                    this.child(
                        div()
                            .p_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(tr("暂无条目，点击上方“添加条目”"))),
                    )
                }),
        )
        .child(status)
        .into_any_element()
}

fn protocol_panel(
    app: &mut XcomApp,
    _window: &mut Window,
    cx: &mut Context<XcomApp>,
) -> AnyElement {
    let config = app.settings.protocol.clone();
    let running = app.protocol.is_running();
    let open = app.link.is_open();
    let weak = cx.entity().downgrade();
    let mono = cx.theme().mono_font_family.clone();
    let text_font = crate::fonts::send_font(&app.settings.fonts, &mono);
    let checksum_items: Vec<(String, Choice, bool)> = crate::protocol::ChecksumKind::ALL
        .into_iter()
        .map(|kind| {
            (
                tr(kind.label()).to_owned(),
                Choice::Checksum(kind),
                kind == config.checksum,
            )
        })
        .collect();
    let warning = config.period_warning(app.settings.serial.baud);
    let progress = app.protocol_progress;
    let log = app
        .protocol_log
        .iter()
        .map(|line| (line.level, line.text.clone(), line.stamp.clone()))
        .collect::<Vec<_>>();
    let task = app.protocol.task_kind().map(|kind| tr(kind.label()));

    let mut actions = h_flex()
        .gap_1()
        .flex_wrap()
        .child(
            Button::new("proto-single")
                .label(tr("单次发送"))
                .small()
                .primary()
                .disabled(!open || running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::ProtocolSingle, window, cx);
                })),
        )
        .child(
            Button::new("proto-auto")
                .label(tr("自动发送"))
                .small()
                .outline()
                .disabled(!open || running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::ProtocolAuto, window, cx);
                })),
        )
        .child(
            Button::new("proto-file")
                .label(tr("文件发送"))
                .small()
                .outline()
                .disabled(!open || running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::ProtocolFile, window, cx);
                })),
        )
        .child(
            Button::new("proto-stop")
                .label(tr("停止"))
                .small()
                .danger()
                .disabled(!running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::ProtocolStop, window, cx);
                })),
        );
    if let Some(task) = task {
        actions = actions.child(
            div()
                .text_sm()
                .text_color(cx.theme().info)
                .child(SharedString::from(crate::i18n::tr1("正在{}", task))),
        );
    }

    let mut layout = v_flex()
        .id("protocol-panel")
        .flex_1()
        .min_h(px(0.0))
        .gap_2()
        .p_2()
        .child(field_row(
            tr("地址"),
            Input::new(&app.inputs.protocol_address)
                .w(px(56.0))
                .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("功能"),
            Input::new(&app.inputs.protocol_function)
                .w(px(56.0))
                .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("校验"),
            choice_button(
                "checksum",
                tr(config.checksum.label()),
                checksum_items,
                false,
                weak.clone(),
            )
            .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("帧周期"),
            Input::new(&app.inputs.protocol_period)
                .w(px(88.0))
                .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("最大重发"),
            Input::new(&app.inputs.protocol_retries)
                .w(px(56.0))
                .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("最大数据长度"),
            Input::new(&app.inputs.protocol_data_len)
                .w(px(56.0))
                .into_any_element(),
            cx,
        ))
        .child(
            h_flex()
                .gap_2()
                .flex_wrap()
                .child(
                    Checkbox::new("proto-auto-send")
                        .label(tr("自动发送"))
                        .checked(config.auto_send)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings.protocol.auto_send = *value;
                            this.protocol.set_config(this.settings.protocol.clone());
                            this.mark_settings_dirty();
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("proto-raw")
                        .label(tr("显示原始帧"))
                        .checked(config.show_raw)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings.protocol.show_raw = *value;
                            this.protocol.set_config(this.settings.protocol.clone());
                            this.mark_settings_dirty();
                            cx.notify();
                        })),
                )
                .child(
                    Checkbox::new("proto-wrap")
                        .label(tr("自动换行"))
                        .checked(config.auto_wrap)
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings.protocol.auto_wrap = *value;
                            this.protocol.set_config(this.settings.protocol.clone());
                            this.mark_settings_dirty();
                            cx.notify();
                        })),
                ),
        );
    if let Some(warning) = warning {
        layout = layout.child(
            div()
                .text_sm()
                .text_color(cx.theme().warning)
                .child(SharedString::from(warning)),
        );
    }
    layout = layout
        .child(div().text_sm().child(SharedString::from(tr("发送数据"))))
        .child(
            Textarea::new(&app.protocol_input)
                .h(px(84.0))
                .w_full()
                .font_family(text_font.family.clone())
                .text_size(px(app.settings.fonts.send_size)),
        )
        .child(actions);
    if let Some((sent, total)) = progress {
        let ratio = if total == 0 {
            0.0
        } else {
            sent as f32 / total as f32
        };
        layout = layout.child(
            gpui_kit::component::progress::Progress::new("protocol-progress")
                .value(ratio)
                .w_full(),
        );
    }
    layout = layout
        .child(
            h_flex()
                .justify_between()
                .child(div().text_sm().child(SharedString::from(tr("响应区"))))
                .child(
                    Button::new("proto-clear")
                        .label(tr("清除"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.protocol_log.clear();
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("protocol-log")
                .flex_1()
                .min_h(px(64.0))
                .overflow_y_scroll()
                .children(log.into_iter().map(|(level, text, stamp)| {
                    h_flex()
                        .gap_2()
                        .items_start()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(stamp)),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(level_color(level, cx))
                                .child(SharedString::from(text)),
                        )
                        .into_any_element()
                }))
                .when(app.protocol_log.is_empty(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(tr("暂无协议交互记录"))),
                    )
                }),
        );
    layout.into_any_element()
}

fn level_color(level: Level, cx: &Context<XcomApp>) -> gpui_kit::Hsla {
    match level {
        Level::Error => cx.theme().danger,
        Level::Warning => cx.theme().warning,
        Level::Success => cx.theme().success,
        Level::Info => cx.theme().foreground,
    }
}

fn tab_button(
    id: &'static str,
    label: &'static str,
    selected: bool,
    cx: &mut Context<XcomApp>,
    apply: fn(&mut XcomApp),
) -> AnyElement {
    Button::new(id)
        .label(label)
        .small()
        .selected(selected)
        .on_click(cx.listener(move |this, _, _, cx| {
            apply(this);
            cx.notify();
        }))
        .into_any_element()
}

fn status_bar(app: &mut XcomApp, cx: &mut Context<XcomApp>) -> AnyElement {
    let theme = cx.theme().clone();
    h_flex()
        .w_full()
        .h(px(30.0))
        .px_3()
        .gap_3()
        .items_center()
        .border_t_1()
        .border_color(theme.border)
        .bg(theme.title_bar)
        .text_sm()
        .child(div().child(if app.link.is_open() {
            SharedString::from(format!(
                "{} [{}]",
                app.settings.serial.port,
                app.settings.serial.summary()
            ))
        } else {
            SharedString::from(tr("未连接"))
        }))
        .child(div().child(SharedString::from(format!(
            "{} {}",
            tr("编码"),
            app.encoding_label()
        ))))
        // Auto-detection context: how much has been analysed so far.  It sits
        // in the status bar rather than the Encoding submenu so the menu stays
        // a list of choices.
        .when(app.settings.encoding.is_none(), |this| {
            this.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(crate::i18n::tr1(
                        "已分析 {} 字节",
                        app.auto.total(),
                    ))),
            )
        })
        .child(div().child(SharedString::from(format!(
            "{} {}  {} {}",
            tr("发送:"),
            app.tx_bytes,
            tr("接收:"),
            app.rx_bytes
        ))))
        .child(
            h_flex().gap_2().children(
                [
                    ("CTS", app.signals.cts),
                    ("DSR", app.signals.dsr),
                    ("DCD", app.signals.dcd),
                    ("RI", app.signals.ri),
                ]
                .into_iter()
                .map(|(name, on)| {
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(div().size_2().rounded_full().bg(if on {
                            cx.theme().success
                        } else {
                            cx.theme().muted_foreground
                        }))
                        .child(div().text_sm().child(SharedString::from(name)))
                }),
            ),
        )
        .when(app.show_paused, |this| {
            this.child(
                div()
                    .text_color(cx.theme().warning)
                    .child(SharedString::from(crate::i18n::tr1(
                        "已暂停（缓存 {} 字节）",
                        app.pending_rx_len(),
                    ))),
            )
        })
        .child(div().flex_1())
        .when_some(app.latest_message(), |this, message| {
            this.child(
                div()
                    .text_color(level_color(message.level, cx))
                    .child(SharedString::from(message.text.clone())),
            )
        })
        .child(div().child(SharedString::from(
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        )))
        .into_any_element()
}

fn section_title(text: &'static str, cx: &mut Context<XcomApp>) -> AnyElement {
    div()
        .text_sm()
        .font_bold()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(text))
        .into_any_element()
}

/// The disclosure triangle of a collapsible section header, pointing down when
/// the section is open the way egui's `CollapsingHeader` does.
fn disclosure(open: bool, cx: &mut Context<XcomApp>) -> AnyElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(if open { "▾" } else { "▸" }))
        .into_any_element()
}

fn info_row(label: &'static str, value: &str, cx: &mut Context<XcomApp>) -> AnyElement {
    h_flex()
        .w_full()
        .justify_between()
        .child(div().text_sm().child(SharedString::from(label)))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(value.to_owned())),
        )
        .into_any_element()
}
