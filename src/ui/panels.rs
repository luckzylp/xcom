//! Every visible panel of the application.
//!
//! The shell is the same as the original tool: menu bar on top, configuration
//! on the left, receive log above the send area in the middle, the multi-send /
//! protocol panel on the right, and a status bar along the bottom.

use gpui_kit::base::{InteractiveElementExt as _, StyledExt as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _,
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
    MouseButton, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, WeakEntity, Window, div, px, relative,
};

use crate::app::{Action, Choice, Level, SideTab, XcomApp};
use crate::i18n::tr;

/// Compose the whole window.
/// The in-window title bar used on Linux when the compositor gives the
/// window client-side decorations.
///
/// It follows the desktop's button layout (left or right), supports dragging
/// the window by the empty title area, double-click to maximise/restore and
/// the desktop window menu on right-click.
fn title_bar(app: &XcomApp, window: &mut Window, cx: &mut Context<XcomApp>) -> AnyElement {
    #[cfg(target_os = "linux")]
    {
        if !matches!(
            window.window_decorations(),
            gpui_kit::Decorations::Client { .. }
        ) {
            return div().into_any_element();
        }

        let maximized = window.is_maximized();
        let layout = app.button_layout;
        log::debug!(
            "title bar: decorations={:?} left={:?} right={:?}",
            window.window_decorations(),
            layout.left,
            layout.right
        );

        let left_controls: Vec<AnyElement> = layout
            .left
            .into_iter()
            .enumerate()
            .filter_map(|(index, button)| window_control(button?, index, "left", maximized))
            .collect();
        let right_controls: Vec<AnyElement> = layout
            .right
            .into_iter()
            .enumerate()
            .filter_map(|(index, button)| window_control(button?, index, "right", maximized))
            .collect();

        let drag = div()
            .id("title-drag")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .px_3()
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
            );

        return h_flex()
            .w_full()
            .h(px(34.0))
            .items_center()
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().title_bar_border)
            .bg(cx.theme().title_bar)
            .child(
                h_flex()
                    .h_full()
                    .items_center()
                    .flex_shrink_0()
                    .children(left_controls),
            )
            .child(drag)
            .child(
                h_flex()
                    .h_full()
                    .items_center()
                    .flex_shrink_0()
                    .children(right_controls),
            )
            .into_any_element();
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, cx);
        div().into_any_element()
    }
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

#[cfg(target_os = "linux")]
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
                .size_range(px(1.0)..px(760.0))
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
                        if this.settings.show_tool_panel {
                            if width < 120.0 {
                                this.settings.show_tool_panel = false;
                                this.mark_settings_dirty();
                                cx.notify();
                            } else if width > 140.0
                                && (this.settings.right_panel_width - width).abs() > 0.5
                            {
                                this.settings.right_panel_width = width;
                                this.mark_settings_dirty();
                            }
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
        .child(title_bar(app, window, cx))
        .child(menu_bar(app, cx))
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

/// A dropdown button whose items apply one [`Choice`].
pub(crate) fn choice_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    items: Vec<(String, Choice, bool)>,
    weak: WeakEntity<XcomApp>,
) -> impl IntoElement {
    Button::new(id)
        .label(label)
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
        .w_full()
        .h(px(32.0))
        .px_2()
        .gap_1()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().title_bar)
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
                    .submenu(tr("显示"), window, cx, move |menu, _, _| {
                        menu.item(check_item(
                            tr("16进制显示"),
                            display.mode == crate::logstore::ViewMode::Hex,
                            Action::ToggleHex,
                            display_weak.clone(),
                        ))
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
    let mut port_items: Vec<(String, Choice, bool)> = ports
        .iter()
        .map(|port| {
            (
                port.display(),
                Choice::Port(port.name.clone()),
                port.name == current_port,
            )
        })
        .collect();
    if !ports.iter().any(|port| port.name == current_port) && !current_port.is_empty() {
        port_items.push((
            current_port.clone(),
            Choice::Port(current_port.clone()),
            true,
        ));
    }
    if port_items.is_empty() {
        port_items.push((
            tr("（无可用串口）").to_owned(),
            Choice::Port(String::new()),
            true,
        ));
    }
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
        .child(section_title(tr("串口设置"), cx))
        .child(field_row(
            tr("串口"),
            choice_button("port", port_label, port_items, weak.clone()).into_any_element(),
            cx,
        ))
        .child(
            h_flex()
                .gap_1()
                .child(
                    Button::new("refresh-ports")
                        .label("↻")
                        .tooltip(tr("刷新"))
                        .small()
                        .ghost()
                        .disabled(is_open)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ports.poll();
                            this.info(tr("已刷新串口列表"));
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("custom-baud")
                        .label(tr("自定义波特率…"))
                        .small()
                        .outline()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_custom_baud(window, cx);
                        })),
                ),
        )
        .child(field_row(
            tr("波特率"),
            choice_button(
                "baud",
                format!("{}", app.settings.serial.baud),
                baud_items,
                weak.clone(),
            )
            .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("数据位"),
            choice_button(
                "data-bits",
                app.settings.serial.data_bits.label(),
                data_bits,
                weak.clone(),
            )
            .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("停止位"),
            choice_button(
                "stop-bits",
                app.settings.serial.stop_bits.label(),
                stop_bits,
                weak.clone(),
            )
            .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("校验"),
            choice_button(
                "parity",
                tr(app.settings.serial.parity.label()),
                parity,
                weak.clone(),
            )
            .into_any_element(),
            cx,
        ))
        .child(field_row(
            tr("流控"),
            choice_button(
                "flow",
                tr(app.settings.serial.flow.label()),
                flow,
                weak.clone(),
            )
            .into_any_element(),
            cx,
        ))
        .child(
            h_flex()
                .gap_2()
                .child(
                    Switch::new("dtr")
                        .checked(dtr)
                        .label("DTR")
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings.serial.dtr = *value;
                            this.apply_line_state(cx);
                            cx.notify();
                        })),
                )
                .child(
                    Switch::new("rts")
                        .checked(rts)
                        .label("RTS")
                        .on_change(cx.listener(|this, value, _, cx| {
                            this.settings.serial.rts = *value;
                            this.apply_line_state(cx);
                            cx.notify();
                        })),
                ),
        )
        .child(
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
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(summary)),
        )
        .child(section_title(tr("统计"), cx))
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
        ))
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

    let mut toolbar = h_flex()
        .gap_2()
        .items_center()
        .flex_wrap()
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
        toolbar = toolbar
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
    toolbar = toolbar
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
        )
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
        toolbar = toolbar.child(
            Button::new("abort-file")
                .label(tr("终止发送"))
                .danger()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::AbortFileSend, window, cx);
                })),
        );
    }

    let mut area = v_flex()
        .h_full()
        .gap_2()
        .p_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(toolbar)
        .child(
            Textarea::new(&app.send_input)
                .flex_1()
                .min_h(px(56.0))
                .font_family(text_font.family.clone())
                .text_size(px(app.settings.fonts.send_size)),
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
                .small()
                .outline()
                .disabled(running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiAdd, window, cx);
                })),
        )
        .child(
            Button::new("multi-import")
                .label(tr("导入"))
                .small()
                .outline()
                .disabled(running)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiImport, window, cx);
                })),
        )
        .child(
            Button::new("multi-export")
                .label(tr("导出"))
                .small()
                .outline()
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_action(Action::MultiExport, window, cx);
                })),
        )
        .child(
            Button::new("multi-send-checked")
                .label(tr("发送已勾选"))
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
        let row = v_flex()
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
            .p_2()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(index % 2 == 1, |this| {
                this.bg(cx.theme().muted.opacity(0.25))
            })
            .when(is_active, |this| this.bg(active.wash))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Checkbox::new(("multi-enabled", index))
                            .checked(entry.enabled)
                            .on_change(cx.listener(move |this, value, _, cx| {
                                if let Some(entry) = this.settings.multi.entries.get_mut(index) {
                                    entry.enabled = *value;
                                    this.mark_settings_dirty();
                                }
                                cx.notify();
                            })),
                    )
                    .child(if running {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .w(px(20.0))
                            .child(SharedString::from(index.to_string()))
                            .into_any_element()
                    } else {
                        div()
                            .id(("multi-drag", index))
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .w(px(20.0))
                            .child(SharedString::from(index.to_string()))
                            .on_drag(DragEntry(index), |drag, _, _, cx| cx.new(|_| drag.clone()))
                            .into_any_element()
                    })
                    .child(Input::new(&inputs.text).flex_1().min_w(px(60.0)))
                    .child(
                        Checkbox::new(("multi-hex", index))
                            .label("HEX")
                            .checked(entry.hex)
                            .on_change(cx.listener(move |this, value, _, cx| {
                                if let Some(entry) = this.settings.multi.entries.get_mut(index) {
                                    entry.hex = *value;
                                    this.mark_settings_dirty();
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Checkbox::new(("multi-newline", index))
                            .label(tr("新行"))
                            .checked(entry.newline)
                            .on_change(cx.listener(move |this, value, _, cx| {
                                if let Some(entry) = this.settings.multi.entries.get_mut(index) {
                                    entry.newline = *value;
                                    this.mark_settings_dirty();
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(("multi-delete", index))
                            .label("×")
                            .tooltip(tr("删除条目"))
                            .small()
                            .ghost()
                            .disabled(running)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_action(Action::MultiRemove(index), window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Input::new(&inputs.note).flex_1().min_w(px(40.0)))
                    .child(
                        v_flex()
                            .w(px(84.0))
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
                    .child(
                        Button::new(("multi-send", index))
                            .label(tr("发送"))
                            .small()
                            .disabled(!open)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_action(Action::MultiSendOne(index), window, cx);
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
                .children(rows),
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
                })),
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
