//! Application dialogs: about, shortcuts, fonts, receive colours, keyword
//! highlighting, log files, custom baud rates and the reset confirmation.
//!
//! Every dialog is opened through the window's overlay layer, so it gets the
//! component system's focus trap, Escape dismissal and focus restoration.  The
//! controls inside read the retained application entity through a weak handle;
//! a change updates the settings and the dialog layer redraws.

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{DialogClose, DialogFooter},
    h_flex,
    input::Input,
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Context, IntoElement, ParentElement as _, SharedString, Styled as _, WeakEntity, Window, div,
    px,
};

use crate::app::{Action, Choice, XcomApp};
use crate::i18n::tr;

fn footer() -> DialogFooter {
    DialogFooter::new()
        .justify_end()
        .gap_2()
        .child(DialogClose::new().trigger(|button| button.label(tr("关闭")).outline()))
}

fn field(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_sm().child(SharedString::from(label)))
        .child(control)
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

impl XcomApp {
    pub(crate) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let version = env!("CARGO_PKG_VERSION");
        window.open_dialog(cx, move |dialog, _, _cx| {
            dialog
                .title(tr("关于 XCOM"))
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
                )
                .footer(footer())
        });
    }

    pub(crate) fn open_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, move |dialog, _, cx| {
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
            dialog
                .title(tr("快捷键"))
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
                )
                .footer(footer())
        });
    }

    pub(crate) fn open_font_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
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
            dialog
                .title(tr("字体设置"))
                .width(px(560.0))
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
                                            (app.settings.line_spacing - value as f32).abs() < 0.01,
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                                weak.clone(),
                            ),
                        )),
                )
                .footer(footer())
        });
    }

    pub(crate) fn open_log_colors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let colors = app.settings.log_colors;
            let palette = crate::theme::log_palette(cx, app.settings.theme, colors);
            dialog
                .title(tr("接收区配色"))
                .width(px(480.0))
                .child(
                    v_flex()
                        .gap_3()
                        .child(onoff(
                            weak.clone(),
                            colors.enabled,
                            tr("启用自定义配色"),
                            |this, value| this.settings.log_colors.enabled = value,
                        ))
                        .child(field(tr("前景色"), Input::new(&app.inputs.log_fg)))
                        .child(field(tr("背景色"), Input::new(&app.inputs.log_bg)))
                        .child(
                            div()
                                .p_3()
                                .rounded(cx.theme().radius)
                                .bg(palette.background)
                                .text_color(palette.foreground)
                                .child(SharedString::from("-> 12:34:56.789  AT+GMR  <- AT")),
                        ),
                )
                .footer(footer())
        });
    }

    pub(crate) fn open_log_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
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
            dialog
                .title(tr("日志文件"))
                .width(px(560.0))
                .child(
                    v_flex()
                        .gap_3()
                        .child(field(tr("保存目录"), Input::new(&app.inputs.log_dir)))
                        .child(field(tr("文件名格式"), Input::new(&app.inputs.log_pattern)))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(tr(
                                    "可用 %Y %y %M %D %h %m %s %S %f %H（串口名）%%",
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
                        ),
                )
                .footer(footer())
        });
    }

    pub(crate) fn open_custom_baud(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let rates = app.settings.serial.custom_bauds.clone();
            let input = app.inputs.custom_baud.clone();
            dialog
                .title(tr("自定义波特率"))
                .width(px(420.0))
                .child(
                    v_flex()
                        .gap_3()
                        .child(field(
                            tr("添加波特率"),
                            h_flex().gap_2().child(Input::new(&input)).child(
                                Button::new("baud-add").label(tr("添加")).on_click({
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
                                }),
                            ),
                        ))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
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
                                        .justify_between()
                                        .child(div().child(SharedString::from(rate.to_string())))
                                        .child(
                                            Button::new(("baud-remove", rate))
                                                .label(tr("删除"))
                                                .ghost()
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
                )
                .footer(footer())
        });
    }

    pub(crate) fn open_highlight(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rebuild_highlight_inputs(window, cx);
        let weak = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(entity) = weak.upgrade() else {
                return dialog;
            };
            let app = entity.read(cx);
            let rules = app.settings.highlights.rules.clone();
            dialog
                .title(tr("关键字高亮"))
                .width(px(760.0))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .justify_between()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(tr(
                                            "每条规则可设为字面量或正则表达式；行尾的 * 表示正则。",
                                        ))),
                                )
                                .child(
                                    Button::new("hl-add")
                                        .label(tr("添加规则"))
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
                                ),
                        )
                        .child(if rules.is_empty() {
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(tr("暂无高亮规则")))
                                .into_any_element()
                        } else {
                            v_flex()
                                .gap_1()
                                .children(rules.iter().enumerate().map(|(index, rule)| {
                                    let inputs = &app.highlight_inputs[index];
                                    let invalid = rule.regex
                                        && crate::highlight::validate_regex(&rule.text).is_err();
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Switch::new(("hl-enabled", index))
                                                .checked(rule.enabled)
                                                .on_change({
                                                    let weak = weak.clone();
                                                    move |next, _, cx| {
                                                        let next = *next;
                                                        let _ = weak.update(cx, |this, cx| {
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
                                                        });
                                                    }
                                                }),
                                        )
                                        .child(Input::new(&inputs.text).flex_1())
                                        .child(Input::new(&inputs.color).w(px(88.0)))
                                        .child(
                                            Checkbox::new(("hl-regex", index))
                                                .label("*")
                                                .checked(rule.regex)
                                                .on_change({
                                                    let weak = weak.clone();
                                                    move |next, _, cx| {
                                                        let next = *next;
                                                        let _ = weak.update(cx, |this, cx| {
                                                            if let Some(rule) = this
                                                                .settings
                                                                .highlights
                                                                .rules
                                                                .get_mut(index)
                                                            {
                                                                rule.regex = next;
                                                                this.mark_settings_dirty();
                                                            }
                                                            cx.notify();
                                                        });
                                                    }
                                                }),
                                        )
                                        .when(invalid, |this| {
                                            this.child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().danger)
                                                    .child(SharedString::from(tr("正则无效"))),
                                            )
                                        })
                                        .child(
                                            Button::new(("hl-remove", index))
                                                .label("×")
                                                .ghost()
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
                                        )
                                }))
                                .into_any_element()
                        }),
                )
                .footer(footer())
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
