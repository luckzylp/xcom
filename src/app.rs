//! Application state, the serial/timer pump, and the top-level view.
//!
//! The UI framework is gpui-kit: this entity is the retained application model,
//! `render` composes the panels, and a background tick drives serial polling,
//! timed sends, the multi-send sequence and the protocol state machine.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, Styled as _, Subscription, Task, Window, div, px,
};
use gpui_kit::{KeyBinding, actions};

use crate::encoding::{AutoDetector, EncodingChoice, EncodingId};
use crate::i18n::tr;
use crate::logfile::LogFile;
use crate::logstore::LogStore;
use crate::protocol::{ProtocolEngine, ProtocolEvent};
use crate::serial::{PortWatcher, SerialEvent, SerialLink, Signals};
use crate::settings::Settings;
use crate::theme::ThemeMode;
use crate::ui::log_view::LogViewState;

actions!(
    xcom,
    [
        Send,
        TogglePort,
        RefreshPorts,
        SaveLog,
        SaveRawLog,
        QuitApp,
        CopyLog,
        SelectAllLog
    ]
);

/// Key context for the window-level shortcuts.
pub(crate) const APP_CONTEXT: &str = "Xcom";
/// Key context for the receive log, which owns copy/select-all while focused.
pub(crate) const LOG_CONTEXT: &str = "XcomLog";

/// Bind the application's keyboard shortcuts.
pub fn init(cx: &mut App) {
    // GPUI's key parser does not accept the `secondary-` alias; spell the
    // platform's command key out.  Cmd on macOS, Ctrl elsewhere.
    cx.bind_keys([
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-enter"
            } else {
                "ctrl-enter"
            },
            Send,
            Some(APP_CONTEXT),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-o"
            } else {
                "ctrl-o"
            },
            TogglePort,
            Some(APP_CONTEXT),
        ),
        KeyBinding::new("f5", RefreshPorts, Some(APP_CONTEXT)),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-s"
            } else {
                "ctrl-s"
            },
            SaveLog,
            Some(APP_CONTEXT),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-s"
            } else {
                "ctrl-shift-s"
            },
            SaveRawLog,
            Some(APP_CONTEXT),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            },
            QuitApp,
            Some(APP_CONTEXT),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-c"
            } else {
                "ctrl-c"
            },
            CopyLog,
            Some(LOG_CONTEXT),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            SelectAllLog,
            Some(LOG_CONTEXT),
        ),
    ]);
}

/// Severity of a status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Info,
    Success,
    Warning,
    Error,
}

/// A transient status message shown in the status bar.
#[derive(Debug, Clone)]
pub(crate) struct Message {
    pub level: Level,
    pub text: String,
    pub at: Instant,
}

/// Which tab the right-hand panel is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SideTab {
    #[default]
    Multi,
    Protocol,
}

/// A chunked background file transmission.
pub(crate) struct FileSend {
    pub path: String,
    pub data: Vec<u8>,
    pub offset: usize,
    /// Bytes per tick, smoothed to avoid starving the UI.
    pub chunk: usize,
    pub last: Instant,
}

impl FileSend {
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    pub fn progress(&self) -> f32 {
        if self.data.is_empty() {
            1.0
        } else {
            self.offset as f32 / self.data.len() as f32
        }
    }
}

/// A line in the protocol transfer log.
pub(crate) struct ProtocolLogLine {
    pub level: Level,
    pub text: String,
    pub stamp: String,
}

/// What [`XcomApp::send_multi_entry`] did with one entry.
pub(crate) enum MultiSendOutcome {
    Sent,
    Skipped,
    Refused,
}

/// A multi-send sequence in progress.
pub(crate) struct MultiRun {
    queue: Vec<usize>,
    position: usize,
    last_sent: Option<SentEntry>,
    next_at: Instant,
    wait: Option<DelayWait>,
    looping: bool,
}

/// The per-entry delay the running multi-send is waiting out.
pub(crate) struct DelayWait {
    index: usize,
    started_at: Instant,
}

/// The entry a running multi-send sent most recently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SentEntry {
    pub index: usize,
    pub ordinal: usize,
    pub round_len: usize,
}

/// A delay countdown the multi-send panel can draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MultiCountdown {
    pub index: usize,
    pub remaining: f32,
    pub remaining_ms: u64,
}

/// Text inputs owned by one multi-send row.
pub(crate) struct MultiRowInputs {
    pub text: Entity<InputState>,
    pub note: Entity<InputState>,
    pub delay: Entity<InputState>,
}

/// Text inputs owned by one keyword-highlighting rule.
pub(crate) struct HighlightRowInputs {
    pub text: Entity<InputState>,
    pub color: Entity<InputState>,
}

/// Numeric and short text inputs shared by the panels.
pub(crate) struct UiInputs {
    pub send_period: Entity<InputState>,
    pub loop_count: Entity<InputState>,
    pub protocol_address: Entity<InputState>,
    pub protocol_function: Entity<InputState>,
    pub protocol_period: Entity<InputState>,
    pub protocol_retries: Entity<InputState>,
    pub protocol_data_len: Entity<InputState>,
    pub max_rows: Entity<InputState>,
    pub max_raw_mb: Entity<InputState>,
    pub custom_baud: Entity<InputState>,
    pub log_fg: Entity<InputState>,
    pub log_bg: Entity<InputState>,
    pub log_dir: Entity<InputState>,
    pub log_pattern: Entity<InputState>,
}

/// A menu or toolbar command.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    TogglePort,
    RefreshPorts,
    SaveReceived,
    SaveRaw,
    Quit,
    CopySelection,
    SelectAll,
    ClearReceive,
    ClearSend,
    ResetSettings,
    ToggleConfigPanel,
    ToggleToolPanel,
    ToggleSendArea,
    ToggleHex,
    ToggleMarkers,
    ToggleTimestamp,
    ToggleWrap,
    ToggleAutoScroll,
    ToggleEcho,
    TogglePause,
    SendFile,
    AbortFileSend,
    UseHistory(usize),
    MultiAdd,
    MultiImport,
    MultiExport,
    MultiSendChecked,
    MultiRemove(usize),
    MultiSendOne(usize),
    MultiMove { from: usize, to: usize },
    MultiNewlineAll,
    ProtocolSingle,
    ProtocolAuto,
    ProtocolFile,
    ProtocolStop,
    StartLogFile,
    StopLogFile,
    ShowFontSettings,
    ShowLogColors,
    ShowHighlight,
    ShowLogFile,
    ShowCustomBaud,
    ShowShortcuts,
    ShowAbout,
    HighlightAdd,
    HighlightRemove(usize),
}

/// Format an RGB triple as the `#RRGGBB` text the colour dialog edits.
pub(crate) fn hex_color(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Parse `#RRGGBB`, `RRGGBB` or `#RGB`.
pub(crate) fn parse_color(text: &str) -> Option<[u8; 3]> {
    let text = text.trim().trim_start_matches('#');
    let parse = |s: &str| u8::from_str_radix(s, 16).ok();
    match text.len() {
        3 => {
            let r = parse(&text[0..1])?;
            let g = parse(&text[1..2])?;
            let b = parse(&text[2..3])?;
            Some([r * 17, g * 17, b * 17])
        }
        6 => Some([
            parse(&text[0..2])?,
            parse(&text[2..4])?,
            parse(&text[4..6])?,
        ]),
        _ => None,
    }
}

/// A menu choice with its action payload.
#[derive(Clone)]
pub(crate) enum Choice {
    Port(String),
    Baud(u32),
    DataBits(crate::serial::DataBitsSel),
    Parity(crate::serial::ParitySel),
    StopBits(crate::serial::StopBitsSel),
    Flow(crate::serial::FlowSel),
    Encoding(EncodingChoice),
    TxEncoding(EncodingChoice),
    Theme(ThemeMode),
    Language(crate::i18n::Lang),
    Checksum(crate::protocol::ChecksumKind),
    UiFont(Option<String>),
    ReceiveFont(Option<String>),
    SendFont(Option<String>),
    UiSize(f32),
    ReceiveSize(f32),
    SendSize(f32),
    LineSpacing(f32),
}

/// The application.
pub struct XcomApp {
    pub(crate) settings: Settings,
    pub(crate) link: SerialLink,
    pub(crate) ports: PortWatcher,
    pub(crate) log: LogStore,
    pub(crate) auto: AutoDetector,
    pub(crate) log_view: LogViewState,

    pub(crate) open_port_enumerated: bool,
    pub(crate) rx_bytes: u64,
    pub(crate) tx_bytes: u64,
    pub(crate) pending_rx: Vec<u8>,
    pub(crate) show_paused: bool,

    pub(crate) messages: Vec<Message>,
    pub(crate) last_timed_send: Option<Instant>,
    pub(crate) multi_run: Option<MultiRun>,
    pub(crate) multi_cycles: u32,
    pub(crate) file_send: Option<FileSend>,

    pub(crate) protocol: ProtocolEngine,
    pub(crate) protocol_log: Vec<ProtocolLogLine>,
    pub(crate) protocol_progress: Option<(usize, usize)>,

    pub(crate) side_tab: SideTab,
    pub(crate) log_file: LogFile,
    pub(crate) signals: Signals,

    /// The single-send editor.
    pub(crate) send_input: Entity<TextareaState>,
    /// The protocol payload editor.
    pub(crate) protocol_input: Entity<TextareaState>,
    /// One set of editors per multi-send row.
    pub(crate) multi_inputs: Vec<MultiRowInputs>,
    /// Numeric and short text inputs shared by the panels.
    pub(crate) inputs: UiInputs,
    /// One set of editors per keyword-highlighting rule.
    pub(crate) highlight_inputs: Vec<HighlightRowInputs>,
    /// Subscriptions for the multi-send row editors; dropped and rebuilt when
    /// the list's structure changes.
    multi_subs: Vec<Subscription>,
    /// Subscriptions for the keyword rule editors.
    highlight_subs: Vec<Subscription>,
    /// Subscriptions for the shared panel inputs.
    ui_subs: Vec<Subscription>,

    /// Keyboard focus for the receive log.
    pub(crate) log_focus: FocusHandle,

    settings_dirty: bool,
    last_save: Instant,
    _tick_task: Task<()>,
    subscriptions: Vec<Subscription>,
    appearance_subscription: Option<Subscription>,
    applied_theme: Option<ThemeMode>,
    applied_lang: Option<crate::i18n::Lang>,
    applied_fonts: Option<(Option<String>, f32)>,
    font_warning_shown: bool,
    /// When a wrap-width change was first observed, so a window drag does not
    /// re-render every retained byte on every intermediate width.
    pub(crate) pending_wrap_cols: Option<(usize, Instant)>,
    /// Last window size written into the settings, to avoid marking dirty every
    /// frame while the window is being resized.
    last_window_size: Option<[f32; 2]>,
    /// Last whole second shown by the status clock.
    last_clock_second: i64,
    /// Set by the reset confirmation, applied at the top of the next frame
    /// where a window is available to resynchronise the editors.
    pub(crate) needs_reset: bool,
    /// Whether a left press on the title bar is still a potential drag.
    pub(crate) title_dragging: bool,
    /// Resizable state for the three columns and the log/send split.
    pub(crate) main_split: Entity<gpui_kit::component::resizable::ResizableState>,
    pub(crate) center_split: Entity<gpui_kit::component::resizable::ResizableState>,
    /// Desktop window-button placement, read before the first frame and kept
    /// current by the window's button-layout observer.
    pub(crate) button_layout: gpui_kit::WindowButtonLayout,
    button_layout_subscription: Option<Subscription>,
}

impl XcomApp {
    /// Build the application after loading persisted settings.
    pub fn new(window: &mut Window, cx: &mut Context<Self>, settings: Settings) -> Self {
        let lang = settings.lang.unwrap_or_else(crate::i18n::system_lang);
        crate::i18n::set_lang(lang);

        let send_text = settings.send.buffer.clone();
        let protocol_text = settings.protocol_data.clone();
        let send_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(send_text)
                .placeholder(tr("输入要发送的内容"))
        });
        let protocol_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value(protocol_text)
                .placeholder(tr("十六进制，如 00 00 00 0A"))
        });

        let mut input = |cx: &mut Context<Self>, value: String| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let inputs = UiInputs {
            send_period: input(cx, settings.send.period_ms.to_string()),
            loop_count: input(
                cx,
                settings
                    .multi
                    .loop_count
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            ),
            protocol_address: input(cx, settings.protocol.address.to_string()),
            protocol_function: input(cx, settings.protocol.function.to_string()),
            protocol_period: input(cx, settings.protocol.frame_period_ms.to_string()),
            protocol_retries: input(cx, settings.protocol.max_retries.to_string()),
            protocol_data_len: input(cx, settings.protocol.max_data_len.to_string()),
            max_rows: input(cx, settings.max_rows.to_string()),
            max_raw_mb: input(cx, (settings.max_raw / (1024 * 1024)).to_string()),
            custom_baud: input(cx, "115200".to_owned()),
            log_fg: input(cx, hex_color(settings.log_colors.foreground)),
            log_bg: input(cx, hex_color(settings.log_colors.background)),
            log_dir: input(cx, settings.log_file.directory.clone()),
            log_pattern: input(cx, settings.log_file.pattern.clone()),
        };

        let main_split = cx.new(|_| gpui_kit::component::resizable::ResizableState::default());
        let center_split = cx.new(|_| gpui_kit::component::resizable::ResizableState::default());

        let mut app = Self {
            log: LogStore::new(encoding_rs::UTF_8),
            auto: AutoDetector::new(),
            log_view: LogViewState::new(),
            link: SerialLink::new(),
            ports: PortWatcher::spawn(Duration::from_millis(1000)),
            settings,
            open_port_enumerated: false,
            rx_bytes: 0,
            tx_bytes: 0,
            pending_rx: Vec::new(),
            show_paused: false,
            messages: Vec::new(),
            last_timed_send: None,
            multi_run: None,
            multi_cycles: 0,
            file_send: None,
            protocol: ProtocolEngine::new(crate::protocol::ProtocolConfig::default(), 115200),
            protocol_log: Vec::new(),
            protocol_progress: None,
            side_tab: SideTab::Multi,
            log_file: LogFile::default(),
            signals: Signals::default(),
            send_input,
            protocol_input,
            multi_inputs: Vec::new(),
            inputs,
            highlight_inputs: Vec::new(),
            multi_subs: Vec::new(),
            highlight_subs: Vec::new(),
            ui_subs: Vec::new(),
            log_focus: cx.focus_handle(),
            settings_dirty: false,
            last_save: Instant::now(),
            _tick_task: Task::ready(()),
            subscriptions: Vec::new(),
            appearance_subscription: None,
            applied_theme: None,
            applied_lang: None,
            applied_fonts: None,
            font_warning_shown: false,
            pending_wrap_cols: None,
            last_window_size: None,
            last_clock_second: 0,
            needs_reset: false,
            title_dragging: false,
            main_split,
            center_split,
            button_layout: crate::ui::panels::read_button_layout(),
            button_layout_subscription: None,
        };

        app.protocol = ProtocolEngine::new(app.settings.protocol.clone(), app.settings.serial.baud);
        app.log
            .set_limits(app.settings.max_rows, app.settings.max_raw);
        app.log.set_options(app.settings.display);
        app.log.set_encoding(app.effective_encoding());
        app.select_default_port();
        app.rebuild_multi_inputs(window, cx);

        let send_sub = cx.subscribe_in(
            &app.send_input,
            window,
            |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings.send.buffer = state.read(cx).value().to_string();
                    this.mark_settings_dirty();
                }
            },
        );
        let protocol_sub = cx.subscribe_in(
            &app.protocol_input,
            window,
            |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings.protocol_data = state.read(cx).value().to_string();
                    this.mark_settings_dirty();
                }
            },
        );
        app.subscriptions.push(send_sub);
        app.subscriptions.push(protocol_sub);

        let send_period = app.inputs.send_period.clone();
        app.ui_subs.push(cx.subscribe_in(
            &send_period,
            window,
            |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<u64>()
                {
                    this.settings.send.period_ms = value.clamp(1, 3_600_000);
                    this.mark_settings_dirty();
                }
            },
        ));

        let loop_count = app.inputs.loop_count.clone();
        app.ui_subs.push(cx.subscribe_in(
            &loop_count,
            window,
            |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().trim().to_owned();
                    let next = if value.is_empty() {
                        None
                    } else {
                        value.parse::<u32>().ok().map(|n| n.clamp(1, 1_000_000))
                    };
                    if this.settings.multi.loop_count != next {
                        this.settings.multi.loop_count = next;
                        this.mark_settings_dirty();
                    }
                }
            },
        ));

        let address = app.inputs.protocol_address.clone();
        app.ui_subs.push(
            cx.subscribe_in(&address, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<u8>()
                {
                    this.settings.protocol.address = value;
                    this.protocol.set_config(this.settings.protocol.clone());
                    this.mark_settings_dirty();
                }
            }),
        );
        let function = app.inputs.protocol_function.clone();
        app.ui_subs.push(
            cx.subscribe_in(&function, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<u8>()
                {
                    this.settings.protocol.function = value;
                    this.protocol.set_config(this.settings.protocol.clone());
                    this.mark_settings_dirty();
                }
            }),
        );
        let frame_period = app.inputs.protocol_period.clone();
        app.ui_subs.push(cx.subscribe_in(
            &frame_period,
            window,
            |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<u64>()
                {
                    this.settings.protocol.frame_period_ms = value.clamp(1, 600_000);
                    this.protocol.set_config(this.settings.protocol.clone());
                    this.mark_settings_dirty();
                }
            },
        ));
        let retries = app.inputs.protocol_retries.clone();
        app.ui_subs.push(
            cx.subscribe_in(&retries, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<u32>()
                {
                    this.settings.protocol.max_retries = value.min(255);
                    this.protocol.set_config(this.settings.protocol.clone());
                    this.mark_settings_dirty();
                }
            }),
        );
        let data_len = app.inputs.protocol_data_len.clone();
        app.ui_subs.push(
            cx.subscribe_in(&data_len, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<usize>()
                {
                    this.settings.protocol.max_data_len = value.clamp(1, 255);
                    this.protocol.set_config(this.settings.protocol.clone());
                    this.mark_settings_dirty();
                }
            }),
        );
        let max_rows = app.inputs.max_rows.clone();
        app.ui_subs.push(
            cx.subscribe_in(&max_rows, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<usize>()
                {
                    this.settings.max_rows = value.clamp(1_000, 5_000_000);
                    this.log
                        .set_limits(this.settings.max_rows, this.settings.max_raw);
                    this.mark_settings_dirty();
                }
            }),
        );
        let max_raw = app.inputs.max_raw_mb.clone();
        app.ui_subs.push(
            cx.subscribe_in(&max_raw, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Ok(value) = state.read(cx).value().trim().parse::<usize>()
                {
                    this.settings.max_raw = value.clamp(1, 512) * 1024 * 1024;
                    this.log
                        .set_limits(this.settings.max_rows, this.settings.max_raw);
                    this.mark_settings_dirty();
                }
            }),
        );
        let log_fg = app.inputs.log_fg.clone();
        app.ui_subs.push(
            cx.subscribe_in(&log_fg, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(rgb) = parse_color(&state.read(cx).value())
                {
                    this.settings.log_colors.foreground = rgb;
                    this.mark_settings_dirty();
                }
            }),
        );
        let log_bg = app.inputs.log_bg.clone();
        app.ui_subs.push(
            cx.subscribe_in(&log_bg, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change)
                    && let Some(rgb) = parse_color(&state.read(cx).value())
                {
                    this.settings.log_colors.background = rgb;
                    this.mark_settings_dirty();
                }
            }),
        );
        let log_dir = app.inputs.log_dir.clone();
        app.ui_subs.push(
            cx.subscribe_in(&log_dir, window, |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings.log_file.directory = state.read(cx).value().to_string();
                    this.mark_settings_dirty();
                }
            }),
        );
        let log_pattern = app.inputs.log_pattern.clone();
        app.ui_subs.push(cx.subscribe_in(
            &log_pattern,
            window,
            |this, state, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.settings.log_file.pattern = state.read(cx).value().to_string();
                    this.mark_settings_dirty();
                }
            },
        ));

        let weak = cx.entity().downgrade();
        app.button_layout_subscription =
            Some(window.observe_button_layout_changed(move |_window, cx| {
                if let Some(layout) = cx.button_layout()
                    && layout
                        .left
                        .iter()
                        .chain(layout.right.iter())
                        .any(|button| matches!(button, Some(gpui_kit::WindowButton::Close)))
                {
                    let _ = weak.update(cx, |this, cx| {
                        if this.button_layout != layout {
                            this.button_layout = layout;
                            cx.notify();
                        }
                    });
                }
            }));

        let weak = cx.entity().downgrade();
        app.appearance_subscription = Some(window.observe_window_appearance(move |_window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.settings.theme == ThemeMode::System {
                    this.applied_theme = None;
                    cx.notify();
                }
            });
        }));

        // The test harness drives frames explicitly; a detached 60 Hz loop
        // would keep its timer alive past the test's last assertion.
        #[cfg(not(test))]
        {
            let weak = cx.entity().downgrade();
            let tick_task = cx.spawn(async move |_this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let updated = weak.update(cx, |this, cx| {
                        if this.tick() {
                            cx.notify();
                        }
                    });
                    if updated.is_err() {
                        break;
                    }
                }
            });
            app._tick_task = tick_task;
        }
        app
    }

    // -- lifecycle ----------------------------------------------------------

    fn select_default_port(&mut self) {
        self.ports.poll();
        let available = self.ports.ports();
        if available.is_empty() {
            return;
        }
        let current_ok = available
            .iter()
            .any(|p| p.name == self.settings.serial.port);
        if !current_ok {
            let pick = available
                .iter()
                .find(|p| p.is_usb())
                .or_else(|| available.first())
                .map(|p| p.name.clone());
            if let Some(name) = pick {
                self.settings.serial.port = name;
            }
        }
    }

    fn note_font_status(&mut self) {
        if self.font_warning_shown {
            return;
        }
        self.font_warning_shown = true;
        match crate::fonts::find_cjk_family() {
            Some(family) => log::info!("中文字体: {family}"),
            None => self.warn(tr(
                "未找到中文字体，界面中文可能显示为方块。请安装 Noto Sans CJK / 文泉驿等字体",
            )),
        }
    }

    // -- messages -----------------------------------------------------------

    pub(crate) fn message(&mut self, level: Level, text: impl Into<String>) {
        let text = text.into();
        log::log!(
            match level {
                Level::Error => log::Level::Error,
                Level::Warning => log::Level::Warn,
                _ => log::Level::Info,
            },
            "{text}"
        );
        if let Some(last) = self.messages.last_mut()
            && last.text == text
        {
            last.at = Instant::now();
            return;
        }
        self.messages.push(Message {
            level,
            text,
            at: Instant::now(),
        });
        if self.messages.len() > 50 {
            self.messages.remove(0);
        }
    }

    pub(crate) fn info(&mut self, text: impl Into<String>) {
        self.message(Level::Info, text);
    }

    pub(crate) fn ok(&mut self, text: impl Into<String>) {
        self.message(Level::Success, text);
    }

    pub(crate) fn warn(&mut self, text: impl Into<String>) {
        self.message(Level::Warning, text);
    }

    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.message(Level::Error, text);
    }

    pub(crate) fn latest_message(&self) -> Option<&Message> {
        self.messages.last()
    }

    pub(crate) fn pending_rx_len(&self) -> usize {
        self.pending_rx.len()
    }

    /// Apply a new wrap width once it has stopped changing.
    pub(crate) fn update_wrap_width(&mut self, cols: usize) -> bool {
        if cols == self.log.width_columns() {
            self.pending_wrap_cols = None;
            return false;
        }
        const SETTLE: Duration = Duration::from_millis(180);
        match self.pending_wrap_cols {
            Some((pending, since)) if pending == cols && since.elapsed() >= SETTLE => {
                self.log.set_width_columns(cols);
                self.log_view.clear_selection();
                self.pending_wrap_cols = None;
                true
            }
            Some((pending, _)) if pending == cols => false,
            _ => {
                self.pending_wrap_cols = Some((cols, Instant::now()));
                false
            }
        }
    }

    // -- encodings ----------------------------------------------------------

    pub(crate) fn effective_encoding(&self) -> &'static encoding_rs::Encoding {
        match self.settings.encoding {
            Some(id) => id.encoding(),
            None => self.auto.active(),
        }
    }

    pub(crate) fn tx_encoding(&self) -> &'static encoding_rs::Encoding {
        match self.settings.tx_encoding {
            Some(id) => id.encoding(),
            None => self.effective_encoding(),
        }
    }

    pub(crate) fn encoding_label(&self) -> String {
        match self.settings.encoding {
            Some(id) => id.short_label().to_owned(),
            None => {
                let detected = EncodingId::from_encoding(self.auto.active())
                    .map(EncodingId::short_label)
                    .unwrap_or_else(|| self.auto.active().name());
                crate::i18n::tr1("自动·{}", detected)
            }
        }
    }

    pub(crate) fn set_encoding(&mut self, choice: EncodingChoice) {
        if self.settings.encoding == choice {
            return;
        }
        self.settings.encoding = choice;
        if choice.is_none() {
            self.auto.reset();
        }
        let encoding = self.effective_encoding();
        self.log.set_encoding(encoding);
        self.mark_settings_dirty();
    }

    // -- port control -------------------------------------------------------

    pub(crate) fn open_port(&mut self) {
        if self.settings.serial.port.trim().is_empty() {
            self.error(tr("请先选择串口"));
            return;
        }
        match self.link.open(&self.settings.serial) {
            Ok(()) => {
                for note in self.link.take_notes() {
                    self.warn(note);
                }
                self.auto.reset();
                let encoding = self.effective_encoding();
                self.log.set_encoding(encoding);
                self.protocol.set_baud(self.settings.serial.baud);
                let summary = self.settings.serial.summary();
                let port = self.settings.serial.port.clone();
                self.open_port_enumerated = self.ports.find(&port).is_some();
                self.ok(crate::i18n::tr2("已打开 {} [{}]", port, summary));
                self.mark_settings_dirty();
            }
            Err(err) => self.error(err),
        }
    }

    pub(crate) fn close_port(&mut self) {
        if let Some(kind) = self.protocol.task_kind() {
            self.protocol.stop(tr("串口已关闭"));
            self.info(crate::i18n::tr1("已停止{}", tr(kind.label())));
        }
        let was_open = self.link.is_open();
        self.link.close();
        self.open_port_enumerated = false;
        self.signals = Signals::default();
        self.settings.send.timed = false;
        if was_open {
            self.info(tr("串口已关闭"));
        }
    }

    /// Apply the DTR/RTS switch state to an open port.
    pub(crate) fn apply_line_state(&mut self, _cx: &mut Context<Self>) {
        if !self.link.is_open() {
            self.mark_settings_dirty();
            return;
        }
        if let Err(err) = self.link.set_dtr(self.settings.serial.dtr) {
            self.error(crate::i18n::tr1("设置 DTR 失败: {}", err));
        }
        if let Err(err) = self.link.set_rts(self.settings.serial.rts) {
            self.error(crate::i18n::tr1("设置 RTS 失败: {}", err));
        }
        self.mark_settings_dirty();
    }

    pub(crate) fn toggle_port(&mut self) {
        if self.link.is_open() {
            self.close_port();
        } else {
            self.open_port();
        }
    }

    // -- sending ------------------------------------------------------------

    pub(crate) fn build_payload(&self) -> Result<Vec<u8>, String> {
        let text = &self.settings.send.buffer;
        let mut bytes = if self.settings.send.hex {
            crate::hex::parse(text).map_err(|e| e.to_string())?
        } else {
            crate::encoding::encode(text, self.tx_encoding())
        };
        if self.settings.send.append_newline {
            bytes.extend_from_slice(&self.settings.send.newline_bytes());
        }
        Ok(bytes)
    }

    pub(crate) fn send_once(&mut self) {
        if !self.link.is_open() {
            self.error(tr("串口未打开"));
            return;
        }
        let payload = match self.build_payload() {
            Ok(bytes) => bytes,
            Err(err) => {
                self.error(err);
                return;
            }
        };
        if payload.is_empty() {
            self.warn(tr("发送内容为空"));
            return;
        }
        if let Err(err) = self.link.send(payload.clone()) {
            self.error(err);
            return;
        }
        self.echo_sent(&payload);
        let buffer = self.settings.send.buffer.clone();
        self.settings.send.remember(&buffer);
        self.mark_settings_dirty();
    }

    pub(crate) fn echo_sent(&mut self, bytes: &[u8]) {
        if !self.settings.echo_sent || bytes.is_empty() {
            return;
        }
        let now = chrono::Local::now().timestamp_millis();
        self.log.append_sent(bytes, now);
    }

    pub(crate) fn send_bytes(&mut self, bytes: Vec<u8>, what: &str) {
        if !self.link.is_open() {
            self.error(crate::i18n::tr1("串口未打开，无法{}", what));
            return;
        }
        if let Err(err) = self.link.send(bytes) {
            self.error(err);
        }
    }

    pub(crate) fn start_file_send(&mut self, path: PathBuf) {
        if !self.link.is_open() {
            self.error(tr("串口未打开"));
            return;
        }
        match std::fs::read(&path) {
            Ok(data) if data.is_empty() => self.warn(tr("文件为空，未发送")),
            Ok(data) => {
                let name = path.display().to_string();
                self.info(crate::i18n::tr2(
                    "开始发送文件 {}（{} 字节）",
                    &name,
                    data.len(),
                ));
                self.file_send = Some(FileSend {
                    path: name,
                    data,
                    offset: 0,
                    chunk: 1024,
                    last: Instant::now(),
                });
            }
            Err(err) => self.error(crate::i18n::tr1("读取文件失败: {}", err)),
        }
    }

    pub(crate) fn abort_file_send(&mut self) {
        if let Some(send) = self.file_send.take() {
            let remaining = send.remaining();
            self.warn(crate::i18n::tr2(
                "已终止文件发送（{}，剩余 {} 字节）",
                &send.path,
                remaining,
            ));
        }
    }

    // -- log ----------------------------------------------------------------

    pub(crate) fn clear_receive(&mut self) {
        self.log.clear();
        self.log_view.clear_selection();
        self.rx_bytes = 0;
        self.tx_bytes = 0;
        self.pending_rx.clear();
        self.info(tr("已清除接收区与收发计数"));
    }

    // -- log file -----------------------------------------------------------

    pub(crate) fn start_log_file(&mut self) {
        let settings = self.settings.log_file.clone();
        let directory = crate::logfile::resolve_directory(&settings.directory);
        let name = crate::logfile::file_name_for(
            &settings.pattern,
            chrono::Local::now(),
            &self.settings.serial.port,
        );
        match self.log_file.start(&directory, &name) {
            Ok(path) => self.ok(crate::i18n::tr1("日志文件: {}", path.display())),
            Err(err) => self.error(crate::i18n::tr1("无法创建日志文件: {}", err)),
        }
    }

    pub(crate) fn stop_log_file(&mut self) {
        let Some(path) = self.log_file.path().map(PathBuf::from) else {
            return;
        };
        let bytes = self.log_file.bytes();
        self.log_file.stop();
        self.info(crate::i18n::tr2(
            "已停止日志记录（{} 字节）: {}",
            bytes,
            path.display(),
        ));
    }

    pub(crate) fn set_log_file_enabled(&mut self, enabled: bool) {
        if enabled {
            if !self.log_file.is_active() {
                self.start_log_file();
            }
        } else {
            self.stop_log_file();
        }
    }

    pub(crate) fn write_log_file(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || !self.log_file.is_active() {
            return;
        }
        if let Err(err) = self.log_file.write_all(bytes) {
            let path = self.log_file.path().map(|path| path.display().to_string());
            self.log_file.stop();
            match path {
                Some(path) => self.error(crate::i18n::tr2("写入日志文件失败: {}（{}）", err, path)),
                None => self.error(crate::i18n::tr1("写入日志文件失败: {}", err)),
            }
        }
    }

    // -- settings -----------------------------------------------------------

    pub(crate) fn set_display_options(&mut self, display: crate::logstore::DisplayOptions) {
        if display == self.settings.display {
            return;
        }
        self.settings.display = display;
        self.log.set_options(display);
        if !display.wrap {
            self.log.set_width_columns(0);
        }
        self.log_view.clear_selection();
        self.mark_settings_dirty();
    }

    pub(crate) fn set_auto_scroll(&mut self, on: bool) {
        if self.settings.auto_scroll == on {
            return;
        }
        self.settings.auto_scroll = on;
        if on {
            self.log_view.clear_selection();
        }
        self.mark_settings_dirty();
    }

    pub(crate) fn mark_settings_dirty(&mut self) {
        self.settings_dirty = true;
    }

    fn maybe_save_settings(&mut self) -> bool {
        if !self.settings_dirty || self.last_save.elapsed() < Duration::from_secs(2) {
            return false;
        }
        match self.settings.save() {
            Ok(()) => self.settings_dirty = false,
            Err(err) => {
                self.settings_dirty = false;
                self.error(crate::i18n::tr1("保存设置失败: {}", err));
            }
        }
        self.last_save = Instant::now();
        true
    }

    /// Save on exit, including the current window size.
    pub fn save_on_exit(&mut self) {
        self.log_file.stop();
        if let Err(err) = self.settings.save() {
            log::error!("退出时保存设置失败: {err}");
        }
    }

    // -- event pumping ------------------------------------------------------

    /// One turn of the background pump. Returns whether the view should redraw.
    pub(crate) fn tick(&mut self) -> bool {
        let mut dirty = self.pump_serial();
        dirty |= self.pump_timers();
        dirty |= self.maybe_save_settings();
        // The status clock only needs a frame when its second changes.
        let second = chrono::Local::now().timestamp();
        if self.last_clock_second != second {
            self.last_clock_second = second;
            dirty = true;
        }
        dirty
    }

    fn pump_serial(&mut self) -> bool {
        let mut dirty = false;
        if self.ports.poll() {
            let port = self.settings.serial.port.clone();
            let present = !port.is_empty() && self.ports.find(&port).is_some();
            if present {
                self.open_port_enumerated = true;
            }
            if self.link.is_open() && self.open_port_enumerated && !present {
                self.warn(crate::i18n::tr1("{} 已断开", port));
                self.close_port();
            }
            self.select_default_port();
            dirty = true;
        }

        let now = Instant::now();
        let epoch_ms = chrono::Local::now().timestamp_millis();
        let mut rx_buf: Vec<u8> = Vec::new();
        for event in self.link.poll() {
            match event {
                SerialEvent::Data(bytes) => {
                    dirty = true;
                    self.rx_bytes += bytes.len() as u64;
                    rx_buf.extend_from_slice(&bytes);
                }
                SerialEvent::Written(count) => {
                    self.tx_bytes += count as u64;
                    dirty = true;
                }
                SerialEvent::WriteFailed(err) => {
                    self.error(err);
                    dirty = true;
                }
                SerialEvent::Lost(err) => {
                    self.error(crate::i18n::tr1("{}（串口可能已被拔出）", err));
                    self.close_port();
                    dirty = true;
                }
            }
        }

        if !rx_buf.is_empty() {
            self.write_log_file(&rx_buf);
            if self.show_paused {
                self.pending_rx.extend_from_slice(&rx_buf);
            } else {
                self.ingest(&rx_buf, epoch_ms);
            }
            if self.protocol.is_running() || self.protocol_panel_visible() {
                self.protocol.on_bytes(&rx_buf, now);
            }
        }

        if self.link.is_open()
            && let Some(signals) = self.link.signals()
            && signals != self.signals
        {
            self.signals = signals;
            dirty = true;
        }
        dirty
    }

    fn ingest(&mut self, bytes: &[u8], epoch_ms: i64) {
        let encoding = if self.settings.encoding.is_none() {
            self.auto.feed(bytes)
        } else {
            self.effective_encoding()
        };
        self.log.set_encoding(encoding);
        self.log.append(bytes, epoch_ms);
    }

    pub(crate) fn protocol_panel_visible(&self) -> bool {
        self.settings.show_tool_panel && self.side_tab == SideTab::Protocol
    }

    pub(crate) fn resume_display(&mut self) {
        self.show_paused = false;
        if !self.pending_rx.is_empty() {
            let pending = std::mem::take(&mut self.pending_rx);
            let epoch_ms = chrono::Local::now().timestamp_millis();
            self.ingest(&pending, epoch_ms);
            self.info(crate::i18n::tr1("已恢复显示，补入 {} 字节", pending.len()));
        }
    }

    fn pump_timers(&mut self) -> bool {
        let now = Instant::now();
        let mut dirty = false;

        if self.settings.send.timed && self.link.is_open() {
            let period = Duration::from_millis(self.settings.send.period_ms.max(1));
            let due = self
                .last_timed_send
                .is_none_or(|last| now.duration_since(last) >= period);
            if due {
                self.last_timed_send = Some(now);
                self.send_once();
                dirty = true;
            }
        } else {
            self.last_timed_send = None;
        }

        dirty |= self.pump_multi(now);
        dirty |= self.pump_file_send(now);
        dirty |= self.pump_protocol(now);
        dirty
    }

    fn pump_file_send(&mut self, now: Instant) -> bool {
        let Some(send) = self.file_send.as_ref() else {
            return false;
        };
        let connected = self.link.is_open();
        let due = now.duration_since(send.last) >= Duration::from_millis(2);
        let remaining = send.remaining();
        let offset = send.offset;
        let total = send.data.len();
        let end = (offset + send.chunk).min(total);
        let chunk = send.data[offset..end].to_vec();

        if !connected {
            self.file_send = None;
            self.warn(tr("串口已关闭，文件发送中止"));
            return true;
        }
        if remaining == 0 {
            self.file_send = None;
            self.ok(crate::i18n::tr1("文件发送完成（{} 字节）", total));
            return true;
        }
        if !due {
            return true;
        }

        if let Some(state) = self.file_send.as_mut() {
            state.last = now;
            state.offset = end;
        }
        if let Err(err) = self.link.send(chunk) {
            self.error(err);
            self.file_send = None;
        }
        true
    }

    fn pump_protocol(&mut self, now: Instant) -> bool {
        if !self.protocol.is_running() {
            if self.protocol_panel_visible() {
                let _ = self.protocol.tick(now);
                self.drain_protocol_events();
            }
            return false;
        }
        if let Some(bytes) = self.protocol.tick(now) {
            let sequence = self.protocol.outstanding_sequence().unwrap_or(0);
            let attempt = self.protocol.outstanding_attempt().unwrap_or(1);
            self.log_protocol(
                Level::Info,
                crate::i18n::tr3(
                    "→ 帧 #{}（第 {} 次）: {}",
                    sequence,
                    attempt,
                    crate::hex::to_hex_string(&bytes, true),
                ),
            );
            self.send_bytes(bytes, tr("发送协议帧"));
        }
        self.drain_protocol_events();
        true
    }

    fn drain_protocol_events(&mut self) {
        for event in self.protocol.drain_events() {
            match event {
                ProtocolEvent::Sent { .. } | ProtocolEvent::Retransmit { .. } => {}
                ProtocolEvent::Acknowledged {
                    sequence, attempt, ..
                } => {
                    self.protocol_progress = None;
                    self.log_protocol(
                        Level::Success,
                        crate::i18n::tr2("✓ 收到应答 #{}（第 {} 次尝试）", sequence, attempt),
                    );
                }
                ProtocolEvent::NoResponse { sequence } => {
                    self.protocol_progress = None;
                    self.log_protocol(
                        Level::Error,
                        crate::i18n::tr1("✗ 帧 #{} 设备无响应，已停止发送", sequence),
                    );
                }
                ProtocolEvent::Response { frame } => {
                    let text = self.format_protocol_frame(&frame);
                    self.log_protocol(Level::Info, format!("← {text}"));
                }
                ProtocolEvent::Malformed { bytes, error } => {
                    self.log_protocol(
                        Level::Warning,
                        crate::i18n::tr2(
                            "← 无效响应 [{}]：{}",
                            crate::hex::to_hex_string(&bytes, true),
                            error,
                        ),
                    );
                }
                ProtocolEvent::FileProgress { sent, total } => {
                    self.protocol_progress = Some((sent, total));
                }
                ProtocolEvent::FileFinished { total } => {
                    self.protocol_progress = None;
                    self.log_protocol(
                        Level::Success,
                        crate::i18n::tr1("文件传输完成，共 {} 字节", total),
                    );
                }
                ProtocolEvent::Stopped(reason) => {
                    self.protocol_progress = None;
                    self.log_protocol(Level::Warning, reason);
                }
            }
        }
    }

    pub(crate) fn format_protocol_frame(&self, frame: &crate::protocol::ReceivedFrame) -> String {
        if self.settings.protocol.show_raw {
            crate::hex::to_hex_string(&frame.raw, true)
        } else {
            crate::i18n::tr_fmt(
                "地址 {} 功能 {} 序列 {} 数据[{}] {}",
                &[
                    format!("{:02X}", frame.address),
                    format!("{:02X}", frame.function),
                    format!("{:02X}", frame.sequence),
                    frame.data.len().to_string(),
                    crate::hex::to_hex_string(&frame.data, true),
                ],
            )
        }
    }

    pub(crate) fn log_protocol(&mut self, level: Level, text: String) {
        let stamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
        self.protocol_log
            .push(ProtocolLogLine { level, text, stamp });
        if self.protocol_log.len() > 500 {
            self.protocol_log.remove(0);
        }
    }

    // -- multi-send ---------------------------------------------------------

    pub(crate) fn start_multi_run(&mut self, looping: bool, now: Instant) {
        let queue = self.settings.multi.enabled_indices();
        if queue.is_empty() {
            self.warn(tr("没有已勾选的发送条目"));
            self.settings.multi.loop_enabled = false;
            return;
        }
        self.multi_cycles = 0;
        self.multi_run = Some(MultiRun {
            queue,
            position: 0,
            last_sent: None,
            next_at: now,
            wait: None,
            looping,
        });
    }

    pub(crate) fn stop_multi_run(&mut self, level: Level, reason: &str) {
        if self.multi_run.take().is_some() {
            self.settings.multi.loop_enabled = false;
            self.message(level, reason.to_owned());
        }
    }

    pub(crate) fn multi_run_active(&self) -> bool {
        self.multi_run.is_some()
    }

    pub(crate) fn multi_run_sent(&self) -> Option<SentEntry> {
        self.multi_run.as_ref().and_then(|run| run.last_sent)
    }

    pub(crate) fn multi_delay_countdown(&self, now: Instant) -> Option<MultiCountdown> {
        let wait = self.multi_run.as_ref()?.wait.as_ref()?;
        let total_ms = self.settings.multi.delay_after(wait.index);
        if total_ms == 0 {
            return None;
        }
        let total = Duration::from_millis(total_ms);
        let remaining = total.saturating_sub(now.saturating_duration_since(wait.started_at));
        Some(MultiCountdown {
            index: wait.index,
            remaining: remaining.as_secs_f32() / total.as_secs_f32(),
            remaining_ms: u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX),
        })
    }

    fn pump_multi(&mut self, now: Instant) -> bool {
        let Some(run) = self.multi_run.as_ref() else {
            return false;
        };
        if !self.link.is_open() {
            self.stop_multi_run(Level::Warning, tr("串口已关闭，多条发送已停止"));
            return true;
        }

        let due = match run.wait.as_ref() {
            Some(wait) => {
                wait.started_at + Duration::from_millis(self.settings.multi.delay_after(wait.index))
            }
            None => run.next_at,
        };
        if now < due {
            if let Some(run) = self.multi_run.as_mut() {
                run.next_at = due;
            }
            return true;
        }

        let run = self.multi_run.as_ref().expect("checked above");
        let index = run.queue[run.position];
        let position = run.position;
        let end_of_round = position + 1 == run.queue.len();
        let queue_len = run.queue.len();
        let looping = run.looping;

        let outcome = if self
            .settings
            .multi
            .entry(index)
            .is_some_and(|entry| entry.enabled)
        {
            self.send_multi_entry(index, true)
        } else {
            MultiSendOutcome::Skipped
        };
        let delay = Duration::from_millis(self.settings.multi.delay_after(index));
        let advanced = !matches!(outcome, MultiSendOutcome::Refused);

        let mut stop_looping = false;
        let mut no_entries_left = false;
        let mut next_round: Option<Vec<usize>> = None;
        if advanced && end_of_round && looping {
            self.multi_cycles += 1;
            if self
                .settings
                .multi
                .loop_count
                .is_some_and(|limit| self.multi_cycles >= limit)
            {
                stop_looping = true;
            } else {
                let queue = self.settings.multi.enabled_indices();
                if queue.is_empty() {
                    no_entries_left = true;
                } else {
                    next_round = Some(queue);
                }
            }
        }

        if stop_looping {
            let completed = self.multi_cycles;
            self.multi_run = None;
            self.settings.multi.loop_enabled = false;
            self.multi_cycles = 0;
            self.mark_settings_dirty();
            let message = crate::i18n::tr1("循环发送已完成 {} 次", completed);
            self.ok(message);
            return true;
        }
        if no_entries_left {
            self.stop_multi_run(Level::Warning, tr("没有已勾选的发送条目"));
            return true;
        }

        if let Some(run) = self.multi_run.as_mut() {
            if matches!(outcome, MultiSendOutcome::Sent) {
                run.last_sent = Some(SentEntry {
                    index,
                    ordinal: position + 1,
                    round_len: queue_len,
                });
            }
            if advanced {
                match next_round {
                    Some(queue) => {
                        run.queue = queue;
                        run.position = 0;
                    }
                    None => run.position = if end_of_round { 0 } else { position + 1 },
                }
                run.wait = Some(DelayWait {
                    index,
                    started_at: now,
                });
                run.next_at = now + delay;
            } else {
                run.wait = None;
                run.next_at = now + Duration::from_millis(5);
            }
        }

        if advanced && end_of_round && !looping {
            self.multi_run = None;
            let message = crate::i18n::tr1("已发送 {} 条已勾选条目", queue_len);
            self.ok(message);
        }
        true
    }

    pub(crate) fn send_multi_entry(&mut self, index: usize, from_loop: bool) -> MultiSendOutcome {
        let Some(entry) = self.settings.multi.entry(index) else {
            return MultiSendOutcome::Skipped;
        };
        if !entry.is_sendable() {
            if !from_loop {
                self.warn(crate::i18n::tr1("条目 {} 为空", index));
            }
            return MultiSendOutcome::Skipped;
        }
        let encoded = match self.settings.multi.encode_entry(entry) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.error(crate::i18n::tr2("条目 {} 编码失败: {}", index, err));
                return MultiSendOutcome::Skipped;
            }
        };
        let label = index.to_string();
        if self.send_multi_entry_bytes(encoded, &crate::i18n::tr1("发送条目 {}", label)) {
            MultiSendOutcome::Sent
        } else {
            MultiSendOutcome::Refused
        }
    }

    fn send_multi_entry_bytes(&mut self, bytes: Vec<u8>, what: &str) -> bool {
        if !self.link.is_open() {
            self.error(format!("{}: {}", tr("串口未打开"), what));
            return false;
        }
        match self.link.send(bytes.clone()) {
            Ok(()) => {
                self.echo_sent(&bytes);
                true
            }
            Err(err) => {
                self.error(err);
                false
            }
        }
    }

    /// Rebuild the per-row editors after a structural change to the list.
    pub(crate) fn rebuild_multi_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.multi_subs.clear();
        self.multi_inputs.clear();
        for index in 0..self.settings.multi.entries.len() {
            let (text, note, delay) = {
                let entry = &self.settings.multi.entries[index];
                (
                    entry.text.clone(),
                    entry.note.clone(),
                    entry.delay_ms.to_string(),
                )
            };
            let text_input = cx.new(|cx| InputState::new(window, cx).default_value(text));
            let note_input = cx.new(|cx| InputState::new(window, cx).default_value(note));
            let delay_input = cx.new(|cx| InputState::new(window, cx).default_value(delay));
            self.multi_subs.push(cx.subscribe_in(
                &text_input,
                window,
                move |this, state, event, _window, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Some(entry) = this.settings.multi.entries.get_mut(index)
                    {
                        entry.text = state.read(cx).value().to_string();
                        this.mark_settings_dirty();
                    }
                },
            ));
            self.multi_subs.push(cx.subscribe_in(
                &note_input,
                window,
                move |this, state, event, _window, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Some(entry) = this.settings.multi.entries.get_mut(index)
                    {
                        entry.note = state.read(cx).value().to_string();
                        this.mark_settings_dirty();
                    }
                },
            ));
            self.multi_subs.push(cx.subscribe_in(
                &delay_input,
                window,
                move |this, state, event, _window, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Ok(value) = state.read(cx).value().trim().parse::<u64>()
                        && let Some(entry) = this.settings.multi.entries.get_mut(index)
                    {
                        entry.delay_ms = value.min(3_600_000);
                        this.mark_settings_dirty();
                    }
                },
            ));
            self.multi_inputs.push(MultiRowInputs {
                text: text_input,
                note: note_input,
                delay: delay_input,
            });
        }
    }

    /// Push the current settings into the row editors (after import or reset).
    pub(crate) fn sync_multi_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (index, row) in self.multi_inputs.iter().enumerate() {
            let Some(entry) = self.settings.multi.entries.get(index) else {
                break;
            };
            let (text, note, delay) = (
                entry.text.clone(),
                entry.note.clone(),
                entry.delay_ms.to_string(),
            );
            row.text
                .update(cx, |state, cx| state.set_value(text, window, cx));
            row.note
                .update(cx, |state, cx| state.set_value(note, window, cx));
            row.delay
                .update(cx, |state, cx| state.set_value(delay, window, cx));
        }
    }

    /// Rebuild the keyword-rule editors after the rule list changes.
    pub(crate) fn rebuild_highlight_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.highlight_subs.clear();
        self.highlight_inputs.clear();
        for index in 0..self.settings.highlights.rules.len() {
            let (text, color) = {
                let rule = &self.settings.highlights.rules[index];
                (rule.text.clone(), hex_color(rule.color))
            };
            let text_input = cx.new(|cx| InputState::new(window, cx).default_value(text));
            let color_input = cx.new(|cx| InputState::new(window, cx).default_value(color));
            self.highlight_subs.push(cx.subscribe_in(
                &text_input,
                window,
                move |this, state, event, _window, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Some(rule) = this.settings.highlights.rules.get_mut(index)
                    {
                        rule.text = state.read(cx).value().to_string();
                        this.mark_settings_dirty();
                    }
                },
            ));
            self.highlight_subs.push(cx.subscribe_in(
                &color_input,
                window,
                move |this, state, event, _window, cx| {
                    if matches!(event, InputEvent::Change)
                        && let Some(rgb) = parse_color(&state.read(cx).value())
                        && let Some(rule) = this.settings.highlights.rules.get_mut(index)
                    {
                        rule.color = rgb;
                        this.mark_settings_dirty();
                    }
                },
            ));
            self.highlight_inputs.push(HighlightRowInputs {
                text: text_input,
                color: color_input,
            });
        }
    }

    pub(crate) fn add_highlight_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings
            .highlights
            .rules
            .push(crate::highlight::HighlightRule::default());
        self.rebuild_highlight_inputs(window, cx);
        self.mark_settings_dirty();
    }

    pub(crate) fn remove_highlight_rule(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index < self.settings.highlights.rules.len() {
            self.settings.highlights.rules.remove(index);
            self.rebuild_highlight_inputs(window, cx);
            self.mark_settings_dirty();
        }
    }

    /// Push the current settings back into every editor after a reset/import.
    pub(crate) fn sync_all_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let send = self.settings.send.buffer.clone();
        self.send_input
            .update(cx, |state, cx| state.set_value(send, window, cx));
        let protocol = self.settings.protocol_data.clone();
        self.protocol_input
            .update(cx, |state, cx| state.set_value(protocol, window, cx));
        let values = [
            (
                self.inputs.send_period.clone(),
                self.settings.send.period_ms.to_string(),
            ),
            (
                self.inputs.loop_count.clone(),
                self.settings
                    .multi
                    .loop_count
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            ),
            (
                self.inputs.protocol_address.clone(),
                self.settings.protocol.address.to_string(),
            ),
            (
                self.inputs.protocol_function.clone(),
                self.settings.protocol.function.to_string(),
            ),
            (
                self.inputs.protocol_period.clone(),
                self.settings.protocol.frame_period_ms.to_string(),
            ),
            (
                self.inputs.protocol_retries.clone(),
                self.settings.protocol.max_retries.to_string(),
            ),
            (
                self.inputs.protocol_data_len.clone(),
                self.settings.protocol.max_data_len.to_string(),
            ),
            (
                self.inputs.max_rows.clone(),
                self.settings.max_rows.to_string(),
            ),
            (
                self.inputs.max_raw_mb.clone(),
                (self.settings.max_raw / (1024 * 1024)).to_string(),
            ),
            (
                self.inputs.log_fg.clone(),
                hex_color(self.settings.log_colors.foreground),
            ),
            (
                self.inputs.log_bg.clone(),
                hex_color(self.settings.log_colors.background),
            ),
            (
                self.inputs.log_dir.clone(),
                self.settings.log_file.directory.clone(),
            ),
            (
                self.inputs.log_pattern.clone(),
                self.settings.log_file.pattern.clone(),
            ),
        ];
        for (entity, value) in values {
            entity.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        self.sync_multi_inputs(window, cx);
    }

    /// Run a menu or toolbar command.
    pub(crate) fn apply_action(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            Action::TogglePort => self.toggle_port(),
            Action::RefreshPorts => {
                self.ports.poll();
                self.info(tr("已刷新串口列表"));
            }
            Action::SaveReceived => self.save_received(false),
            Action::SaveRaw => self.save_received(true),
            Action::Quit => {
                cx.quit();
                return;
            }
            Action::CopySelection => self.copy_log_selection(cx),
            Action::SelectAll => self.select_all_log(),
            Action::ClearReceive => self.clear_receive(),
            Action::ClearSend => {
                self.settings.send.buffer.clear();
                self.send_input
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.tx_bytes = 0;
                self.rx_bytes = 0;
                self.mark_settings_dirty();
                self.info(tr("已清除发送区与收发计数"));
            }
            Action::ResetSettings => self.open_reset_confirm(window, cx),
            Action::ToggleConfigPanel => {
                self.settings.show_config_panel = !self.settings.show_config_panel;
                if self.settings.show_config_panel && self.settings.left_panel_width < 180.0 {
                    self.settings.left_panel_width = 300.0;
                }
                let width = self.settings.left_panel_width;
                self.main_split.update(cx, |state, cx| {
                    if state.sizes().len() >= 3 {
                        state.resize_panel(0, px(width), window, cx);
                    }
                });
                self.mark_settings_dirty();
            }
            Action::ToggleToolPanel => {
                self.settings.show_tool_panel = !self.settings.show_tool_panel;
                if self.settings.show_tool_panel && self.settings.right_panel_width < 180.0 {
                    self.settings.right_panel_width = 320.0;
                }
                let width = self.settings.right_panel_width;
                let left = self.settings.left_panel_width;
                self.main_split.update(cx, |state, cx| {
                    let sizes = state.sizes();
                    if sizes.len() >= 3 {
                        let total: f32 = sizes.iter().map(|size| f32::from(*size)).sum();
                        let center = (total - left - width).max(1.0);
                        state.resize_panel(1, px(center), window, cx);
                    }
                });
                self.mark_settings_dirty();
            }
            Action::ToggleSendArea => {
                self.settings.show_send_area = !self.settings.show_send_area;
                if self.settings.show_send_area && self.settings.send_area_height < 120.0 {
                    self.settings.send_area_height = 196.0;
                }
                let height = self.settings.send_area_height;
                self.center_split.update(cx, |state, cx| {
                    let sizes = state.sizes();
                    if sizes.len() >= 2 {
                        let total: f32 = sizes.iter().map(|size| f32::from(*size)).sum();
                        state.resize_panel(0, px((total - height).max(1.0)), window, cx);
                    }
                });
                self.mark_settings_dirty();
            }
            Action::ToggleHex => {
                let mut options = self.settings.display;
                options.mode = if options.mode == crate::logstore::ViewMode::Hex {
                    crate::logstore::ViewMode::Text
                } else {
                    crate::logstore::ViewMode::Hex
                };
                self.set_display_options(options);
            }
            Action::ToggleMarkers => {
                let mut options = self.settings.display;
                options.markers = !options.markers;
                self.set_display_options(options);
            }
            Action::ToggleTimestamp => {
                let mut options = self.settings.display;
                options.timestamp = !options.timestamp;
                self.set_display_options(options);
            }
            Action::ToggleWrap => {
                let mut options = self.settings.display;
                options.wrap = !options.wrap;
                self.set_display_options(options);
            }
            Action::ToggleAutoScroll => {
                let value = !self.settings.auto_scroll;
                self.set_auto_scroll(value);
            }
            Action::ToggleEcho => {
                self.settings.echo_sent = !self.settings.echo_sent;
                self.mark_settings_dirty();
            }
            Action::TogglePause => {
                if self.show_paused {
                    self.resume_display();
                } else {
                    self.show_paused = true;
                }
            }
            Action::SendFile => self.pick_file_to_send(),
            Action::AbortFileSend => self.abort_file_send(),
            Action::UseHistory(index) => {
                if let Some(text) = self.settings.send.history.get(index).cloned() {
                    self.settings.send.buffer = text.clone();
                    self.send_input
                        .update(cx, |state, cx| state.set_value(text, window, cx));
                    self.mark_settings_dirty();
                }
            }
            Action::MultiAdd => {
                self.settings.multi.add_entry();
                self.rebuild_multi_inputs(window, cx);
                self.mark_settings_dirty();
            }
            Action::MultiImport => self.import_multi(window, cx),
            Action::MultiExport => self.export_multi(),
            Action::MultiSendChecked => self.start_multi_run(false, Instant::now()),
            Action::MultiRemove(index) => {
                self.settings.multi.remove_entry(index);
                self.rebuild_multi_inputs(window, cx);
                self.mark_settings_dirty();
            }
            Action::MultiSendOne(index) => {
                self.send_multi_entry(index, false);
            }
            Action::MultiMove { from, to } => {
                if !self.multi_run_active() {
                    self.settings.multi.move_entry(from, to);
                    self.rebuild_multi_inputs(window, cx);
                    self.mark_settings_dirty();
                }
            }
            Action::MultiNewlineAll => {
                let all = !self
                    .settings
                    .multi
                    .entries
                    .iter()
                    .all(|entry| entry.newline);
                for entry in &mut self.settings.multi.entries {
                    entry.newline = all;
                }
                self.mark_settings_dirty();
            }
            Action::ProtocolSingle => self.start_protocol(crate::protocol::TaskKind::Single),
            Action::ProtocolAuto => self.start_protocol(crate::protocol::TaskKind::Auto),
            Action::ProtocolFile => self.start_protocol_file(),
            Action::ProtocolStop => {
                self.protocol.stop(tr("用户已停止传输"));
                self.protocol_progress = None;
            }
            Action::StartLogFile => self.set_log_file_enabled(true),
            Action::StopLogFile => self.set_log_file_enabled(false),
            Action::ShowFontSettings => self.open_font_settings(window, cx),
            Action::ShowLogColors => self.open_log_colors(window, cx),
            Action::ShowHighlight => self.open_highlight(window, cx),
            Action::ShowLogFile => self.open_log_file(window, cx),
            Action::ShowCustomBaud => self.open_custom_baud(window, cx),
            Action::ShowShortcuts => self.open_shortcuts(window, cx),
            Action::ShowAbout => self.open_about(window, cx),
            Action::HighlightAdd => self.add_highlight_rule(window, cx),
            Action::HighlightRemove(index) => self.remove_highlight_rule(index, window, cx),
        }
        cx.notify();
    }

    fn pick_file_to_send(&mut self) {
        if !self.link.is_open() {
            self.error(tr("串口未打开"));
            return;
        }
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.start_file_send(path);
        }
    }

    fn import_multi(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match self.settings.multi.import_json(&text) {
                Ok(count) => {
                    self.rebuild_multi_inputs(window, cx);
                    self.mark_settings_dirty();
                    self.ok(crate::i18n::tr1("已导入 {} 个条目", count));
                }
                Err(err) => self.error(err),
            },
            Err(err) => self.error(crate::i18n::tr1("读取文件失败: {}", err)),
        }
    }

    fn export_multi(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("xcom-entries.json")
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        match self.settings.multi.export_json() {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => self.ok(crate::i18n::tr1("已导出到 {}", path.display())),
                Err(err) => self.error(crate::i18n::tr1("写入失败: {}", err)),
            },
            Err(err) => self.error(err),
        }
    }

    fn start_protocol(&mut self, kind: crate::protocol::TaskKind) {
        let raw = self.settings.protocol_data.clone();
        let data = if raw.trim().is_empty() {
            Vec::new()
        } else {
            match crate::hex::parse(&raw) {
                Ok(bytes) => bytes,
                Err(err) => {
                    self.error(crate::i18n::tr1("数据格式错误: {}", err));
                    return;
                }
            }
        };
        if data.is_empty() {
            self.warn(tr("请先输入要发送的数据"));
            return;
        }
        self.protocol_log.clear();
        let now = Instant::now();
        match kind {
            crate::protocol::TaskKind::Single => self.protocol.start_single(data, now),
            crate::protocol::TaskKind::Auto => self.protocol.start_auto(data, now),
            crate::protocol::TaskKind::File => {}
        }
        self.info(crate::i18n::tr1("已启动{}", tr(kind.label())));
    }

    fn start_protocol_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_file() else {
            return;
        };
        match std::fs::read(&path) {
            Ok(data) => {
                self.protocol_log.clear();
                self.protocol.start_file(data, Instant::now());
                self.info(crate::i18n::tr1("已启动文件传输: {}", path.display()));
            }
            Err(err) => self.error(crate::i18n::tr1("读取文件失败: {}", err)),
        }
    }

    // -- choices ------------------------------------------------------------

    pub(crate) fn apply_choice(&mut self, choice: Choice, cx: &mut Context<Self>) {
        match choice {
            Choice::Port(name) => self.settings.serial.port = name,
            Choice::Baud(rate) => {
                self.settings.serial.baud = rate;
                self.protocol.set_baud(rate);
            }
            Choice::DataBits(value) => self.settings.serial.data_bits = value,
            Choice::Parity(value) => self.settings.serial.parity = value,
            Choice::StopBits(value) => self.settings.serial.stop_bits = value,
            Choice::Flow(value) => self.settings.serial.flow = value,
            Choice::Encoding(choice) => self.set_encoding(choice),
            Choice::TxEncoding(choice) => self.settings.tx_encoding = choice,
            Choice::Theme(theme) => {
                self.settings.theme = theme;
                self.applied_theme = None;
            }
            Choice::Language(lang) => {
                crate::i18n::set_lang(lang);
                self.settings.lang = Some(lang);
                self.applied_lang = None;
            }
            Choice::Checksum(kind) => {
                self.settings.protocol.checksum = kind;
                self.protocol.set_config(self.settings.protocol.clone());
            }
            Choice::UiFont(family) => {
                self.settings.fonts.ui_family = family;
                self.applied_fonts = None;
            }
            Choice::ReceiveFont(family) => self.settings.fonts.receive_family = family,
            Choice::SendFont(family) => self.settings.fonts.send_family = family,
            Choice::UiSize(size) => {
                self.settings.fonts.ui_size = size.clamp(9.0, 24.0);
                self.applied_fonts = None;
            }
            Choice::ReceiveSize(size) => self.settings.fonts.receive_size = size.clamp(8.0, 32.0),
            Choice::SendSize(size) => self.settings.fonts.send_size = size.clamp(8.0, 32.0),
            Choice::LineSpacing(spacing) => {
                self.settings.line_spacing = spacing.clamp(
                    crate::settings::LINE_SPACING_MIN,
                    crate::settings::LINE_SPACING_MAX,
                )
            }
        }
        self.mark_settings_dirty();
        cx.notify();
    }

    /// Apply the factory defaults everywhere.
    pub(crate) fn apply_default_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.log_file.stop();
        self.settings = crate::settings::Settings::default();
        self.log
            .set_limits(self.settings.max_rows, self.settings.max_raw);
        self.log.set_options(self.settings.display);
        crate::i18n::set_lang(crate::i18n::system_lang());
        self.applied_theme = None;
        self.applied_fonts = None;
        self.rebuild_multi_inputs(window, cx);
        self.rebuild_highlight_inputs(window, cx);
        self.sync_all_inputs(window, cx);
        self.mark_settings_dirty();
        self.info(tr("已恢复默认设置"));
    }

    // -- files --------------------------------------------------------------

    pub(crate) fn save_received(&mut self, raw: bool) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(if raw { "xcom-raw.bin" } else { "xcom.log" })
            .save_file()
        else {
            return;
        };
        let data = if raw {
            self.log.raw().to_vec()
        } else {
            self.log.render_all().into_bytes()
        };
        match std::fs::write(&path, &data) {
            Ok(()) => self.ok(crate::i18n::tr2(
                "已保存 {} 字节到 {}",
                data.len(),
                path.display(),
            )),
            Err(err) => self.error(crate::i18n::tr1("保存失败: {}", err)),
        }
    }

    pub(crate) fn copy_log_selection(&mut self, cx: &mut App) {
        if let Some(text) = self.log_view.selected_text(&self.log) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.info(tr("已复制选中内容到剪贴板"));
        } else {
            self.warn(tr("没有可复制的内容"));
        }
    }

    pub(crate) fn select_all_log(&mut self) {
        self.log_view.select_all(&self.log);
    }

    // -- appearance ---------------------------------------------------------

    fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.applied_theme != Some(self.settings.theme) {
            crate::theme::apply(cx, self.settings.theme);
            self.applied_theme = Some(self.settings.theme);
        }
        let signature = (
            self.settings.fonts.ui_family.clone(),
            self.settings.fonts.ui_size,
        );
        if self.applied_fonts.as_ref() != Some(&signature) {
            crate::theme::set_base_font(
                cx,
                self.settings.fonts.ui_family.as_deref(),
                self.settings.fonts.ui_size,
            );
            self.applied_fonts = Some(signature);
        }
        if self.applied_lang != Some(crate::i18n::lang()) {
            self.applied_lang = Some(crate::i18n::lang());
            window.set_window_title(&crate::i18n::window_title());
        }
        self.note_font_status();

        let size = window.bounds().size;
        let size = [f32::from(size.width), f32::from(size.height)];
        if size[0] > 320.0 && size[1] > 240.0 && self.last_window_size != Some(size) {
            self.last_window_size = Some(size);
            self.settings.window_size = Some(size);
            self.mark_settings_dirty();
        }
    }
}

impl Render for XcomApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_reset {
            self.needs_reset = false;
            self.apply_default_settings(window, cx);
        }
        self.apply_appearance(window, cx);
        let content = crate::ui::panels::render(self, window, cx);
        div()
            .id("xcom-root")
            .key_context(APP_CONTEXT)
            .size_full()
            .on_action(cx.listener(|this, _: &Send, _, cx| {
                this.send_once();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TogglePort, _, cx| {
                this.toggle_port();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &RefreshPorts, _, cx| {
                this.ports.poll();
                this.info(tr("已刷新串口列表"));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveLog, _, cx| {
                this.save_received(false);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveRawLog, _, cx| {
                this.save_received(true);
                cx.notify();
            }))
            .on_action(cx.listener(|_this, _: &QuitApp, _, cx| {
                cx.quit();
            }))
            .child(content)
    }
}

#[cfg(test)]
mod ui_tests {
    use super::*;
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{TestAppContext, WindowOptions};

    #[test]
    fn colour_text_round_trips_and_rejects_junk() {
        assert_eq!(parse_color("#12AB34"), Some([0x12, 0xAB, 0x34]));
        assert_eq!(parse_color("12ab34"), Some([0x12, 0xAB, 0x34]));
        assert_eq!(parse_color("#abc"), Some([0xAA, 0xBB, 0xCC]));
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("not a colour"), None);
        assert_eq!(hex_color([0x0A, 0xFF, 0x10]), "#0AFF10");
    }

    #[gpui_kit::test]
    fn the_main_window_renders_the_panels(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let settings = crate::settings::Settings {
            show_tool_panel: true,
            ..Default::default()
        };
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| XcomApp::new(window, cx, settings.clone()))
            })
            .expect("open the application window")
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            for id in [
                "menu-file",
                "menu-view",
                "send",
                "multi-add",
                "tab-protocol",
                "refresh-ports",
            ] {
                assert!(window.find(id).visible(), "{id} was not laid out");
            }
            window.click("tab-protocol", cx);
            assert!(window.find("proto-single").visible());

            // Dialog builders read the retained entity and build real
            // controls; an open/render pass proves that path does not panic.
            view.update(cx, |this, cx| this.open_font_settings(window, cx));
            window.render_frame(cx);
            assert!(window.try_find("font-ui-family").is_some());

            // Menu items run through this path: resolve the entity's current
            // window, update it, then redraw.
            let before = view.read(cx).settings.multi.entries.len();
            crate::ui::panels::run_action(
                view.downgrade(),
                crate::app::Action::MultiAdd,
                window,
                cx,
            );
            window.render_frame(cx);
            assert_eq!(view.read(cx).settings.multi.entries.len(), before + 1);

            // On Linux with client-side decorations the custom title bar
            // must offer the window controls on whichever side the desktop
            // chose.  Headless test windows may be server-decorated, in which
            // case the native frame owns them and there is nothing to assert.
            if window.try_find("title-drag").is_some() {
                let close_ids = [
                    "window-left-0-close",
                    "window-left-1-close",
                    "window-left-2-close",
                    "window-right-0-close",
                    "window-right-1-close",
                    "window-right-2-close",
                ];
                assert!(
                    close_ids.iter().any(|id| window.try_find(*id).is_some()),
                    "the title bar must offer a close control"
                );
            }

            window.close_all_dialogs(cx);
            window.render_frame(cx);
            window.click("tab-multi", cx);
            window.click("multi-add", cx);
            assert_eq!(view.read(cx).settings.multi.entries.len(), before + 2);

            // A docked panel leaves a strip that restores it.
            view.update(cx, |this, _| this.settings.show_config_panel = false);
            window.render_frame(cx);
            assert!(window.find("config-strip").visible());
            window.click("config-strip", cx);
            assert!(view.read(cx).settings.show_config_panel);
        })
        .unwrap();
    }
}

#[cfg(all(test, unix))]
mod multi_tests {
    use super::*;
    use gpui_kit::{TestAppContext, WindowOptions};

    struct TestPty {
        master: std::fs::File,
        slave_path: String,
    }

    impl TestPty {
        fn open() -> Option<Self> {
            use std::os::fd::FromRawFd;
            let mut master: libc::c_int = -1;
            let mut slave: libc::c_int = -1;
            let rc = unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if rc != 0 || master < 0 || slave < 0 {
                return None;
            }
            let master_file = unsafe { std::fs::File::from_raw_fd(master) };
            let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };
            let name = unsafe { libc::ptsname(master) };
            if name.is_null() {
                return None;
            }
            let slave_path = unsafe { std::ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned();
            let flags = unsafe { libc::fcntl(master, libc::F_GETFL) };
            if flags >= 0 {
                unsafe { libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK) };
            }
            drop(slave_file);
            Some(Self {
                master: master_file,
                slave_path,
            })
        }

        fn read_available(&mut self, timeout: Duration) -> Vec<u8> {
            use std::io::Read;
            let deadline = Instant::now() + timeout;
            let mut out = Vec::new();
            let mut buf = [0u8; 256];
            while Instant::now() < deadline {
                match self.master.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            out
        }
    }

    fn open_app(cx: &mut TestAppContext, pty: &TestPty) -> Option<Entity<XcomApp>> {
        cx.update(gpui_kit::init);
        let mut settings = crate::settings::Settings::default();
        settings.serial.port = pty.slave_path.clone();
        let (_handle, view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| XcomApp::new(window, cx, settings.clone()))
                })
            })
            .expect("open test window");
        let opened = view.update(cx, |app, _| app.link.open(&app.settings.serial).is_ok());
        if !opened {
            return None;
        }
        Some(view)
    }

    #[gpui_kit::test]
    fn editing_a_delay_retimes_the_wait_already_running(cx: &mut TestAppContext) {
        let Some(mut pty) = TestPty::open() else {
            return;
        };
        let Some(view) = open_app(cx, &pty) else {
            return;
        };
        let start = Instant::now();
        view.update(cx, |app, _| {
            app.settings.multi.entries.clear();
            let a = app.settings.multi.add_entry();
            let entry = app.settings.multi.entry_mut(a).expect("entry");
            entry.text = "A".to_owned();
            entry.enabled = true;
            entry.delay_ms = 500;
            let b = app.settings.multi.add_entry();
            let entry = app.settings.multi.entry_mut(b).expect("entry");
            entry.text = "B".to_owned();
            entry.enabled = true;
            app.start_multi_run(false, start);
            app.pump_multi(start);
            app.settings.multi.entries[0].delay_ms = 30;
            app.pump_multi(start + Duration::from_millis(40));
        });
        assert_eq!(
            String::from_utf8_lossy(&pty.read_available(Duration::from_millis(150))),
            "AB",
            "the edited delay must retime the wait in progress"
        );
    }

    #[gpui_kit::test]
    fn unchecking_an_entry_skips_it_in_the_current_round(cx: &mut TestAppContext) {
        let Some(mut pty) = TestPty::open() else {
            return;
        };
        let Some(view) = open_app(cx, &pty) else {
            return;
        };
        let start = Instant::now();
        view.update(cx, |app, _| {
            app.settings.multi.entries.clear();
            for text in ["A", "B"] {
                let index = app.settings.multi.add_entry();
                let entry = app.settings.multi.entry_mut(index).expect("entry");
                entry.text = text.to_owned();
                entry.enabled = true;
            }
            app.settings.multi.loop_count = Some(1);
            app.start_multi_run(true, start);
            app.pump_multi(start);
            app.settings.multi.entries[1].enabled = false;
            app.pump_multi(start);
        });
        assert_eq!(
            String::from_utf8_lossy(&pty.read_available(Duration::from_millis(150))),
            "A",
            "B was unchecked before its turn and must not be sent"
        );
    }
}
