//! Application state, event pumping and layout orchestration.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::Context;
use encoding_rs::Encoding;

use crate::encoding::{AutoDetector, EncodingId};
use crate::i18n::tr;
use crate::logstore::LogStore;
use crate::protocol::{ProtocolEngine, ProtocolEvent};
use crate::serial::{PortWatcher, SerialEvent, SerialLink, Signals};
use crate::settings::{FontSettings, Settings};
use crate::ui::log_view::{self, LogViewOptions, LogViewState};
use crate::ui::panels;

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
    /// Bytes still to send.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.offset)
    }

    /// Completion in `0.0..=1.0`.
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
///
/// `Refused` is the back-pressure case — the writer queue is full — and the
/// run retries the entry rather than skipping it. `Skipped` covers entries
/// that can never go out (empty or unencodable), which the run moves past.
pub(crate) enum MultiSendOutcome {
    Sent,
    Skipped,
    Refused,
}

/// A multi-send sequence in progress.
///
/// Entries are sent one at a time, with each entry's own delay applied after
/// it — including the last entry of a round, before the next round begins.
/// Pacing the sequence this way is what lets a device answer between entries.
pub(crate) struct MultiRun {
    queue: Vec<usize>,
    position: usize,
    /// The entry sent most recently; see [`SentEntry`].
    last_sent: Option<SentEntry>,
    /// When the next entry may go out.
    next_at: Instant,
    /// Whether to start another round after this one.
    looping: bool,
}

/// The entry a running multi-send sent most recently.
///
/// Recorded at send time rather than derived from the queue position: the run
/// advances `position` as part of sending, so by the time the panel is drawn it
/// already points at the entry that is *next*, and the panel has to mark the
/// one that just went out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SentEntry {
    /// 条目编号 — the `#` the panel shows, i.e. the row's index on its page.
    pub index: usize,
    /// Which entry of the round this was, counting from one.
    pub ordinal: usize,
    /// How many entries the round has.
    pub round_len: usize,
}

/// The application.
pub struct XcomApp {
    pub(crate) settings: Settings,
    pub(crate) link: SerialLink,
    pub(crate) ports: PortWatcher,
    pub(crate) log: LogStore,
    pub(crate) auto: AutoDetector,
    pub(crate) log_view: LogViewState,

    /// Whether the open port has ever appeared in the OS enumeration.
    ///
    /// Only then does its disappearance mean anything: plenty of perfectly
    /// usable ports never show up in the list at all -- legacy `ttyS*` UARTs,
    /// pseudo-terminals, and some USB bridges whose driver does not expose
    /// them.  Treating "absent from the list" as "unplugged" closed those
    /// ports a second after opening them.
    open_port_enumerated: bool,
    /// Session byte counters (reset by 清除).
    pub(crate) rx_bytes: u64,
    pub(crate) tx_bytes: u64,

    /// Bytes received while the display was paused.
    pending_rx: Vec<u8>,
    /// Whether the display is currently frozen.
    pub(crate) show_paused: bool,

    pub(crate) messages: Vec<Message>,

    /// 定时发送 bookkeeping.
    pub(crate) last_timed_send: Option<Instant>,
    /// The running multi-send sequence, if any.
    pub(crate) multi_run: Option<MultiRun>,
    /// Completed passes of the multi-send loop, for the configured limit.
    pub(crate) multi_cycles: u32,
    /// Chunked file transmission.
    pub(crate) file_send: Option<FileSend>,

    pub(crate) protocol: ProtocolEngine,
    pub(crate) protocol_log: Vec<ProtocolLogLine>,
    pub(crate) protocol_progress: Option<(usize, usize)>,

    pub(crate) side_tab: SideTab,
    pub(crate) show_about: bool,
    /// Whether the Display settings dialog is open.
    pub(crate) show_display_settings: bool,
    /// Whether the confirmation dialog for restoring defaults is open.
    pub(crate) show_reset_confirm: bool,
    /// Whether the keyboard-shortcuts dialog is open.
    pub(crate) show_shortcuts: bool,
    /// Set when the display settings page should scroll to the keyword rules.
    pub(crate) scroll_to_highlights: bool,

    /// Handshake line states, sampled by a background thread.
    pub(crate) signals: Signals,

    /// Last theme applied, so styling only runs on change.
    applied_theme: Option<crate::theme::ThemeMode>,
    /// Last language applied to the window title.
    applied_lang: Option<crate::i18n::Lang>,
    /// Last font families installed in the egui context.
    applied_fonts: Option<crate::settings::FontSettings>,
    /// Deferred settings write.
    settings_dirty: bool,
    last_save: Instant,
    /// Set when the CJK font is missing so the warning is emitted once.
    font_warning_shown: bool,
    /// Splitters collected while laying out panels, registered afterwards so
    /// they sit above every panel's contents.
    pub(crate) pending_splitters: Vec<panels::PendingSplitter>,
    /// A pending wrap-width change, with the time it was first observed.
    ///
    /// Re-wrapping re-renders every retained byte, so a window drag must not
    /// trigger it on each intermediate width.
    pending_wrap_cols: Option<(usize, Instant)>,
}

impl XcomApp {
    /// Build the application, loading persisted settings.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();

        // Restore the window size before the first frame.
        if let Some([w, h]) = settings.window_size {
            cc.egui_ctx
                .send_viewport_cmd(eframe::egui::ViewportCommand::InnerSize(
                    eframe::egui::vec2(w.max(640.0), h.max(420.0)),
                ));
        }

        // Resolve the language before the first frame and remember the choice,
        // so a system-locale guess sticks once the user has seen it.
        let lang = settings.lang.unwrap_or_else(crate::i18n::system_lang);
        crate::i18n::set_lang(lang);

        let mut app = Self::headless(settings);
        if app.settings.lang.is_none() {
            app.settings.lang = Some(lang);
            app.mark_settings_dirty();
        }

        // Apply the persisted theme now so the first frame is already correct.
        crate::theme::apply(&cc.egui_ctx, app.settings.theme);
        app.applied_theme = Some(app.settings.theme);
        // `main` installs fonts before constructing the app; record that the
        // startup definitions are already current.
        app.applied_fonts = Some(app.settings.fonts.clone());

        app
    }

    /// Build the application from explicit settings, with no egui context.
    ///
    /// Kept separate from [`new`](Self::new) so the UI can be laid out in a
    /// headless test without an eframe `CreationContext`.
    pub(crate) fn headless(settings: Settings) -> Self {
        let mut app = Self {
            log: LogStore::new(encoding_rs::UTF_8),
            auto: AutoDetector::new(),
            log_view: LogViewState::default(),
            link: SerialLink::new(),
            ports: PortWatcher::spawn(Duration::from_millis(1000)),
            settings,
            rx_bytes: 0,
            tx_bytes: 0,
            open_port_enumerated: false,
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
            show_about: false,
            show_display_settings: false,
            show_reset_confirm: false,
            show_shortcuts: false,
            scroll_to_highlights: false,
            signals: Signals::default(),
            applied_theme: None,
            applied_lang: None,
            applied_fonts: None,
            settings_dirty: false,
            last_save: Instant::now(),
            font_warning_shown: false,
            pending_wrap_cols: None,
            pending_splitters: Vec::new(),
        };

        app.protocol = ProtocolEngine::new(app.settings.protocol.clone(), app.settings.serial.baud);
        app.log
            .set_limits(app.settings.max_rows, app.settings.max_raw);
        app.log.set_options(app.settings.display);
        app.log.set_encoding(app.effective_encoding());
        app.select_default_port();

        app
    }

    // -- lifecycle ----------------------------------------------------------

    /// Pick the first available port when nothing is selected or it vanished.
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
            // Prefer a USB adapter over a legacy built-in UART.
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

    /// Report font status once, after the first frame.
    fn note_font_status(&mut self) {
        if self.font_warning_shown {
            return;
        }
        self.font_warning_shown = true;
        if let Some(ui_family) = crate::fonts::installed_ui_family() {
            log::info!("Zed UI 字体: {ui_family}");
        }
        match crate::fonts::installed_family() {
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
        // Collapse an immediate repeat rather than flooding the list.
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

    /// The most recent message, if any.
    pub(crate) fn latest_message(&self) -> Option<&Message> {
        self.messages.last()
    }

    /// Bytes received while the display was paused.
    pub(crate) fn pending_rx_len(&self) -> usize {
        self.pending_rx.len()
    }

    /// Apply a new wrap width once it has stopped changing.
    pub(crate) fn update_wrap_width(&mut self, cols: usize, ctx: &Context) {
        if cols == self.log.width_columns() {
            self.pending_wrap_cols = None;
            return;
        }
        const SETTLE: Duration = Duration::from_millis(180);
        match self.pending_wrap_cols {
            Some((pending, since)) if pending == cols && since.elapsed() >= SETTLE => {
                self.log.set_width_columns(cols);
                self.pending_wrap_cols = None;
            }
            Some((pending, _)) if pending == cols => {
                // Still settling: keep the frame clock alive.
                ctx.request_repaint_after(SETTLE);
            }
            _ => {
                self.pending_wrap_cols = Some((cols, Instant::now()));
                ctx.request_repaint_after(SETTLE);
            }
        }
    }

    // -- encodings ----------------------------------------------------------

    /// The encoding used to decode received bytes.
    pub(crate) fn effective_encoding(&self) -> &'static Encoding {
        match self.settings.encoding {
            Some(id) => id.encoding(),
            None => self.auto.active(),
        }
    }

    /// The encoding used to encode transmitted text.
    pub(crate) fn tx_encoding(&self) -> &'static Encoding {
        match self.settings.tx_encoding {
            Some(id) => id.encoding(),
            None => self.effective_encoding(),
        }
    }

    /// Human readable description of the receive encoding, for the status bar.
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

    /// Change the receive encoding, re-rendering history if needed.
    pub(crate) fn set_encoding(&mut self, choice: Option<EncodingId>) {
        if self.settings.encoding == choice {
            return;
        }
        self.settings.encoding = choice;
        if choice.is_none() {
            // Start detection over so the new data decides.
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
        // 定时发送 is a run state, so closing the port stops it and unticks the
        // box; the interval is left as configured for the next run.
        self.settings.send.timed = false;
        if was_open {
            self.info(tr("串口已关闭"));
        }
    }

    pub(crate) fn toggle_port(&mut self) {
        if self.link.is_open() {
            self.close_port();
        } else {
            self.open_port();
        }
    }

    // -- sending ------------------------------------------------------------

    /// Encode the single-send buffer according to the current mode.
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

    /// Send the single-send buffer.
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

    /// Mirror transmitted bytes into the receive view, when enabled.
    ///
    /// Called from the text-like send paths only.  Echoing a file transfer
    /// would bury the receive view under a line per chunk, and protocol frames
    /// already have their own log.
    pub(crate) fn echo_sent(&mut self, bytes: &[u8]) {
        if !self.settings.echo_sent || bytes.is_empty() {
            return;
        }
        let now = chrono::Local::now().timestamp_millis();
        self.log.append_sent(bytes, now);
    }

    /// Queue raw bytes, reporting a failure through the status bar.
    pub(crate) fn send_bytes(&mut self, bytes: Vec<u8>, what: &str) {
        if !self.link.is_open() {
            self.error(crate::i18n::tr1("串口未打开，无法{}", what));
            return;
        }
        if let Err(err) = self.link.send(bytes) {
            self.error(err);
        }
    }

    /// Begin sending a file over the plain serial link.
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

    /// Abort an in-progress file transmission.
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

    // -- settings -----------------------------------------------------------

    /// Turn "auto-scroll to newest" on or off.
    ///
    /// Enabling it also drops the text selection: a selection suspends
    /// following so it is not dragged away from the pointer, and keeping it
    /// would make the switch appear to do nothing.
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

    /// Persist settings if something changed and the user paused interacting.
    fn maybe_save_settings(&mut self, ctx: &Context) {
        if !self.settings_dirty {
            return;
        }
        if self.last_save.elapsed() < Duration::from_secs(2) {
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        self.settings.window_size = ctx.input(|i| {
            i.viewport()
                .inner_rect
                .map(|rect| [rect.width(), rect.height()])
        });
        match self.settings.save() {
            Ok(()) => self.settings_dirty = false,
            Err(err) => {
                // Do not spam: report once and stop retrying every frame.
                self.settings_dirty = false;
                self.error(crate::i18n::tr1("保存设置失败: {}", err));
            }
        }
        self.last_save = Instant::now();
    }

    // -- event pumping ------------------------------------------------------

    /// Drain the serial link and route bytes to the log and protocol engine.
    fn pump_serial(&mut self, ctx: &Context) {
        if self.ports.poll() {
            let port = self.settings.serial.port.clone();
            let present = !port.is_empty() && self.ports.find(&port).is_some();
            if present {
                self.open_port_enumerated = true;
            }
            // Only a port we had actually seen vanish counts as unplugged.
            // Many working ports never appear in the enumeration at all.
            if self.link.is_open() && self.open_port_enumerated && !present {
                self.warn(crate::i18n::tr1("{} 已断开", port));
                self.close_port();
            }
            self.select_default_port();
        }

        let now = Instant::now();
        let epoch_ms = chrono::Local::now().timestamp_millis();
        let mut saw_data = false;

        // Coalesce all Data events into one buffer before handing them to the
        // log store and protocol engine.  On Windows, ReadFile with
        // COMMTIMEOUTS returns immediately when any bytes are available, so the
        // reader thread can produce many tiny chunks per frame.  Calling
        // `ingest` (and therefore `LogStore::append` -> `refresh_cur_rows`)
        // once per chunk made the per-frame cost O(chunks x live_line_len);
        // batching reduces it to O(total_bytes + live_line_len).
        let mut rx_buf: Vec<u8> = Vec::new();

        for event in self.link.poll() {
            match event {
                SerialEvent::Data(bytes) => {
                    saw_data = true;
                    self.rx_bytes += bytes.len() as u64;
                    rx_buf.extend_from_slice(&bytes);
                }
                SerialEvent::Written(count) => {
                    self.tx_bytes += count as u64;
                }
                SerialEvent::WriteFailed(err) => self.error(err),
                SerialEvent::Lost(err) => {
                    self.error(crate::i18n::tr1("{}（串口可能已被拔出）", err));
                    self.close_port();
                }
            }
        }

        if !rx_buf.is_empty() {
            if self.show_paused {
                self.pending_rx.extend_from_slice(&rx_buf);
            } else {
                self.ingest(&rx_buf, epoch_ms);
            }
            if self.protocol.is_running() || self.protocol_panel_visible() {
                self.protocol.on_bytes(&rx_buf, now);
            }
        }

        if saw_data && !self.show_paused {
            // Throttle to ~60 FPS so the UI thread retains idle time to process
            // pointer and keyboard input even while data is streaming in.
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        // The handshake lines are polled by a background thread; reading the
        // cached snapshot is a zero-syscall lock+copy, so it is cheap enough
        // to do every frame.
        if self.link.is_open()
            && let Some(signals) = self.link.signals()
        {
            self.signals = signals;
        }

        if self.link.is_open() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Decode and file away newly received bytes.
    fn ingest(&mut self, bytes: &[u8], epoch_ms: i64) {
        let encoding = if self.settings.encoding.is_none() {
            self.auto.feed(bytes)
        } else {
            self.effective_encoding()
        };
        self.log.set_encoding(encoding);
        self.log.append(bytes, epoch_ms);
    }

    fn protocol_panel_visible(&self) -> bool {
        self.settings.show_tool_panel && self.side_tab == SideTab::Protocol
    }

    /// Release buffered bytes after unpausing.
    pub(crate) fn resume_display(&mut self) {
        self.show_paused = false;
        if !self.pending_rx.is_empty() {
            let pending = std::mem::take(&mut self.pending_rx);
            let epoch_ms = chrono::Local::now().timestamp_millis();
            self.ingest(&pending, epoch_ms);
            self.info(crate::i18n::tr1("已恢复显示，补入 {} 字节", pending.len()));
        }
    }

    /// Drive the timed send, multi-send loop, file transmission and protocol
    /// state machine.
    fn pump_timers(&mut self, ctx: &Context) {
        let now = Instant::now();

        // 定时发送
        if self.settings.send.timed && self.link.is_open() {
            let period = Duration::from_millis(self.settings.send.period_ms.max(1));
            let due = self
                .last_timed_send
                .is_none_or(|last| now.duration_since(last) >= period);
            if due {
                self.last_timed_send = Some(now);
                self.send_once();
            }
            ctx.request_repaint_after(Duration::from_millis(10));
        } else {
            self.last_timed_send = None;
        }

        // 多条发送: driven by the sequence so per-entry delays apply.
        self.pump_multi(ctx, now);

        // Chunked file transmission
        self.pump_file_send(ctx, now);

        // Protocol transfer
        self.pump_protocol(ctx, now);
    }

    fn pump_file_send(&mut self, ctx: &Context, now: Instant) {
        // Gather everything needed before mutating, so no borrows overlap.
        let Some(send) = self.file_send.as_ref() else {
            return;
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
            return;
        }
        if remaining == 0 {
            self.file_send = None;
            self.ok(crate::i18n::tr1("文件发送完成（{} 字节）", total));
            return;
        }
        if !due {
            ctx.request_repaint_after(Duration::from_millis(2));
            return;
        }

        if let Some(state) = self.file_send.as_mut() {
            state.last = now;
            state.offset = end;
        }
        if let Err(err) = self.link.send(chunk) {
            self.error(err);
            self.file_send = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(2));
    }

    fn pump_protocol(&mut self, ctx: &Context, now: Instant) {
        if !self.protocol.is_running() {
            // Still let the engine delimit and report unsolicited frames.
            if self.protocol_panel_visible() {
                let _ = self.protocol.tick(now);
                self.drain_protocol_events();
            }
            return;
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
        ctx.request_repaint_after(Duration::from_millis(5));
    }

    fn drain_protocol_events(&mut self) {
        for event in self.protocol.drain_events() {
            match event {
                ProtocolEvent::Sent { .. } | ProtocolEvent::Retransmit { .. } => {
                    // Already reported at the transmit site with full bytes.
                }
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

    /// Render a received frame for the protocol log.
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

    /// Start sending every checked entry in the pool, in order.
    ///
    /// `looping` repeats the sequence until stopped or the configured pass
    /// limit is reached; each entry's own delay paces it.
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
            looping,
        });
    }

    /// Abandon any sequence in progress.
    ///
    /// The loop switch is a run state rather than a preference, so stopping the
    /// run clears it too — otherwise the box stays ticked after the port is
    /// closed and reads as "still looping" with nothing running.
    pub(crate) fn stop_multi_run(&mut self, level: Level, reason: &str) {
        if self.multi_run.take().is_some() {
            self.settings.multi.loop_enabled = false;
            self.message(level, reason.to_owned());
        }
    }

    pub(crate) fn multi_run_active(&self) -> bool {
        self.multi_run.is_some()
    }

    /// The entry the running multi-send sent most recently, if any.
    pub(crate) fn multi_run_sent(&self) -> Option<SentEntry> {
        self.multi_run.as_ref().and_then(|run| run.last_sent)
    }

    /// Advance the running sequence.  Sends at most one entry per call.
    fn pump_multi(&mut self, ctx: &Context, now: Instant) {
        let Some(run) = self.multi_run.as_ref() else {
            return;
        };
        if !self.link.is_open() {
            self.stop_multi_run(Level::Warning, tr("串口已关闭，多条发送已停止"));
            return;
        }
        if now < run.next_at {
            ctx.request_repaint_after(run.next_at - now);
            return;
        }

        // Copy out what the send needs, then let go of the borrow.
        let index = run.queue[run.position];
        let end_of_round = run.position + 1 == run.queue.len();
        let queue_len = run.queue.len();
        let looping = run.looping;

        let outcome = self.send_multi_entry(index, true);
        let delay = Duration::from_millis(self.settings.multi.delay_after(index));

        // `Refused` (the writer queue is saturated) holds the position so the
        // entry is retried once the writer drains, instead of being skipped.
        // `Sent` and `Skipped` both advance: a round is complete once we move
        // past its last entry, whether or not that entry went out.
        let advanced = !matches!(outcome, MultiSendOutcome::Refused);

        let mut stop_looping = false;
        if advanced && end_of_round && looping {
            self.multi_cycles += 1;
            stop_looping = self
                .settings
                .multi
                .loop_count
                .is_some_and(|limit| self.multi_cycles >= limit);
        }

        if let Some(run) = self.multi_run.as_mut() {
            if matches!(outcome, MultiSendOutcome::Sent) {
                run.last_sent = Some(SentEntry {
                    index,
                    ordinal: run.position + 1,
                    round_len: queue_len,
                });
            }
            if advanced {
                // A round wraps after its last entry, but the last entry's own
                // delay still applies — there is no separate round gap.
                run.position = if end_of_round { 0 } else { run.position + 1 };
                run.next_at = now + delay;
            } else {
                // Queue saturated: nudge the next attempt just far enough
                // ahead that the run does not spin at the UI frame rate.
                run.next_at = now + Duration::from_millis(5);
            }
        }

        if stop_looping {
            let limit = self.settings.multi.loop_count.unwrap_or(0);
            self.multi_run = None;
            self.settings.multi.loop_enabled = false;
            self.multi_cycles = 0;
            self.mark_settings_dirty();
            let message = crate::i18n::tr1("循环发送已完成 {} 次", limit);
            self.ok(message);
        } else if advanced && end_of_round && !looping {
            self.multi_run = None;
            let message = crate::i18n::tr1("已发送 {} 条已勾选条目", queue_len);
            self.ok(message);
        }
        ctx.request_repaint_after(Duration::from_millis(5));
    }

    /// Encode and send one multi-send entry by its global index.
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
            // The writer queue is full or the link dropped mid-send: the
            // caller treats this as back-pressure and retries the entry.
            MultiSendOutcome::Refused
        }
    }

    /// Queue an already-encoded multi-send payload, reporting whether it went.
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

    // -- shortcuts ----------------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &Context) {
        // Ctrl+Enter sends, matching the original tool.
        let send = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        if send {
            self.send_once();
        }
        let toggle_port = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O));
        if toggle_port {
            self.toggle_port();
        }
        let refresh_ports = ctx.input(|i| i.key_pressed(egui::Key::F5));
        if refresh_ports {
            self.ports.poll();
            self.info(tr("已刷新串口列表"));
        }
        // The desktop convention for "open the settings dialog".
        let display_settings =
            ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma));
        if display_settings {
            self.show_display_settings = true;
        }
        // Save / Save As, matching the shortcuts shown in the File menu.
        let save =
            ctx.input(|i| i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::S));
        if save {
            panels::save_log(self);
        }
        let save_raw =
            ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::S));
        if save_raw {
            panels::save_raw_log(self);
        }
        let quit = ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q));
        if quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Apply the factory defaults everywhere, including the parts that live
    /// outside `settings` (the live log options and the UI language).
    pub(crate) fn apply_default_settings(&mut self) {
        self.settings = crate::settings::Settings::default();
        self.log
            .set_limits(self.settings.max_rows, self.settings.max_raw);
        self.log.set_options(self.settings.display);
        // `Settings::default()` has `lang: None` ("follow the system"), so the
        // live language must follow too or the session keeps the old language
        // until restart.
        crate::i18n::set_lang(crate::i18n::system_lang());
        self.mark_settings_dirty();
        self.info(tr("已恢复默认设置"));
    }
}

impl eframe::App for XcomApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        crate::ui::window::clear_color(crate::theme::panel_background(visuals.dark_mode))
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.window_size = None;
        if let Err(err) = self.settings.save() {
            log::error!("退出时保存设置失败: {err}");
        }
    }
}

impl XcomApp {
    /// Draw one complete frame.
    ///
    /// eframe 0.36 roots the app at a `Ui` rather than a `Context`, so the
    /// context is taken from it and used for the top-level panels.  Split out
    /// of the `App` impl so the headless test can lay out a real frame.
    pub(crate) fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;

        // The window backdrop goes down first, so every panel paints on top of
        // it.  Panels are transparent on platforms where we own the frame (see
        // `ui::window`), otherwise they would square off the bottom corners.
        crate::ui::window::paint_background(
            ui,
            crate::theme::panel_background(crate::theme::is_dark(ctx, self.settings.theme)),
        );

        // Styling only needs to be re-applied when the theme changes.
        let theme = self.settings.theme;
        if self.applied_theme != Some(theme) {
            crate::theme::apply(ctx, theme);
            self.applied_theme = Some(theme);
        }
        let fonts = self.settings.fonts.clone();
        if self
            .applied_fonts
            .as_ref()
            .map(FontSettings::family_signature)
            != Some(fonts.family_signature())
        {
            #[cfg(test)]
            crate::fonts::install_default_families(ctx);
            #[cfg(not(test))]
            crate::fonts::install(ctx, &fonts);
        }
        crate::fonts::apply_text_sizes(ctx, &fonts);
        self.applied_fonts = Some(fonts);
        // The title bar is not re-read from the app, so switching language has
        // to push a new one; otherwise a Chinese UI keeps an English title
        // until the next launch.
        let language = crate::i18n::lang();
        if self.applied_lang != Some(language) {
            self.applied_lang = Some(language);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(crate::i18n::window_title()));
        }

        // egui's default grab band is 5 px, which is easy to miss and presents
        // as "the separator cannot be dragged at all".
        ui.style_mut().interaction.resize_grab_radius_side = 8.0;

        self.note_font_status();

        self.pump_serial(ctx);
        self.pump_timers(ctx);
        self.handle_shortcuts(ctx);

        panels::menu_bar(self, ui);
        panels::status_bar(self, ui);
        panels::left_panel(self, ui);
        panels::tool_panel(self, ui);
        panels::center_panel(self, ui);
        panels::apply_splitters(self, ui);
        panels::display_settings_window(self, ctx);
        panels::reset_confirm_window(self, ctx);
        panels::shortcuts_window(self, ctx);
        panels::about_window(self, ctx);

        self.maybe_save_settings(ctx);
    }
}

/// Draw the receive log.  Kept here so the panel code can call it without
/// depending on the log view module's internals.
pub(crate) fn draw_log(
    app: &mut XcomApp,
    ui: &mut egui::Ui,
    palette: crate::theme::LogPalette,
) -> log_view::LogViewOutput {
    // Tell the store how many columns fit, so text wrapping matches the panel.
    let font = crate::fonts::receive_font_id(app.settings.fonts.receive_size);
    let char_width = ui.ctx().fonts_mut(|f| f.glyph_width(&font, 'M'));
    if char_width > 0.0 && app.settings.display.wrap {
        let cols = (ui.available_width() / char_width).floor().max(8.0) as usize;
        app.update_wrap_width(cols, ui.ctx());
    }

    let options = LogViewOptions {
        auto_scroll: app.settings.auto_scroll,
        font,
        line_spacing: app.settings.line_spacing,
        paused: app.show_paused,
        display: app.settings.display,
        echo_sent: app.settings.echo_sent,
        highlights: &app.settings.highlights,
    };
    // `show` needs &LogStore and &mut LogViewState from the same struct.
    let XcomApp { log, log_view, .. } = app;
    log_view::show(ui, log, palette, log_view, &options)
}

/// Headless layout tests.
///
/// The panel code is the largest surface in the program and the easiest place
/// for a panic to hide, because it only runs on a real frame.  `run_ui` gives
/// us the same root `Ui` that eframe hands to `App::ui`, so a full frame can be
/// laid out and tessellated without a window or a GPU.
#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::logstore::{HexStyle, ViewMode};
    use crate::theme::ThemeMode;

    /// A frame input with a monotonically increasing time.
    ///
    /// Every frame in these tests must take its time from here: egui panics if
    /// time ever goes backwards, and a frame with no time advances its clock by
    /// `predicted_dt`, which would overtake a fixed value.
    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 840.0),
            )),
            time: Some(next_time()),
            ..Default::default()
        }
    }

    /// Lay out two frames and assert the result is not empty.
    ///
    /// Two passes matter: the first populates panel state (scroll areas, grid
    /// widths) and the second re-reads it, which is where sizing bugs show up.
    fn render(app: &mut XcomApp, ctx: &egui::Context, what: &str) {
        if app.applied_fonts.is_none() {
            crate::fonts::install_default_families(ctx);
        }
        for pass in 0..2 {
            let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
            let shapes = output.shapes.len();
            // A real renderer applies these; the test only cares that the frame
            // laid out, so discard them explicitly rather than leaking.
            output.textures_delta.clear();
            assert!(shapes > 0, "{what}: pass {pass} produced no shapes");
        }
    }

    fn seeded() -> XcomApp {
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.log
            .append("AT+GMR\r\nOK\r\n温度: 25.6C\r\n".as_bytes(), 0);
        app.rx_bytes = 31;
        app.tx_bytes = 8;
        app.settings.send.buffer = "AT".to_owned();
        app.settings.send.history = vec!["AT".to_owned(), "AT+RST".to_owned()];
        app.settings.serial.port = "/dev/ttyUSB0".to_owned();
        app
    }

    /// A frame with an optional pointer press at `pos`.
    fn pointer(pos: egui::Pos2, pressed: Option<bool>) -> egui::RawInput {
        let mut raw = input();
        raw.events.push(egui::Event::PointerMoved(pos));
        if let Some(pressed) = pressed {
            raw.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        raw
    }

    /// Lay out every real panel for one frame, returning the centre column.
    fn full_frame(ctx: &egui::Context, app: &mut XcomApp, raw: egui::RawInput) -> egui::Rect {
        crate::fonts::install_default_families(ctx);
        let slot = std::cell::Cell::new(egui::Rect::NOTHING);
        let mut output = ctx.run_ui(raw, |ui| {
            panels::menu_bar(app, ui);
            panels::status_bar(app, ui);
            panels::left_panel(app, ui);
            panels::tool_panel(app, ui);
            slot.set(ui.available_rect_before_wrap());
            panels::center_panel(app, ui);
            // Mirrors `draw`: the splitters are registered after every panel.
            panels::apply_splitters(app, ui);
        });
        output.textures_delta.clear();
        slot.get()
    }

    /// Press at `from`, drag to `to`, release.  Every frame lays out the real
    /// panels, which is required: a panel that is not laid out never registers
    /// its resize handle, so there would be nothing to grab.
    /// A monotonically increasing frame time.
    ///
    /// egui panics if time goes backwards, so every drag in a test must
    /// continue from where the previous one stopped rather than restarting.
    fn next_time() -> f64 {
        thread_local! {
            static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(1000.0) };
        }
        TIME.with(|t| {
            let next = t.get() + 0.02;
            t.set(next);
            next
        })
    }

    fn drag(ctx: &egui::Context, app: &mut XcomApp, from: egui::Pos2, to: egui::Pos2) {
        let positions = [from, from, to, to, to];
        let presses = [None, Some(true), None, None, Some(false)];
        for (position, pressed) in positions.into_iter().zip(presses) {
            full_frame(ctx, app, pointer(position, pressed));
        }
    }

    fn click(ctx: &egui::Context, app: &mut XcomApp, pos: egui::Pos2) {
        for pressed in [None, Some(true), Some(false)] {
            full_frame(ctx, app, pointer(pos, pressed));
        }
    }

    #[test]
    fn open_port_button_is_shifted_left_of_center() {
        let ctx = egui::Context::default();
        let mut app = seeded();

        full_frame(&ctx, &mut app, input());

        let content = panels::probe::serial_content_rect();
        let button = panels::probe::open_button_rect();
        assert!(content.width() > 0.0, "serial content was not laid out");
        assert!(button.width() > 0.0, "open button was not laid out");
        let offset = button.center().x - content.center().x;
        assert!(
            (offset + 16.0).abs() <= 0.75,
            "open button offset should be -16.00px from the serial section centre, got {offset:.2}px"
        );
    }

    #[test]
    fn english_refresh_button_stays_inside_serial_content() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::EnUs);
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.left_panel_width = crate::ui::panels::LEFT_PANEL_MIN;

        full_frame(&ctx, &mut app, input());

        let content = panels::probe::serial_content_rect();
        let refresh = panels::probe::refresh_button_rect();
        let combo = panels::probe::serial_combo_rects()
            .into_iter()
            .find(|(name, _)| *name == "port")
            .map(|(_, rect)| rect)
            .expect("port combo was not laid out");
        assert!(refresh.width() > 0.0, "refresh button was not laid out");
        assert!(
            refresh.max.x <= content.max.x + 0.25,
            "refresh button is clipped by serial content: {refresh:?} outside {content:?}"
        );
        assert!(
            combo.max.x <= content.max.x + 0.25,
            "port combo is clipped by serial content: {combo:?} outside {content:?}"
        );
        // Leave the global language as the rest of the suite expects it: a
        // leaked EnUs changes wrapped-string widths in unrelated layout tests.
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
    }

    #[test]
    fn serial_port_combo_and_refresh_share_row_in_both_languages() {
        let _guard = crate::i18n::test_lock();
        for language in [crate::i18n::Lang::ZhCn, crate::i18n::Lang::EnUs] {
            crate::i18n::set_lang(language);
            let ctx = egui::Context::default();
            let mut app = seeded();
            for _ in 0..2 {
                full_frame(&ctx, &mut app, input());
            }

            let combos = panels::probe::serial_combo_rects();
            let find_combo = |name: &str| {
                combos
                    .iter()
                    .find(|(candidate, _)| *candidate == name)
                    .map(|(_, rect)| *rect)
                    .unwrap_or_else(|| panic!("{language:?}: {name} combo missing"))
            };
            let combo = find_combo("port");
            let baud_combo = find_combo("baud");
            let data_bits = find_combo("data_bits");
            let refresh = panels::probe::refresh_button_rect();
            let custom_baud_button = panels::probe::custom_baud_button_rect();
            assert!(combo.width() > 0.0, "{language:?}: port combo missing");
            assert!(
                data_bits.width() > 0.0,
                "{language:?}: data-bits combo missing"
            );
            assert!(refresh.width() > 0.0, "{language:?}: refresh missing");
            for name in ["baud", "stop_bits", "parity", "flow"] {
                let rect = find_combo(name);
                assert!(
                    rect.width() > 0.0,
                    "{language:?}: {name} combo was not laid out"
                );
            }
            for (name, rect) in combos {
                if name == "port" || name == "baud" {
                    assert!(
                        rect.width() <= data_bits.width() + 0.25,
                        "{language:?}: {name} combo plus its icon should fit within the standard combo column"
                    );
                } else {
                    assert!(
                        (rect.width() - data_bits.width()).abs() <= 0.25,
                        "{language:?}: {name} combo width {:.2} differs from data-bits {:.2}",
                        rect.width(),
                        data_bits.width()
                    );
                }
            }
            let content = panels::probe::serial_content_rect();
            assert!(
                combo.max.x <= content.max.x + 0.25,
                "{language:?}: port combo is clipped: {combo:?} outside {content:?}"
            );
            assert!(
                (combo.center().y - refresh.center().y).abs() <= 0.5,
                "{language:?}: refresh moved to another row: {combo:?} vs {refresh:?}"
            );
            assert!(
                refresh.min.x >= combo.max.x,
                "{language:?}: refresh must be to the right of the port selector: {combo:?} vs {refresh:?}"
            );
            assert!(
                custom_baud_button.width() > 0.0,
                "{language:?}: custom-baud manager button was not laid out"
            );
            assert!(
                custom_baud_button.min.x >= baud_combo.max.x,
                "{language:?}: custom-baud manager button must sit to the right of the baud selector"
            );
            assert!(
                (custom_baud_button.center().y - baud_combo.center().y).abs() <= 0.5,
                "{language:?}: custom-baud manager button is not vertically centered with the baud selector"
            );
            assert!(
                !combo.intersects(refresh),
                "{language:?}: port combo overlaps refresh: {combo:?} vs {refresh:?}"
            );
        }
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
    }

    #[test]
    fn custom_baud_manager_adds_and_removes_rates_from_the_popup() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let ctx = egui::Context::default();
        let mut app = seeded();

        full_frame(&ctx, &mut app, input());
        let manage = panels::probe::custom_baud_button_rect();
        assert!(manage.width() > 0.0, "custom-baud manager button missing");
        click(&ctx, &mut app, manage.center());

        let popup = panels::probe::custom_baud_window_rect();
        let input_rect = panels::probe::custom_baud_input_rect();
        let add = panels::probe::custom_baud_add_rect();
        assert!(
            popup.width() > 0.0,
            "custom-baud manager popup did not open"
        );
        assert!(input_rect.width() > 0.0, "custom-baud input missing");
        assert!(add.width() > 0.0, "custom-baud add button missing");
        assert!(!app.settings.serial.custom_bauds.contains(&115_201));
        click(&ctx, &mut app, add.center());
        assert!(
            app.settings.serial.custom_bauds.contains(&115_201),
            "clicking Add did not persist the entered baud rate"
        );
        assert_eq!(app.settings.serial.baud, 115_200);
        assert!(app.settings.serial.baud_choices().contains(&115_201));

        full_frame(&ctx, &mut app, input());
        let delete = panels::probe::custom_baud_delete_rects()
            .into_iter()
            .find(|(baud, _)| *baud == 115_201)
            .map(|(_, rect)| rect)
            .expect("the newly added baud rate was not listed in the popup");
        click(&ctx, &mut app, delete.center());
        assert!(
            !app.settings.serial.custom_bauds.contains(&115_201),
            "clicking Delete did not remove the baud rate"
        );
        assert!(!app.settings.serial.baud_choices().contains(&115_201));

        // The dialog draws its own header, so its close button must work too.
        full_frame(&ctx, &mut app, input());
        let close = panels::probe::custom_baud_close_rect();
        assert!(close.width() > 0.0, "the dialog close button is missing");
        click(&ctx, &mut app, close.center());
        // Closing fades the window out, so the dialog only reports as gone once
        // the animation finishes.
        let mut dismissed = false;
        for _ in 0..90 {
            full_frame(&ctx, &mut app, input());
            if panels::probe::custom_baud_window_rect() == egui::Rect::NOTHING {
                dismissed = true;
                break;
            }
        }
        assert!(
            dismissed,
            "clicking the custom close button did not dismiss the dialog"
        );
    }

    #[test]
    fn custom_baud_button_draws_an_icon_instead_of_a_plus() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let ctx = egui::Context::default();
        let mut app = seeded();

        full_frame(&ctx, &mut app, input());

        let manage = panels::probe::custom_baud_button_rect();
        assert!(manage.width() > 0.0, "custom-baud manager button missing");
        let label = panels::probe::custom_baud_button_label();
        assert!(
            !label.contains('+'),
            "the custom-baud button must not fall back to a plus sign, got {label:?}"
        );
        assert!(
            panels::probe::custom_baud_icon_shapes() > 0,
            "the custom-baud button painted no icon geometry"
        );
    }

    #[test]
    fn custom_baud_dialog_title_is_body_sized_not_heading_sized() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let ctx = egui::Context::default();
        let mut app = seeded();

        full_frame(&ctx, &mut app, input());
        let manage = panels::probe::custom_baud_button_rect();
        click(&ctx, &mut app, manage.center());

        let window = panels::probe::custom_baud_window_rect();
        let header = panels::probe::custom_baud_header_rect();
        let title = panels::probe::custom_baud_title_rect();
        assert!(window.width() > 0.0, "the dialog did not open");
        assert!(header.height() > 0.0, "the dialog header was not laid out");
        assert!(title.height() > 0.0, "the dialog title was not laid out");

        // The window's own title bar used `TextStyle::Heading` (~1.38x body),
        // which is what made the dialog title look oversized.
        let body_row = ctx.fonts_mut(|fonts| {
            fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
        });
        let ratio = title.height() / body_row;
        assert!(
            (ratio - 1.0).abs() <= 0.05,
            "the dialog title is {ratio:.2}x the body row height ({:.1}px vs {:.1}px)",
            title.height(),
            body_row
        );
        let top_gap = header.top() - window.top();
        assert!(
            top_gap <= 10.0,
            "the dialog header leaves a {top_gap:.1}px band above it"
        );
    }

    /// Both dialogs put their close button where the desktop puts it.
    ///
    /// egui's own title bar hard-codes the button on the right, so each dialog
    /// draws its own header; this is the test that they agree — with the
    /// desktop *and* with each other.
    #[test]
    fn every_dialog_close_button_follows_the_desktop_layout() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_about = true;

        full_frame(&ctx, &mut app, input());
        let manage = panels::probe::custom_baud_button_rect();
        click(&ctx, &mut app, manage.center());
        render(&mut app, &ctx, "dialogs");

        let on_the_left = crate::ui::dialog::close_on_left();
        for (name, window, header, title, close) in [
            (
                "custom baud",
                panels::probe::custom_baud_window_rect(),
                panels::probe::custom_baud_header_rect(),
                panels::probe::custom_baud_title_rect(),
                panels::probe::custom_baud_close_rect(),
            ),
            (
                "about",
                panels::probe::about_window_rect(),
                panels::probe::about_header_rect(),
                panels::probe::about_title_rect(),
                panels::probe::about_close_rect(),
            ),
        ] {
            assert!(window.width() > 0.0, "{name}: the dialog was not laid out");
            assert!(header.height() > 0.0, "{name}: no header was drawn");
            assert!(
                close.width() > 0.0,
                "{name}: the close button was not laid out"
            );
            // Title and close button share the header row, so moving the button
            // moved nothing else out of the bar.
            assert!(
                header.contains_rect(title) && header.contains_rect(close),
                "{name}: the header must hold both the title {title:?} and the \
                 close button {close:?}, but is {header:?}"
            );
            assert_eq!(
                close.center().x < window.center().x,
                on_the_left,
                "{name}: the close button is on the wrong side \
                 (the desktop puts it on the {}), close {close:?} window {window:?}",
                if on_the_left { "left" } else { "right" },
            );
        }
    }

    #[test]
    fn rust_version_requirement_is_1_90_0() {
        let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
            .expect("Cargo.toml is readable");
        assert!(
            manifest.contains("rust-version = \"1.90.0\""),
            "Cargo.toml must require Rust 1.90.0"
        );
    }

    /// Every action the workflows run must have moved off Node.js 20.
    ///
    /// GitHub forces the Node.js 20 majors onto Node.js 24 and annotates every
    /// job that uses one, which is the warning the build jobs were reporting.
    /// A `uses:` pin is invisible to the compiler, so the guard reads the
    /// workflow files: it goes red on exactly the pins GitHub complains about.
    #[test]
    fn workflow_actions_run_on_node_24() {
        // The first major whose published `action.yml` declares
        // `runs.using: node24`.  Anything below it is a Node.js 20 pin.
        const NODE_24_FROM: &[(&str, u32)] = &[
            ("actions/checkout", 5),
            ("actions/upload-artifact", 6),
            ("actions/download-artifact", 7),
            ("softprops/action-gh-release", 3),
        ];
        // Actions that cannot be gated this way: a composite action runs no
        // Node.js at all, and `Swatinem/rust-cache` moved to Node.js 24 inside
        // its `v2` line, so the floating tag already resolves to it.
        const UNGATED: &[&str] = &["dtolnay/rust-toolchain", "Swatinem/rust-cache"];

        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/.github/workflows");
        let mut pins = 0;
        for entry in std::fs::read_dir(dir).expect("the workflows directory is readable") {
            let path = entry.expect("a directory entry").path();
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yml" | "yaml")
            ) {
                continue;
            }
            let workflow = path.file_name().unwrap_or_default().to_string_lossy();
            let text = std::fs::read_to_string(&path).expect("a workflow is readable");
            for (number, line) in text.lines().enumerate() {
                let line = line.trim();
                let Some(uses) = line
                    .strip_prefix("- uses: ")
                    .or_else(|| line.strip_prefix("uses: "))
                else {
                    continue;
                };
                let (action, reference) = uses.trim().split_once('@').unwrap_or((uses, ""));
                pins += 1;
                if UNGATED.contains(&action) {
                    continue;
                }
                let Some((_, minimum)) = NODE_24_FROM.iter().find(|(name, _)| *name == action)
                else {
                    panic!(
                        "{workflow}:{}: {action} is not registered here; add its first \
                         Node.js 24 major to keep the pin honest",
                        number + 1
                    );
                };
                let major = reference
                    .trim_start_matches('v')
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or_else(|_| {
                        panic!(
                            "{workflow}:{}: {action} is pinned to {reference:?}",
                            number + 1
                        )
                    });
                assert!(
                    major >= *minimum,
                    "{workflow}:{}: {action}@{reference} targets Node.js 20, which GitHub \
                     now forces onto Node.js 24; pin @v{minimum} or newer",
                    number + 1
                );
            }
        }
        assert!(
            pins >= 10,
            "only {pins} action pins were found; the workflows are not being read"
        );
    }

    /// The menu bar is one left-to-right row of top-level menus.
    ///
    /// Menus are text buttons, and the same taller-font case that used to push
    /// the old toolbar off its centre line applies: every title must share one
    /// baseline, and the order must be stable regardless of label widths.
    #[test]
    fn menu_titles_share_one_row_in_reading_order() {
        // Pin the language: English and Chinese labels have different widths,
        // and a concurrent language change would move the very rects this
        // asserts on.
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.fonts.ui_size = 20.0;
        crate::fonts::apply_text_sizes(&ctx, &app.settings.fonts);

        full_frame(&ctx, &mut app, input());

        let rects = panels::probe::top_bar_rects();
        let names = ["file", "edit", "view", "tools", "help"];
        let find = |name: &str| {
            rects
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("{name} menu was not laid out"))
        };
        let reference = find("file").center().y;
        let mut previous_right = f32::NEG_INFINITY;
        for name in names {
            let rect = find(name);
            let offset = rect.center().y - reference;
            assert!(
                offset.abs() <= 0.75,
                "the {name} menu is {offset:.2}px off the menu bar's centre line"
            );
            assert!(
                rect.min.x >= previous_right,
                "the {name} menu overlaps or precedes the one before it: {rect:?}"
            );
            previous_right = rect.max.x;
        }
    }

    /// The send toolbar's actions drop to a left-aligned row when the options
    /// above them leave no room for the whole group.
    #[test]
    fn send_toolbar_wraps_its_actions_to_a_left_aligned_row() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.show_tool_panel = true;
        app.settings.send.append_newline = false;

        for _ in 0..2 {
            let mut narrow = input();
            narrow.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(880.0, 540.0),
            ));
            full_frame(&ctx, &mut app, narrow);
        }

        let rects = panels::probe::send_toolbar_rects();
        let find = |name: &str| {
            rects
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("{name} control was not laid out"))
        };
        let section = find("section");
        let history = find("history");
        let clear = find("clear");
        let file = find("file");
        let send = find("send");

        assert!(
            history.min.y >= section.max.y,
            "the actions stayed on the options row: {history:?} vs {section:?}"
        );
        assert!(
            (history.min.x - section.min.x).abs() <= 0.75,
            "the wrapped action row starts at {:.1}px, not the row's left edge {:.1}px",
            history.min.x,
            section.min.x
        );
        // The group still reads 历史 · 清除发送 · 发送文件 · 发送.
        for (left, right) in [("history", clear), ("clear", file), ("file", send)] {
            let left_rect = find(left);
            assert!(
                left_rect.max.x <= right.min.x,
                "{left} ({left_rect:?}) is not left of its neighbour ({right:?})"
            );
        }
        for (name, rect) in [("clear", clear), ("file", file), ("send", send)] {
            assert!(
                (rect.center().y - history.center().y).abs() <= 0.75,
                "{name} is on a different line from 历史: {:?} vs {history:?}",
                rect.center()
            );
        }
    }

    /// The action group never overlaps the options at any window width.
    ///
    /// The row used to decide from a fixed width guess; just above that guess
    /// the right-aligned group ran into the interval and timed-send controls
    /// before the layout moved it down.  Sweep the transition so the fix is
    /// checked where the old overlap happened.
    #[test]
    fn send_toolbar_actions_never_overlap_the_options() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.show_tool_panel = true;

        for width in [
            880.0_f32, 900.0, 940.0, 980.0, 1020.0, 1080.0, 1140.0, 1200.0, 1280.0, 1440.0,
        ] {
            for _ in 0..2 {
                let mut raw = input();
                raw.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 540.0),
                ));
                full_frame(&ctx, &mut app, raw);
            }

            let rects = panels::probe::send_toolbar_rects();
            let find = |name: &str| {
                rects
                    .iter()
                    .find(|(candidate, _)| *candidate == name)
                    .map(|(_, rect)| *rect)
                    .unwrap_or_else(|| panic!("{name} was not laid out at {width}px"))
            };
            let options = [
                ("section", find("section")),
                ("hex", find("hex")),
                ("newline", find("newline")),
                ("interval label", find("interval_label")),
                ("interval", find("interval")),
                ("timed", find("timed")),
            ];
            let actions = [
                ("history", find("history")),
                ("clear", find("clear")),
                ("file", find("file")),
                ("send", find("send")),
            ];
            for (option_name, option) in options {
                for (action_name, action) in actions {
                    let overlap = option.intersect(action);
                    assert!(
                        overlap.width() <= 0.5 || overlap.height() <= 0.5,
                        "at {width}px the {option_name} control {option:?} overlaps \
                         {action_name} {action:?}"
                    );
                }
            }
        }
    }

    /// The roomy case is unchanged: one row, group against the right edge.
    #[test]
    fn send_toolbar_actions_stay_right_aligned_when_they_fit() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.send.append_newline = false;

        full_frame(&ctx, &mut app, input());

        let rects = panels::probe::send_toolbar_rects();
        let find = |name: &str| {
            rects
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("{name} control was not laid out"))
        };
        let hex = find("hex");
        let history = find("history");
        let send = find("send");
        let panel = panels::probe::send_panel_rect();
        assert!(
            (send.center().y - hex.center().y).abs() <= 0.75,
            "the actions left the options row: {:?} vs {hex:?}",
            send.center()
        );
        assert!(
            history.min.x > hex.max.x,
            "the options and the actions overlap: {hex:?} vs {history:?}"
        );
        assert!(
            panel.max.x - send.max.x <= 8.0,
            "the action group no longer hugs the right edge: {send:?} in {panel:?}"
        );
    }

    #[test]
    fn send_history_combobox_matches_action_button_height() {
        let ctx = egui::Context::default();
        let mut app = seeded();

        full_frame(&ctx, &mut app, input());

        let rects = panels::probe::send_toolbar_rects();
        let find = |name: &str| {
            rects
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("{name} control was not laid out"))
        };
        let history = find("history");
        let send = find("send");
        let file = find("file");
        let clear = find("clear");
        for (name, rect) in [("file", file), ("clear", clear)] {
            assert!(
                (rect.height() - send.height()).abs() <= 0.25,
                "{name} height {:.2} differs from send {:.2}",
                rect.height(),
                send.height()
            );
        }
        assert!(
            (history.height() - send.height()).abs() <= 0.25,
            "history height {:.2} differs from send button {:.2}",
            history.height(),
            send.height()
        );
        assert!(
            (history.center().y - send.center().y).abs() <= 0.5,
            "history is vertically misaligned by {:.2}px",
            (history.center().y - send.center().y).abs()
        );
    }

    #[test]
    fn send_toolbar_does_not_overlap_at_minimum_window_width() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.show_tool_panel = true;
        app.settings.send.append_newline = false;

        for _ in 0..2 {
            let mut narrow = input();
            narrow.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(880.0, 540.0),
            ));
            full_frame(&ctx, &mut app, narrow);
        }

        let rects = panels::probe::send_toolbar_rects();
        let find = |name: &str| {
            rects
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, rect)| *rect)
                .unwrap_or_else(|| panic!("{name} control was not laid out"))
        };
        let left = [
            ("section", find("section")),
            ("hex", find("hex")),
            ("newline", find("newline")),
        ];
        let right = [
            ("history", find("history")),
            ("clear", find("clear")),
            ("file", find("file")),
            ("send", find("send")),
        ];
        for (left_name, left_rect) in left {
            for (right_name, right_rect) in right {
                let overlaps = left_rect.min.x < right_rect.max.x
                    && right_rect.min.x < left_rect.max.x
                    && left_rect.min.y < right_rect.max.y
                    && right_rect.min.y < left_rect.max.y;
                assert!(
                    !overlaps,
                    "{left_name} overlapped {right_name}: {left_rect:?} vs {right_rect:?}"
                );
            }
        }
    }

    #[test]
    fn mixed_script_text_keeps_one_baseline() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx, &crate::settings::FontSettings::default());
        let mut output = ctx.run_ui(input(), |_| ());
        output.textures_delta.clear();
        let text = "启用#内容备注HEX新行延迟";
        for font in [
            egui::FontId::proportional(14.0),
            egui::FontId::monospace(14.0),
        ] {
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE)
            });

            let baselines: Vec<(char, f32)> = galley
                .rows
                .iter()
                .flat_map(|row| row.glyphs.iter())
                .map(|glyph| (glyph.chr, glyph.pos.y))
                .collect();
            assert_eq!(baselines.len(), text.chars().count());
            let min = baselines
                .iter()
                .map(|(_, y)| *y)
                .fold(f32::INFINITY, f32::min);
            let max = baselines
                .iter()
                .map(|(_, y)| *y)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                max - min <= 0.01,
                "ASCII/CJK glyphs use different baselines in {font:?}: {min:.3}..={max:.3} ({baselines:?})"
            );
        }
    }

    #[test]
    fn multi_send_headers_are_vertically_aligned() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.show_tool_panel = true;
        app.side_tab = SideTab::Multi;

        full_frame(&ctx, &mut app, input());

        let headers = panels::probe::multi_header_rects();
        assert_eq!(headers.len(), 8, "every table column must have a header");
        let first_centre = headers[0].center().y;
        for (index, rect) in headers.iter().enumerate() {
            assert!(
                (rect.center().y - first_centre).abs() <= 0.25,
                "header {index} is vertically misaligned: {:?} vs {first_centre:.2}",
                rect.center()
            );
        }
    }

    #[test]
    fn dragging_the_settings_separator_widens_the_panel() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_tool_panel = false;

        let before_rect = full_frame(&ctx, &mut app, input());
        let before_content = panels::probe::left_content_width();
        assert!(
            before_content > 0.0,
            "the settings content was not laid out"
        );

        let grab = egui::pos2(before_rect.min.x, before_rect.center().y);
        drag(&ctx, &mut app, grab, egui::pos2(grab.x + 80.0, grab.y));

        let after_rect = full_frame(&ctx, &mut app, input());
        assert!(
            after_rect.min.x > before_rect.min.x + 40.0,
            "the panel did not widen: {} -> {}",
            before_rect.min.x,
            after_rect.min.x
        );
        assert!(
            panels::probe::left_content_width() > before_content,
            "the content did not follow the panel"
        );
    }

    #[test]
    fn dragging_the_send_separator_grows_the_editor() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());

        full_frame(&ctx, &mut app, input());
        let before = panels::probe::editor_rect();
        let panel = panels::probe::send_panel_rect();
        assert!(before.height() > 0.0, "the editor was not laid out");
        assert!(panel.height() > 0.0, "the send panel was not laid out");

        let grab = egui::pos2(panel.center().x, panel.top());
        drag(&ctx, &mut app, grab, egui::pos2(grab.x, grab.y - 90.0));

        let after = panels::probe::editor_rect();
        assert!(
            after.height() > before.height() + 40.0,
            "the editor did not follow the separator: {} -> {}",
            before.height(),
            after.height()
        );
    }

    #[test]
    fn dragging_the_send_area_to_the_bottom_edge_collapses_it() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());

        full_frame(&ctx, &mut app, input());
        let panel = panels::probe::send_panel_rect();
        let grab = egui::pos2(panel.center().x, panel.top());
        drag(&ctx, &mut app, grab, egui::pos2(grab.x, grab.y + 600.0));

        assert!(
            !app.settings.show_send_area,
            "dragging the send area off the bottom did not collapse it"
        );
        assert_eq!(
            app.settings.send_area_height,
            panels::SEND_AREA_DEFAULT,
            "collapsing must keep the height to restore"
        );

        // The edge strip is clickable, like the collapsed side panels.
        let rect = full_frame(&ctx, &mut app, input());
        click(
            &ctx,
            &mut app,
            egui::pos2(rect.center().x, rect.bottom() - 1.0),
        );
        assert!(
            app.settings.show_send_area,
            "clicking the send area's strip did not reopen it"
        );
        assert_eq!(app.settings.send_area_height, panels::SEND_AREA_DEFAULT);
    }

    #[test]
    fn a_collapsed_send_area_can_be_dragged_back_out() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_send_area = false;

        let rect = full_frame(&ctx, &mut app, input());
        let edge = egui::pos2(rect.center().x, rect.bottom());
        drag(&ctx, &mut app, edge, egui::pos2(edge.x, edge.y - 160.0));

        assert!(
            app.settings.show_send_area,
            "dragging up from the bottom edge did not reopen the send area"
        );
        assert!(
            app.settings.send_area_height > panels::SEND_AREA_MIN,
            "the drag did not set a new height: {}",
            app.settings.send_area_height
        );
    }

    #[test]
    fn shrinking_the_settings_panel_persists() {
        // Reported: dragging the left separator *left* clipped the panel's
        // content during the gesture and then snapped back on release, while
        // the receive area never grew.
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_tool_panel = false;

        let before = full_frame(&ctx, &mut app, input());
        let grab = egui::pos2(before.min.x, before.center().y);
        drag(&ctx, &mut app, grab, egui::pos2(grab.x - 40.0, grab.y));

        let after = full_frame(&ctx, &mut app, input());
        assert!(
            after.min.x < before.min.x - 20.0,
            "the panel snapped back instead of staying shrunk: {} -> {}",
            before.min.x,
            after.min.x
        );

        // And it must still be shrunk several frames later, i.e. the size was
        // committed rather than merely applied for one frame.
        for _ in 0..5 {
            full_frame(&ctx, &mut app, input());
        }
        assert!(
            full_frame(&ctx, &mut app, input()).min.x < before.min.x - 20.0,
            "the shrunk size was not persisted"
        );
    }

    #[test]
    fn tool_panel_is_never_narrower_than_its_table() {
        // The panel is sized for the widest supported language, so the same
        // stored width produces the same panel in Chinese and English; what
        // must hold in both is that the table it holds still fits.
        let _guard = crate::i18n::test_lock();
        for language in [crate::i18n::Lang::ZhCn, crate::i18n::Lang::EnUs] {
            crate::i18n::set_lang(language);
            let ctx = egui::Context::default();
            let mut app = XcomApp::headless(crate::settings::Settings::default());
            app.settings.show_tool_panel = true;
            app.side_tab = SideTab::Multi;
            full_frame(&ctx, &mut app, input());
            let (viewport, content, scrollbars) = panels::probe::multi_table();
            assert!(
                !scrollbars[0] && !scrollbars[1],
                "{language:?}: egui laid out a scrollbar (horizontal={}, vertical={})",
                scrollbars[0],
                scrollbars[1]
            );
            assert!(
                content.x <= viewport.width() + 0.5,
                "{language:?}: the table ({:.1}) is wider than its panel ({:.1})",
                content.x,
                viewport.width()
            );
        }
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
    }

    #[test]
    fn the_default_tool_panel_width_is_language_independent() {
        // Reported: with the shipped defaults the English panel opened wider
        // than the Chinese one.  The minimum is now measured against every
        // supported language, so switching language must not resize it.
        let _guard = crate::i18n::test_lock();
        let mut widths = Vec::new();
        for language in [crate::i18n::Lang::ZhCn, crate::i18n::Lang::EnUs] {
            crate::i18n::set_lang(language);
            let ctx = egui::Context::default();
            let mut app = XcomApp::headless(crate::settings::Settings::default());
            app.settings.show_tool_panel = true;
            app.side_tab = SideTab::Multi;
            full_frame(&ctx, &mut app, input());
            full_frame(&ctx, &mut app, input());
            let panel = panels::probe::tool_panel_rect();
            assert!(panel.width() > 0.0, "{language:?}: panel not laid out");
            widths.push(panel.width());
        }
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        assert!(
            (widths[0] - widths[1]).abs() < 0.5,
            "the default tool panel width depends on the language: {widths:?}"
        );
    }

    #[test]
    fn shrinking_the_tool_panel_persists() {
        // The panel's minimum follows the widest supported language, so the
        // default panel already sits on that floor.  Widen it first to leave
        // room to shrink, then check the smaller width sticks instead of
        // snapping back on release.
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_tool_panel = true;
        app.side_tab = SideTab::Multi;

        let before = full_frame(&ctx, &mut app, input());
        let grab = egui::pos2(before.max.x, before.center().y);
        drag(&ctx, &mut app, grab, egui::pos2(grab.x - 120.0, grab.y));
        let widened = full_frame(&ctx, &mut app, input());
        assert!(
            widened.max.x < before.max.x - 80.0,
            "the panel did not widen: {} -> {}",
            before.max.x,
            widened.max.x
        );

        // Shrink by 15px from the widened state; still above the floor.
        let grab = egui::pos2(widened.max.x, widened.center().y);
        drag(&ctx, &mut app, grab, egui::pos2(grab.x + 15.0, grab.y));

        let mut after = full_frame(&ctx, &mut app, input());
        for _ in 0..5 {
            after = full_frame(&ctx, &mut app, input());
        }
        assert!(
            after.max.x > widened.max.x + 10.0,
            "the tool panel snapped back: {} -> {}",
            widened.max.x,
            after.max.x
        );
    }

    #[test]
    fn the_tool_panel_splitter_moves_both_ways() {
        // The right-hand panel is anchored to the window's right edge, so its
        // size is `fixed_edge - pointer.x`.  Getting that sign wrong makes the
        // panel jump to its minimum and then refuse to widen.
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_tool_panel = true;
        app.side_tab = SideTab::Multi;

        let start = panels::RIGHT_PANEL_DEFAULT;

        // Drag left: the panel gets wider.
        let before = full_frame(&ctx, &mut app, input());
        drag(
            &ctx,
            &mut app,
            egui::pos2(before.max.x, before.center().y),
            egui::pos2(before.max.x - 70.0, before.center().y),
        );
        let wider = app.settings.right_panel_width;
        assert!(
            wider > start + 40.0,
            "dragging left did not widen the tool panel: {start} -> {wider}"
        );

        // Drag right: it shrinks again, and the smaller size sticks.
        let before = full_frame(&ctx, &mut app, input());
        drag(
            &ctx,
            &mut app,
            egui::pos2(before.max.x, before.center().y),
            egui::pos2(before.max.x + 80.0, before.center().y),
        );
        let narrower = app.settings.right_panel_width;
        assert!(
            narrower < wider - 40.0,
            "dragging right did not shrink the tool panel: {wider} -> {narrower}"
        );
        for _ in 0..4 {
            full_frame(&ctx, &mut app, input());
        }
        assert_eq!(
            app.settings.right_panel_width, narrower,
            "the new width did not persist"
        );
    }

    #[test]
    fn the_tool_panel_stops_at_the_centre_columns_floor() {
        // The panel maxima are *soft*: the receive view keeps CENTER_MIN, so a
        // drag far past the maximum stops early instead of squeezing it away.
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        let rect = full_frame(&ctx, &mut app, input());

        // Drag left, much further than the panel can possibly grow.
        drag(
            &ctx,
            &mut app,
            egui::pos2(rect.max.x, rect.center().y),
            egui::pos2(0.0, rect.center().y),
        );
        assert!(
            app.settings.right_panel_width <= panels::RIGHT_PANEL_MAX,
            "the hard maximum still applies, got {}",
            app.settings.right_panel_width
        );

        let centre = full_frame(&ctx, &mut app, input());
        assert!(
            centre.width() >= panels::CENTER_MIN - 8.0,
            "the centre column was squeezed to {:.1}px, below its {}px floor",
            centre.width(),
            panels::CENTER_MIN
        );
    }

    /// Drag a panel's splitter far past the window edge and check it folded
    /// away -- and that the width to come back to was kept.
    #[test]
    fn dragging_a_panel_to_the_window_edge_collapses_it() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());

        let before = full_frame(&ctx, &mut app, input());
        let grab = egui::pos2(before.min.x, before.center().y);
        drag(&ctx, &mut app, grab, egui::pos2(grab.x - 600.0, grab.y));
        assert!(
            !app.settings.show_config_panel,
            "dragging the configuration panel off the edge did not collapse it"
        );
        assert_eq!(
            app.settings.left_panel_width,
            panels::LEFT_PANEL_DEFAULT,
            "collapsing must keep the width to restore"
        );
        let after = full_frame(&ctx, &mut app, input());
        assert!(
            after.min.x <= 1.0,
            "the receive view did not take the freed space: starts at {:.1}",
            after.min.x
        );

        // Same gesture, mirrored, for the tool panel.
        let before = full_frame(&ctx, &mut app, input());
        let grab = egui::pos2(before.max.x, before.center().y);
        drag(&ctx, &mut app, grab, egui::pos2(grab.x + 600.0, grab.y));
        assert!(
            !app.settings.show_tool_panel,
            "dragging the tool panel off the edge did not collapse it"
        );
        assert_eq!(
            app.settings.right_panel_width,
            panels::RIGHT_PANEL_DEFAULT,
            "collapsing must keep the width to restore"
        );
        assert_eq!(
            full_frame(&ctx, &mut app, input()).max.x,
            1280.0,
            "the receive view did not reach the window's right edge"
        );
    }

    #[test]
    fn clicking_a_collapsed_edge_strip_reopens_the_panel() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_config_panel = false;
        app.settings.show_tool_panel = false;

        let rect = full_frame(&ctx, &mut app, input());

        // The strip is wide enough to hit but does not eat the receive view.
        click(&ctx, &mut app, egui::pos2(6.0, rect.center().y));
        assert!(
            app.settings.show_config_panel,
            "clicking the configuration panel's strip did not reopen it"
        );
        assert!(
            !app.settings.show_tool_panel,
            "the other panel must be left alone"
        );

        let rect = full_frame(&ctx, &mut app, input());
        click(&ctx, &mut app, egui::pos2(1274.0, rect.center().y));
        assert!(
            app.settings.show_tool_panel,
            "clicking the tool panel's strip did not reopen it"
        );
    }

    #[test]
    fn a_collapsed_panel_can_be_dragged_back_out() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_config_panel = false;

        let rect = full_frame(&ctx, &mut app, input());
        drag(
            &ctx,
            &mut app,
            egui::pos2(0.0, rect.center().y),
            egui::pos2(400.0, rect.center().y),
        );
        assert!(
            app.settings.show_config_panel,
            "dragging away from the edge did not reopen the panel"
        );
        assert!(
            app.settings.left_panel_width > panels::LEFT_PANEL_MIN,
            "the drag did not set a new width: {}",
            app.settings.left_panel_width
        );
    }

    /// A pseudo-terminal pair, so the timer can be tested against a real port.
    #[cfg(unix)]
    struct TestPty {
        master: std::fs::File,
        slave_path: String,
    }

    #[cfg(unix)]
    impl TestPty {
        fn open() -> Option<Self> {
            use std::os::fd::FromRawFd;
            let mut master: libc::c_int = -1;
            let mut slave: libc::c_int = -1;
            // SAFETY: openpty fills both descriptors on success; checked below.
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
            // SAFETY: both descriptors are valid and now owned here.
            let master_file = unsafe { std::fs::File::from_raw_fd(master) };
            let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };
            // SAFETY: `master` is still open.
            let name = unsafe { libc::ptsname(master) };
            if name.is_null() {
                return None;
            }
            // SAFETY: NUL-terminated string valid until the next ptsname call.
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

    #[test]
    #[cfg(unix)]
    fn a_sent_payload_is_echoed_into_the_receive_view() {
        let Some(pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.serial.port = pty.slave_path.clone();
        if let Err(e) = app.link.open(&app.settings.serial) {
            eprintln!("skipping: cannot open serial port: {e}");
            return;
        }
        app.settings.send.buffer = "AT".to_owned();
        app.settings.send.append_newline = false;

        app.settings.echo_sent = true;
        app.send_once();
        assert_eq!(app.log.row_count(), 1, "the send is echoed");
        assert_eq!(app.log.row(0), format!("{}AT", crate::logstore::MARK_SENT));
        assert_eq!(app.log.row_kind(0), crate::logstore::RowKind::Sent);
        // Echoing must not pollute the received stream.
        assert!(app.log.raw().is_empty());

        app.log.clear();
        app.settings.echo_sent = false;
        app.send_once();
        assert_eq!(app.log.row_count(), 0, "echo off adds nothing");
    }

    #[test]
    #[cfg(unix)]
    fn timed_send_repeats_on_its_period() {
        let Some(mut pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        crate::fonts::install_default_families(&ctx);
        app.settings.serial.port = pty.slave_path.clone();
        if let Err(e) = app.link.open(&app.settings.serial) {
            eprintln!("skipping: cannot open serial port: {e}");
            return;
        }

        app.settings.send.buffer = "AT".to_owned();
        app.settings.send.append_newline = false;
        app.settings.send.timed = true;
        app.settings.send.period_ms = 30;

        let deadline = Instant::now() + Duration::from_millis(250);
        while Instant::now() < deadline {
            let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
            output.textures_delta.clear();
            std::thread::sleep(Duration::from_millis(5));
        }

        let got = pty.read_available(Duration::from_millis(100));
        println!(
            "link_open={} messages={:?}",
            app.link.is_open(),
            app.messages.iter().map(|m| &m.text).collect::<Vec<_>>()
        );
        let text = String::from_utf8_lossy(&got);
        assert!(
            text.matches("AT").count() >= 2,
            "expected the payload repeatedly, got {text:?}"
        );
    }

    #[test]
    fn closing_the_port_untickets_timed_send() {
        // 定时发送 is a run state, so it cannot outlive the port; the interval
        // is left as configured for the next run.
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.send.timed = true;
        app.close_port();
        assert!(!app.settings.send.timed);
    }

    #[test]
    fn settings_that_need_a_control_have_one_in_the_ui() {
        // The echo feature shipped with the setting wired through the store and
        // the send path but *no* control anywhere, so the user could neither
        // see nor change it.  Every behavioural test passed because they set
        // the field directly.  This checks the other half: that the panels
        // actually offer a control for each setting a user is meant to reach.
        let panels =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui/panels.rs"))
                .expect("panels.rs is readable");
        for field in [
            "echo_sent",
            "auto_scroll",
            "line_spacing",
            "fonts.ui_family",
            "fonts.receive_family",
            "fonts.send_family",
            "fonts.ui_size",
            "fonts.receive_size",
            "fonts.send_size",
            "loop_enabled",
            "loop_count",
        ] {
            // The write, not merely a mention: reading a field proves nothing
            // about whether a control can change it.
            assert!(
                panels.contains(&format!("{field} = ")),
                "settings.{field} has no control that sets it"
            );
        }
    }

    #[test]
    fn renders_the_default_frame() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        render(&mut app, &ctx, "default");
    }

    #[test]
    fn renders_with_an_empty_log() {
        // The zero-row path allocates a placeholder rect and must not divide by
        // zero or index into missing rows.
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        render(&mut app, &ctx, "empty log");
    }

    #[test]
    fn renders_every_theme() {
        let ctx = egui::Context::default();
        for theme in ThemeMode::ALL {
            let mut app = seeded();
            app.settings.theme = theme;
            render(&mut app, &ctx, theme.label());
        }
    }

    #[test]
    fn renders_every_display_mode() {
        let ctx = egui::Context::default();
        let cases = [
            ("hex stream", ViewMode::Hex, HexStyle::Stream, false, false),
            ("hex dump", ViewMode::Hex, HexStyle::Dump, false, false),
            (
                "text timestamp",
                ViewMode::Text,
                HexStyle::Stream,
                true,
                false,
            ),
            ("text wrap", ViewMode::Text, HexStyle::Stream, false, true),
            ("hex timestamp", ViewMode::Hex, HexStyle::Stream, true, true),
        ];
        for (name, mode, hex_style, timestamp, wrap) in cases {
            let mut app = seeded();
            app.settings.display = crate::logstore::DisplayOptions {
                mode,
                hex_style,
                timestamp,
                wrap,
                ..Default::default()
            };
            app.log.set_options(app.settings.display);
            render(&mut app, &ctx, name);
        }
    }

    /// Every text shape a frame painted, as `(text, section colours)`.
    fn painted_text(output: &egui::FullOutput) -> Vec<(String, Vec<egui::Color32>)> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.job.text.clone(),
                    text.galley
                        .job
                        .sections
                        .iter()
                        .map(|s| s.format.color)
                        .collect(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_receive_view_paints_coloured_direction_markers() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.echo_sent = true;
        app.log.append_sent(b"AT+RST", 0);
        let palette = crate::theme::log_palette(&ctx, app.settings.theme, app.settings.log_colors);
        crate::fonts::install_default_families(&ctx);

        // Two passes: the first lays the panels out, the second paints with
        // settled geometry, and it is that output we inspect.
        let mut painted = Vec::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
            output.textures_delta.clear();
            painted = painted_text(&output);
        }

        let colours = |row: &str| {
            painted
                .iter()
                .find(|(text, _)| text == row)
                .map(|(_, colours)| colours.clone())
                .unwrap_or_else(|| panic!("row {row:?} was not painted"))
        };
        // The marker and the data it introduces are separate sections, so the
        // arrow keeps its own colour; the plain row is a single section.
        assert_eq!(
            colours(&format!("{}AT+GMR", crate::logstore::MARK_RECEIVED)),
            vec![palette.received_marker, palette.foreground]
        );
        assert_eq!(
            colours(&format!("{}AT+RST", crate::logstore::MARK_SENT)),
            vec![palette.sent_marker, palette.foreground]
        );

        // Switching the markers off must reach the painted output, not just the
        // store: this is the option the user toggles in Display settings.
        app.settings.display.markers = false;
        app.log.set_options(app.settings.display);
        for _ in 0..2 {
            let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
            output.textures_delta.clear();
            painted = painted_text(&output);
        }
        assert!(
            !painted
                .iter()
                .any(|(text, _)| text.starts_with(crate::logstore::MARK_RECEIVED)
                    || text.starts_with(crate::logstore::MARK_SENT)),
            "a marker was still painted: {:?}",
            painted.iter().map(|(t, _)| t).collect::<Vec<_>>()
        );
        assert!(painted.iter().any(|(text, _)| text == "AT+GMR"));
    }

    #[test]
    fn custom_colours_drive_both_received_and_echoed_text() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.echo_sent = true;
        app.settings.log_colors = crate::theme::LogColorOverride {
            enabled: true,
            foreground: [0x12, 0x34, 0x56],
            background: [0x00, 0x00, 0x00],
        };
        app.log.append_sent(b"AT+RST", 0);
        crate::fonts::install_default_families(&ctx);
        let custom = egui::Color32::from_rgb(0x12, 0x34, 0x56);

        let mut painted = Vec::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(input(), |ui| app.draw(ui));
            output.textures_delta.clear();
            painted = painted_text(&output);
        }

        let colours = |row: &str| {
            painted
                .iter()
                .find(|(text, _)| text == row)
                .map(|(_, colours)| colours.clone())
                .unwrap_or_else(|| panic!("row {row:?} was not painted"))
        };
        // Both directions are text in the receive area, so the one custom
        // foreground colour must reach both data sections.
        assert_eq!(
            *colours(&format!("{}AT+GMR", crate::logstore::MARK_RECEIVED))
                .last()
                .unwrap(),
            custom
        );
        assert_eq!(
            *colours(&format!("{}AT+RST", crate::logstore::MARK_SENT))
                .last()
                .unwrap(),
            custom
        );
    }

    #[test]
    fn renders_with_a_live_selection_and_a_copy_action() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        let store = &app.log;
        app.log_view.select_all(store);
        assert!(app.log_view.has_selection());
        render(&mut app, &ctx, "selection");
    }

    #[test]
    fn renders_while_paused_with_buffered_bytes() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_paused = true;
        app.pending_rx.extend_from_slice(b"queued");
        assert_eq!(app.pending_rx_len(), b"queued".len());
        render(&mut app, &ctx, "paused");

        // Resuming must fold the buffer into the log.
        let before = app.log.row_count();
        app.resume_display();
        assert!(!app.show_paused);
        assert_eq!(app.pending_rx_len(), 0);
        assert!(app.log.row_count() >= before);
        render(&mut app, &ctx, "resumed");
    }

    /// A right-click anywhere in the receive area opens its action menu.
    ///
    /// The actions used to live in a toolbar; this guards the replacement, and
    /// in particular the backdrop that makes a short log's blank area a valid
    /// target.
    #[test]
    fn right_clicking_the_receive_area_opens_the_context_menu() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        full_frame(&ctx, &mut app, input());
        let panel = panels::probe::send_panel_rect();
        assert!(panel.height() > 0.0, "the send panel was not laid out");
        // Below the three seeded rows, inside the receive viewport.
        let pos = egui::pos2(panel.center().x, panel.top() - 60.0);

        let mut raw = input();
        raw.events.push(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            raw.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: Default::default(),
            });
        }
        full_frame(&ctx, &mut app, raw);

        assert!(
            egui::Popup::is_any_open(&ctx),
            "right-clicking the receive area did not open a context menu"
        );
    }

    /// The display settings page also hosts the keyword rules.
    #[test]
    fn renders_the_display_settings_window() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_display_settings = true;
        app.settings.log_colors.enabled = true;
        app.settings.highlights.enabled = true;
        app.settings.highlights.rules = vec![
            crate::highlight::HighlightRule {
                text: "OK".to_owned(),
                regex: false,
                color: [0x4C, 0xAF, 0x50],
            },
            crate::highlight::HighlightRule {
                text: "ERROR".to_owned(),
                regex: false,
                color: [0xE5, 0x53, 0x53],
            },
        ];
        render(&mut app, &ctx, "display settings");
    }

    /// The reset confirmation dialog renders without applying anything: the
    /// reset must wait for the explicit confirmation button.
    #[test]
    fn renders_the_reset_confirmation_dialog_without_resetting() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.echo_sent = true;
        app.show_reset_confirm = true;
        render(&mut app, &ctx, "reset confirmation");
        assert!(app.show_reset_confirm, "the dialog closed on its own");
        assert!(
            app.settings.echo_sent,
            "rendering the dialog must not apply the reset"
        );
    }

    /// The keyboard-shortcuts dialog renders with the full shortcut map.
    #[test]
    fn renders_the_shortcuts_dialog() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_shortcuts = true;
        render(&mut app, &ctx, "shortcuts");
    }

    /// The keyword column is a table cell with room for a real pattern.
    #[test]
    fn highlight_table_gives_the_keyword_field_room() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_display_settings = true;
        app.settings.highlights.enabled = true;
        app.settings.highlights.rules = vec![
            crate::highlight::HighlightRule {
                text: "ERROR".to_owned(),
                regex: false,
                color: [0xE5, 0x53, 0x53],
            },
            crate::highlight::HighlightRule {
                text: "OK".to_owned(),
                regex: false,
                color: [0x4C, 0xAF, 0x50],
            },
        ];
        render(&mut app, &ctx, "display settings");

        let table = panels::probe::highlight_table_rect();
        let fields = panels::probe::highlight_keyword_rects();
        assert_eq!(fields.len(), 2, "the keyword rows were not laid out");
        assert!(
            table.width() <= 450.5,
            "the keyword table overflows the 450px page: {:.1}px",
            table.width()
        );
        assert!(
            table.width() >= 400.0,
            "the keyword table is only {:.1}px wide",
            table.width()
        );
        for rect in fields {
            assert!(
                rect.width() >= 290.0,
                "a keyword field is only {:.1}px wide",
                rect.width()
            );
            assert!(
                table.contains_rect(rect),
                "a keyword field escaped the table: {rect:?} vs {table:?}"
            );
        }
    }

    /// The protocol form stays inside its panel in every language.
    ///
    /// At the minimum window width the English labels leave too little room for
    /// a fixed-width combo box to be laid out on its row.  egui cannot wrap a
    /// combo after starting it, so it used to paint the control past the panel
    /// edge: the separator looked broken and the left-hand controls were
    /// clipped.  Both languages must keep every row inside the panel.
    #[test]
    fn protocol_form_fits_its_panel_in_both_languages() {
        let _guard = crate::i18n::test_lock();
        for language in crate::i18n::Lang::ALL {
            crate::i18n::set_lang(language);
            let ctx = egui::Context::default();
            let mut app = seeded();
            app.settings.show_tool_panel = true;
            app.settings.right_panel_width = panels::RIGHT_PANEL_MIN;
            app.side_tab = SideTab::Protocol;
            for _ in 0..2 {
                let mut narrow = input();
                narrow.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(880.0, 540.0),
                ));
                full_frame(&ctx, &mut app, narrow);
            }

            let panel = panels::probe::tool_panel_rect();
            let content = panels::probe::protocol_content_rect();
            let rows = panels::probe::protocol_rows();
            assert!(!rows.is_empty(), "the protocol form was not laid out");
            for (name, rect) in rows {
                assert!(
                    rect.right() <= panel.right() + 0.5,
                    "{language:?}: the protocol {name} row ends at {:.1}, \
                     past the panel edge {:.1}",
                    rect.right(),
                    panel.right()
                );
            }
            assert!(
                content.right() <= panel.right() + 0.5,
                "{language:?}: protocol content ends at {:.1}, past the panel edge {:.1}",
                content.right(),
                panel.right()
            );
            assert!(
                content.bottom() <= panel.bottom() + 0.5,
                "{language:?}: protocol content ends at {:.1}, past the panel bottom {:.1}",
                content.bottom(),
                panel.bottom()
            );
        }
    }

    /// The send toolbar and editor stay inside the area at its floor.
    ///
    /// Dragging the separator down used to clip the toolbar: the old minimum
    /// was a constant, while the toolbar is one or two rows depending on the
    /// width and every row grows with the interface font.  The floor is now
    /// computed from those parts, so the whole toolbar and a usable editor must
    /// remain visible in both languages and at the largest font.
    #[test]
    fn send_toolbar_and_editor_fit_at_the_area_floor() {
        let _guard = crate::i18n::test_lock();
        for language in crate::i18n::Lang::ALL {
            crate::i18n::set_lang(language);
            for width in [880.0_f32, 1600.0] {
                for ui_size in [13.0_f32, 24.0] {
                    let ctx = egui::Context::default();
                    let mut app = seeded();
                    app.settings.fonts.ui_size = ui_size;
                    // The stored value the separator would leave behind when
                    // dragged all the way down.
                    app.settings.send_area_height = panels::SEND_AREA_MIN;
                    // A file send adds a progress bar above the editor, which
                    // is one more piece of content the floor must hold.
                    app.file_send = Some(FileSend {
                        path: "firmware.bin".to_owned(),
                        data: vec![0; 32],
                        offset: 0,
                        chunk: 8,
                        last: std::time::Instant::now(),
                    });
                    for _ in 0..2 {
                        let mut raw = input();
                        raw.screen_rect = Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 540.0),
                        ));
                        full_frame(&ctx, &mut app, raw);
                    }

                    let panel = panels::probe::send_panel_rect();
                    assert!(panel.height() > 0.0, "the send panel was not laid out");
                    for (name, rect) in panels::probe::send_toolbar_rects() {
                        assert!(
                            rect.min.y >= panel.min.y - 0.5,
                            "{language:?} {width}x540 font {ui_size}: the {name} control \
                             starts above the send panel: {rect:?} vs {panel:?}"
                        );
                        assert!(
                            rect.max.y <= panel.max.y + 0.5,
                            "{language:?} {width}x540 font {ui_size}: the {name} control \
                             is clipped at the bottom: {rect:?} vs {panel:?}"
                        );
                    }

                    let editor = panels::probe::editor_rect();
                    assert!(
                        editor.height() >= panels::SEND_EDITOR_MIN - 0.5,
                        "{language:?} {width}x540 font {ui_size}: the editor is only \
                         {:.1}px tall",
                        editor.height()
                    );
                    assert!(
                        editor.max.y <= panel.max.y + 0.5,
                        "{language:?} {width}x540 font {ui_size}: the editor overflows the \
                         send panel: {editor:?} vs {panel:?}"
                    );
                }
            }
        }
    }

    /// Dragging the separator down stops at the measured toolbar floor.
    ///
    /// The floor is dynamic, so this drives the real drag path rather than
    /// only setting `send_area_height` below it: the stored height must be
    /// lifted above the constant minimum and the toolbar must stay whole.
    #[test]
    fn dragging_the_send_separator_stops_at_the_toolbar_floor() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.fonts.ui_size = 24.0;

        full_frame(&ctx, &mut app, input());
        let panel = panels::probe::send_panel_rect();
        let grab = egui::pos2(panel.center().x, panel.top());
        drag(&ctx, &mut app, grab, egui::pos2(grab.x, grab.y + 90.0));

        let panel = full_frame(&ctx, &mut app, input());
        assert!(
            app.settings.show_send_area,
            "the send area collapsed instead of stopping at its floor"
        );
        assert!(
            app.settings.send_area_height > panels::SEND_AREA_MIN,
            "the floor is still the constant minimum: {}",
            app.settings.send_area_height
        );

        let toolbar = panels::probe::send_toolbar_rects();
        assert!(!toolbar.is_empty(), "the toolbar was not laid out");
        for (name, rect) in toolbar {
            assert!(
                rect.max.y <= panel.max.y + 0.5,
                "the {name} control is clipped after the drag: {rect:?} vs {panel:?}"
            );
        }
    }

    #[test]
    fn renders_both_tool_panel_tabs() {
        let ctx = egui::Context::default();
        for tab in [SideTab::Multi, SideTab::Protocol] {
            let mut app = seeded();
            app.side_tab = tab;
            render(&mut app, &ctx, "tool panel");
        }
    }

    #[test]
    fn renders_with_the_side_panel_hidden() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.show_tool_panel = false;
        render(&mut app, &ctx, "side panel hidden");
    }

    #[test]
    fn renders_the_about_window() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_about = true;
        render(&mut app, &ctx, "about");
    }

    #[test]
    fn renders_the_multi_send_pages() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        for index in 0..crate::multi::TOTAL_ENTRIES {
            if let Some(entry) = app.settings.multi.entry_mut(index) {
                entry.text = format!("CMD{index}");
                entry.note = "备注".to_owned();
                entry.enabled = index % 2 == 0;
                entry.hex = index % 3 == 0;
            }
        }
        for page in 0..app.settings.multi.page_count() {
            app.settings.multi.page = page;
            render(&mut app, &ctx, "multi page");
        }
    }

    #[test]
    fn renders_an_in_progress_file_send() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.file_send = Some(FileSend {
            path: "/tmp/firmware.bin".to_owned(),
            data: vec![0u8; 4096],
            offset: 1024,
            chunk: 512,
            last: Instant::now(),
        });
        let send = app.file_send.as_ref().expect("installed");
        assert_eq!(send.remaining(), 3072);
        assert!((send.progress() - 0.25).abs() < f32::EPSILON);
        render(&mut app, &ctx, "file send");
    }

    #[test]
    fn renders_a_running_protocol_transfer() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.side_tab = SideTab::Protocol;
        app.settings.protocol_data = "00 00 00 0A".to_owned();
        app.protocol_log.clear();
        app.log_protocol(Level::Info, "→ 帧 #1".to_owned());
        app.log_protocol(Level::Success, "✓ 收到应答 #1".to_owned());
        app.protocol_progress = Some((64, 256));
        app.protocol.start_auto(vec![0, 0, 0, 0x0A], Instant::now());
        assert!(app.protocol.is_running());
        render(&mut app, &ctx, "protocol running");
        app.protocol.stop("测试结束");
        render(&mut app, &ctx, "protocol stopped");
    }

    #[test]
    fn renders_the_interface_in_english() {
        // The guard test proves every key has a translation; this proves the
        // whole frame actually lays out in English without panicking.
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::EnUs);
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.show_about = true;
        for tab in [SideTab::Multi, SideTab::Protocol] {
            app.side_tab = tab;
            render(&mut app, &ctx, "english");
        }
        assert_eq!(crate::i18n::tr("打开串口"), "Open port");
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        render(&mut app, &ctx, "chinese");
    }

    #[test]
    fn renders_extreme_font_and_spacing_settings() {
        let ctx = egui::Context::default();
        for (size, spacing) in [
            (8.0, crate::settings::LINE_SPACING_MIN),
            (28.0, crate::settings::LINE_SPACING_MAX),
        ] {
            let mut app = seeded();
            app.settings.fonts.ui_size = 24.0;
            app.settings.fonts.receive_size = size;
            app.settings.fonts.send_size = size;
            app.settings.line_spacing = spacing;
            app.settings.display.wrap = true;
            render(&mut app, &ctx, "extreme settings");
        }
    }

    #[test]
    fn layout_never_panics_with_a_narrow_window() {
        // Guards the wrapping maths, which divides by the glyph width.
        let ctx = egui::Context::default();
        let mut app = seeded();
        crate::fonts::install_default_families(&ctx);
        app.settings.display.wrap = true;
        for width in [240.0f32, 480.0, 1024.0] {
            let narrow = || egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 600.0),
                )),
                ..Default::default()
            };
            for _ in 0..2 {
                let mut output = ctx.run_ui(narrow(), |ui| app.draw(ui));
                let shapes = output.shapes.len();
                output.textures_delta.clear();
                assert!(shapes > 0, "width {width}");
            }
        }
    }

    /// Put `entries` (text, delay_ms) on page 0, all checked.
    fn arm(app: &mut XcomApp, entries: &[(&str, u64)]) {
        for (index, (text, delay)) in entries.iter().enumerate() {
            let entry = app.settings.multi.entry_mut(index).expect("entry");
            entry.text = (*text).to_owned();
            entry.enabled = true;
            entry.newline = false;
            entry.delay_ms = *delay;
        }
    }

    #[test]
    fn sending_checked_entries_with_nothing_checked_warns_and_does_not_start() {
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.messages.clear();
        app.start_multi_run(false, Instant::now());
        assert!(!app.multi_run_active(), "nothing to send");
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].level, Level::Warning);
    }

    #[test]
    fn a_sequence_stops_when_the_port_is_not_open() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        arm(&mut app, &[("A", 0), ("B", 0)]);
        app.start_multi_run(false, Instant::now());
        assert!(app.multi_run_active());
        app.pump_multi(&ctx, Instant::now());
        assert!(!app.multi_run_active(), "closed port stops the sequence");
    }

    /// A closed port is also what the user sees after choosing 关闭串口: the
    /// run stops, and with it the loop switch and the panel's marker.  All
    /// three are one state, so none of them can be left claiming otherwise.
    #[test]
    fn a_closed_port_clears_the_loop_switch_and_the_marker() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        arm(&mut app, &[("A", 0), ("B", 0)]);
        app.settings.multi.loop_enabled = true;
        app.start_multi_run(true, Instant::now());
        assert!(app.multi_run_active());

        app.pump_multi(&ctx, Instant::now());

        assert!(!app.multi_run_active(), "the run stopped");
        assert!(
            !app.settings.multi.loop_enabled,
            "the loop switch must not stay ticked"
        );
        assert!(
            app.multi_run_sent().is_none(),
            "no entry may stay marked as being sent"
        );
        assert_eq!(
            app.latest_message().map(|m| m.level),
            Some(Level::Warning),
            "and the stop is reported"
        );
    }

    #[test]
    #[cfg(unix)]
    fn the_multi_send_panel_marks_the_entry_the_run_is_on() {
        let Some(pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_tool_panel = true;
        app.settings.serial.port = pty.slave_path.clone();
        if let Err(e) = app.link.open(&app.settings.serial) {
            eprintln!("skipping: cannot open serial port: {e}");
            return;
        }
        arm(&mut app, &[("A", 0), ("B", 0), ("C", 0)]);
        full_frame(&ctx, &mut app, input());
        assert_eq!(
            panels::probe::multi_active_row_rect(),
            egui::Rect::NOTHING,
            "nothing is marked before a run starts"
        );

        let start = Instant::now();
        app.start_multi_run(false, start);
        app.pump_multi(&ctx, start);
        full_frame(&ctx, &mut app, input());
        let first_row = panels::probe::multi_active_row_rect();
        let first_text = panels::probe::multi_status_text();
        assert_ne!(first_row, egui::Rect::NOTHING, "the row was not marked");
        assert!(
            first_text.contains("#0") && first_text.contains("1 / 3"),
            "the status line must name the entry and its place in the round, got {first_text:?}"
        );

        app.pump_multi(&ctx, start);
        full_frame(&ctx, &mut app, input());
        let second_row = panels::probe::multi_active_row_rect();
        let second_text = panels::probe::multi_status_text();
        assert!(
            second_text.contains("#1") && second_text.contains("2 / 3"),
            "the status line follows the run, got {second_text:?}"
        );
        assert!(
            second_row.min.y > first_row.min.y,
            "the marker must follow the row: {first_row:?} then {second_row:?}"
        );

        // The last entry ends the run, and the marker goes with it.
        app.pump_multi(&ctx, start);
        assert!(!app.multi_run_active());
        full_frame(&ctx, &mut app, input());
        assert_eq!(
            panels::probe::multi_active_row_rect(),
            egui::Rect::NOTHING,
            "the marker must not outlive the run"
        );
        assert!(
            panels::probe::multi_status_text().is_empty(),
            "nor must the status line"
        );
    }

    /// A run spans the whole pool, but the marker only appears on the page
    /// that currently shows the entry in flight.
    #[test]
    #[cfg(unix)]
    fn the_marker_follows_the_entry_onto_whatever_page_shows_it() {
        let Some(pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.show_tool_panel = true;
        app.settings.serial.port = pty.slave_path.clone();
        if let Err(e) = app.link.open(&app.settings.serial) {
            eprintln!("skipping: cannot open serial port: {e}");
            return;
        }
        arm(&mut app, &[("A", 1000), ("B", 1000)]);
        app.settings.multi.page = 0;

        let start = Instant::now();
        app.start_multi_run(false, start);
        app.pump_multi(&ctx, start);
        full_frame(&ctx, &mut app, input());
        assert_ne!(panels::probe::multi_active_row_rect(), egui::Rect::NOTHING);

        // Paging away does not stop the run, and the mark stays with the entry
        // rather than the page.
        app.settings.multi.page = 1;
        full_frame(&ctx, &mut app, input());
        assert_eq!(
            panels::probe::multi_active_row_rect(),
            egui::Rect::NOTHING,
            "another page's row must not be marked"
        );
        assert!(
            panels::probe::multi_status_text().contains('1'),
            "the status line is about the run, not the page"
        );
        assert!(app.multi_run_active(), "paging must not stop the run");
    }

    #[test]
    #[cfg(unix)]
    fn the_sequence_waits_each_entry_delay_before_the_next() {
        let Some(mut pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.serial.port = pty.slave_path.clone();
        if let Err(e) = app.link.open(&app.settings.serial) {
            eprintln!("skipping: cannot open serial port: {e}");
            return;
        }
        arm(&mut app, &[("A", 60), ("B", 0)]);

        let start = Instant::now();
        app.start_multi_run(false, start);

        app.pump_multi(&ctx, start);
        // Well inside A's delay, so B must not have gone out yet.
        app.pump_multi(&ctx, start + Duration::from_millis(30));
        let early = pty.read_available(Duration::from_millis(80));
        assert_eq!(String::from_utf8_lossy(&early), "A");

        app.pump_multi(&ctx, start + Duration::from_millis(70));
        let rest = pty.read_available(Duration::from_millis(120));
        assert_eq!(String::from_utf8_lossy(&rest), "B");
        assert!(!app.multi_run_active(), "the sequence finished");
    }

    #[test]
    #[cfg(unix)]
    fn looping_waits_the_last_entry_delay_and_stops_at_the_configured_count() {
        let Some(mut pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.serial.port = pty.slave_path.clone();
        if let Err(e) = app.link.open(&app.settings.serial) {
            eprintln!("skipping: cannot open serial port: {e}");
            return;
        }
        // The delay lives on the entry, including across the round boundary:
        // there is no separate round gap any more.
        arm(&mut app, &[("A", 50)]);
        app.settings.multi.loop_count = Some(2);
        app.settings.multi.loop_enabled = true;

        let start = Instant::now();
        app.start_multi_run(true, start);

        // Round 1.
        app.pump_multi(&ctx, start);
        // Inside the entry's delay: nothing more yet.
        app.pump_multi(&ctx, start + Duration::from_millis(20));
        let first = pty.read_available(Duration::from_millis(80));
        assert_eq!(String::from_utf8_lossy(&first), "A", "delay not respected");
        assert!(app.multi_run_active(), "still looping");

        // Round 2, then the limit stops it.
        app.pump_multi(&ctx, start + Duration::from_millis(60));
        let second = pty.read_available(Duration::from_millis(120));
        assert_eq!(String::from_utf8_lossy(&second), "A");
        assert!(!app.multi_run_active(), "stopped after two rounds");
        assert!(
            !app.settings.multi.loop_enabled,
            "the loop switch is cleared"
        );
    }

    #[test]
    fn enabled_indices_skip_unchecked_and_empty_entries() {
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        for (index, (text, enabled)) in [("a", true), ("b", false), ("", true), ("c", true)]
            .into_iter()
            .enumerate()
        {
            let entry = app.settings.multi.entry_mut(index).expect("entry");
            entry.text = text.to_owned();
            entry.enabled = enabled;
        }
        assert_eq!(app.settings.multi.enabled_indices(), vec![0, 3]);
    }

    #[test]
    fn messages_are_deduplicated_and_bounded() {
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.info("same");
        app.info("same");
        assert_eq!(app.messages.len(), 1, "an immediate repeat collapses");
        assert_eq!(app.latest_message().map(|m| m.text.as_str()), Some("same"));

        for i in 0..80 {
            app.error(format!("error {i}"));
        }
        assert!(app.messages.len() <= 50, "message list must be bounded");
    }

    #[test]
    fn payload_encoding_follows_the_send_settings() {
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.send.buffer = "AT".to_owned();
        app.settings.send.append_newline = false;
        assert_eq!(app.build_payload().unwrap(), b"AT");

        app.settings.send.append_newline = true;
        assert_eq!(app.build_payload().unwrap(), b"AT\r\n");

        app.settings.send.hex = true;
        app.settings.send.buffer = "01 0A".to_owned();
        assert_eq!(app.build_payload().unwrap(), vec![0x01, 0x0A, b'\r', b'\n']);

        app.settings.send.buffer = "0G".to_owned();
        assert!(app.build_payload().is_err(), "bad hex must be reported");
    }

    #[test]
    fn switching_to_auto_encoding_restarts_detection() {
        // Asserted behaviourally rather than by reading the label: the label is
        // translated, which made this test depend on whichever language another
        // test happened to leave active.
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        assert!(app.settings.encoding.is_none(), "auto-detect by default");

        app.ingest(b"AT+GMR\r\n", 0);
        assert!(app.auto.total() > 0, "the detector saw the data");

        app.set_encoding(Some(crate::encoding::EncodingId::Gbk));
        app.set_encoding(None);
        assert!(
            app.settings.encoding.is_none(),
            "an explicit choice is cleared"
        );
        assert_eq!(app.auto.total(), 0, "auto-detect starts over from scratch");
    }

    /// Lay out the real panels at a given window size.
    fn frame_at(ctx: &egui::Context, app: &mut XcomApp, size: (f32, f32)) {
        for _ in 0..2 {
            let mut raw = input();
            raw.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size.0, size.1),
            ));
            full_frame(ctx, app, raw);
        }
    }

    /// A loop under way, in the state `pump_multi` would have left it.
    ///
    /// Built by hand rather than driven through a port: these are layout tests,
    /// and it is the run in flight that the panel has to lay out around.
    fn looping(app: &mut XcomApp) {
        app.settings.multi.loop_enabled = true;
        app.settings.multi.loop_count = Some(10);
        app.multi_cycles = 3;
        app.multi_run = Some(MultiRun {
            queue: vec![0, 1, 2],
            position: 1,
            last_sent: Some(SentEntry {
                index: 12,
                ordinal: 3,
                round_len: 10,
            }),
            next_at: Instant::now(),
            looping: true,
        });
    }

    /// The multi-send table fits its panel: every column of the row and every
    /// entry on the page, with no scroll bar to reach them.
    ///
    /// The row is eight columns wide, and a row that overflows the panel grows a
    /// horizontal scroll bar.  That bar takes the send buttons and the delay
    /// fields out of reach, and with a solid scroll style it also eats a row's
    /// height off the bottom of the table, so the last entry needs scrolling to.
    #[test]
    fn the_multi_send_table_fits_without_scrolling() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        for (w, h) in [
            (880.0, 540.0),
            (1200.0, 780.0),
            (1280.0, 840.0),
            (1920.0, 1080.0),
            (2560.0, 1440.0),
        ] {
            for running in [false, true] {
                let ctx = egui::Context::default();
                let mut app = seeded();
                app.settings.show_tool_panel = true;
                app.side_tab = SideTab::Multi;
                // Match the user's actual persisted panel width and font size.
                app.settings.right_panel_width = 498.08594;
                app.settings.fonts.ui_size = 13.0;
                if running {
                    looping(&mut app);
                }
                frame_at(&ctx, &mut app, (w, h));

                let (viewport, content, scrollbars) = panels::probe::multi_table();
                assert!(
                    !scrollbars[0] && !scrollbars[1],
                    "{w}x{h} (looping: {running}): egui laid out a scrollbar \
                     (horizontal={}, vertical={})",
                    scrollbars[0],
                    scrollbars[1]
                );
                assert!(
                    viewport.height() > 0.0,
                    "{w}x{h}: the table was not laid out"
                );
                assert!(
                    content.x <= viewport.width() + 0.5,
                    "{w}x{h} (looping: {running}): the table is {:.1} wide where it has \
                     {:.1}, so its columns need a horizontal scroll bar",
                    content.x,
                    viewport.width()
                );
                assert!(
                    content.y <= viewport.height() + 0.5,
                    "{w}x{h} (looping: {running}): the table is {:.1} tall where it has \
                     {:.1}, so the last entry needs scrolling into view",
                    content.y,
                    viewport.height()
                );
                let last_row = panels::probe::multi_last_row_rect();
                assert!(
                    last_row.min.y >= viewport.min.y - 0.5
                        && last_row.max.y <= viewport.max.y + 0.5,
                    "{w}x{h} (looping: {running}): last row is outside the visible \
                     table viewport: row={last_row:?}, viewport={viewport:?}"
                );
            }
        }
    }

    /// The entries a page holds are the ones that fit: one more row would not.
    ///
    /// Asking for more is what leaves the last entry half out of the table;
    /// asking for fewer wastes a row.
    #[test]
    fn the_page_holds_every_entry_that_fits() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        for (w, h) in [(1200.0, 780.0), (1280.0, 840.0), (1920.0, 1080.0)] {
            let ctx = egui::Context::default();
            let mut app = seeded();
            app.side_tab = SideTab::Multi;
            frame_at(&ctx, &mut app, (w, h));

            let (viewport, content, scrollbars) = panels::probe::multi_table();
            assert!(
                !scrollbars[0] && !scrollbars[1],
                "{w}x{h}: egui laid out a scrollbar (horizontal={}, vertical={})",
                scrollbars[0],
                scrollbars[1]
            );
            let row_height = 20.0_f32.max(
                ctx.fonts_mut(|fonts| {
                    fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
                }) + 4.0,
            );
            let pitch = row_height + 4.0;
            assert!(
                viewport.height() - content.y < pitch,
                "{w}x{h}: {:.1} of the table is left empty below the last entry, \
                 which is room for another row",
                viewport.height() - content.y
            );
        }
    }

    /// The multi-send table fits with the wider English headers too.
    #[test]
    fn the_multi_send_table_fits_in_english() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::EnUs);
        let (w, h) = (1920.0, 1080.0);
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.show_tool_panel = true;
        app.side_tab = SideTab::Multi;
        frame_at(&ctx, &mut app, (w, h));

        let (viewport, content, scrollbars) = panels::probe::multi_table();
        assert!(
            !scrollbars[0] && !scrollbars[1],
            "{w}x{h}: egui laid out a scrollbar (horizontal={}, vertical={})",
            scrollbars[0],
            scrollbars[1]
        );
        assert!(
            content.x <= viewport.width() + 0.5,
            "{w}x{h}: the English headers need a horizontal scroll bar: \
             {:.1} in {:.1}",
            content.x,
            viewport.width()
        );
    }

    /// The send status bar keeps its place under the table, whatever the run.
    ///
    /// Laid out as one wrapping row it was three rows tall while looping, and a
    /// panel that had reserved one clipped the rest: the loop state and the
    /// entry in flight ended up behind the window's status bar.  One fixed
    /// row, with overflow truncated, so the table's height — and the page
    /// boundary with it — does not move when a run starts or stops.
    #[test]
    fn the_send_status_bar_keeps_its_place() {
        let _guard = crate::i18n::test_lock();
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
        let mut heights = Vec::new();
        for (w, h) in [(880.0, 540.0), (1280.0, 840.0), (1920.0, 1080.0)] {
            for running in [false, true] {
                let ctx = egui::Context::default();
                let mut app = seeded();
                app.settings.show_tool_panel = true;
                app.side_tab = SideTab::Multi;
                if running {
                    looping(&mut app);
                }
                frame_at(&ctx, &mut app, (w, h));

                let bar = panels::probe::multi_send_status_rect();
                let panel = panels::probe::tool_panel_rect();
                let window_bar = panels::probe::bottom_status_rect();
                assert!(
                    bar.height() > 0.0,
                    "{w}x{h}: the status bar was not laid out"
                );
                // One row of state plus the rule: anything taller means the
                // status text wrapped instead of being kept on one line.
                let row_height = 20.0_f32.max(
                    ctx.fonts_mut(|fonts| {
                        fonts.row_height(&egui::FontId::proportional(app.settings.fonts.ui_size))
                    }) + 4.0,
                );
                assert!(
                    bar.height() <= row_height + 6.0,
                    "{w}x{h} (looping: {running}): the status bar is {}px tall, more \
                     than one {row_height:.1}px row",
                    bar.height()
                );
                assert!(
                    bar.max.y <= panel.max.y + 0.5,
                    "{w}x{h} (looping: {running}): the status bar ends at {:.1}, below \
                     the panel's {:.1}, so its last line is cut off",
                    bar.max.y,
                    panel.max.y
                );
                assert!(
                    panel.max.y - bar.max.y <= 1.0,
                    "{w}x{h} (looping: {running}): the status bar stops {:.1} short of \
                     the panel's bottom",
                    panel.max.y - bar.max.y
                );
                assert!(
                    bar.max.y <= window_bar.min.y,
                    "{w}x{h} (looping: {running}): the window's status bar covers the \
                     send status bar ({:.1} vs {:.1})",
                    bar.max.y,
                    window_bar.min.y
                );
                if running {
                    assert!(
                        !panels::probe::multi_status_text().is_empty(),
                        "{w}x{h}: a run in flight must be named in the bar"
                    );
                }
                heights.push(bar.height());
            }
        }
        let first = heights[0];
        assert!(
            heights.iter().all(|height| (height - first).abs() < 0.5),
            "the bar changes height with the run, which moves the table: {heights:?}"
        );
    }

    /// A pending frame must not resize the window.
    ///
    /// The status bar used to append a live "receiving" preview after RI.
    /// Whenever a burst left a frame incomplete, that label wrapped the bar
    /// onto a second row, the centre column lost 15px, and the receive area
    /// visibly jumped on every burst.  The bar now stays one row; the live
    /// line itself is already painted at the end of the log.
    #[test]
    fn a_pending_frame_does_not_move_the_receive_area() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.serial.port = "/dev/ttyUSB0".to_owned();
        app.settings.display.timestamp = true;
        let display = app.settings.display;
        app.log.set_options(display);
        app.ingest(b"hello\r\nworld\r\n", 0);

        // 900x640 is narrow enough that the old preview wrapped the bar.
        let mut bars = Vec::new();
        let mut centre = Vec::new();
        for pending in [false, true, false, true] {
            if pending {
                // No terminator: this chunk stays as the live row.
                app.ingest(&[b'x'; 60], 0);
            } else {
                app.ingest(b"\r\n", 0);
            }
            let mut slot = egui::Rect::NOTHING;
            // Two passes: panels remember their size from the previous frame,
            // so a one-frame growth would only be visible on the second.
            for _ in 0..2 {
                let mut raw = input();
                raw.screen_rect = Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 640.0),
                ));
                slot = full_frame(&ctx, &mut app, raw);
            }
            bars.push(panels::probe::bottom_status_rect().height());
            centre.push(slot.height());
        }
        assert!(
            bars.iter().all(|h| (h - bars[0]).abs() < 0.5),
            "the status bar changes height when a frame is incomplete: {bars:?}"
        );
        assert!(
            centre.iter().all(|h| (h - centre[0]).abs() < 0.5),
            "the receive area moves when a frame is incomplete: {centre:?}"
        );
    }

    /// Switching auto-scroll on must actually go to the newest row.
    ///
    /// `ScrollArea::stick_to_bottom` only follows once the offset already
    /// touches the end, so a view that had been left at the top kept ignoring
    /// the option no matter how much data arrived.
    #[test]
    fn enabling_auto_scroll_jumps_to_the_newest_row() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        app.settings.auto_scroll = false;
        for i in 0..80 {
            app.log.append(format!("line {i}\r\n").as_bytes(), 0);
        }
        frame_at(&ctx, &mut app, (1000.0, 700.0));

        let (off, content, viewport) = crate::ui::log_view::probe::scroll();
        assert_eq!(off.y, 0.0, "the option is off, yet the view scrolled");
        let bottom = (content.y - viewport.y).max(0.0);
        assert!(bottom > 0.0, "the fixture needs more rows than fit");

        app.set_auto_scroll(true);
        app.log.append(b"newest\r\n", 0);
        // One frame only: the jump must not wait for a second pass.
        let mut raw = input();
        raw.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 700.0),
        ));
        full_frame(&ctx, &mut app, raw);

        let (off, content, viewport) = crate::ui::log_view::probe::scroll();
        let bottom = (content.y - viewport.y).max(0.0);
        assert!(
            (off.y - bottom).abs() < 0.5,
            "enabling auto-scroll left the view at {:.1}, not the newest row at {:.1}",
            off.y,
            bottom
        );

        // And it keeps following from there.
        app.log.append(b"another\r\n", 0);
        frame_at(&ctx, &mut app, (1000.0, 700.0));
        let (off, content, viewport) = crate::ui::log_view::probe::scroll();
        let bottom = (content.y - viewport.y).max(0.0);
        assert!(
            (off.y - bottom).abs() < 0.5,
            "auto-scroll stopped following at {:.1} (end is {:.1})",
            off.y,
            bottom
        );
    }

    /// Enabling auto-scroll drops a selection, since a selection is what
    /// suspends following; otherwise the switch would look broken.
    #[test]
    fn enabling_auto_scroll_releases_the_selection() {
        let mut app = seeded();
        app.settings.auto_scroll = false;
        app.log_view.select_all(&app.log);
        assert!(app.log_view.has_selection());

        app.set_auto_scroll(true);
        assert!(!app.log_view.has_selection());
        assert!(app.settings.auto_scroll);
    }

    /// The receive view's row pitch must be a whole number of physical pixels.
    ///
    /// A `ScrollArea` rounds its content origin to a pixel.  With a fractional
    /// pitch (egui's row height is e.g. 15.125px) that rounding drifts against
    /// the rows as content grows, so every visible line wobbles by one pixel in
    /// a slow wave while the log scrolls.  Snapping the pitch removes the
    /// drift, and the text stays put on the same pixels.
    #[test]
    fn the_receive_row_pitch_is_pixel_aligned() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(ppp);
            let mut app = seeded();
            // Exercise the extra-spacing path too: pitch = font height + spacing.
            app.settings.line_spacing = 1.3;
            full_frame(&ctx, &mut app, input());

            let (row_height, recorded_ppp) = crate::ui::log_view::probe::row_pitch();
            assert!(
                row_height > 0.0,
                "ppp={ppp}: the receive view was not laid out"
            );
            let pixels = row_height * recorded_ppp;
            assert!(
                (pixels - pixels.round()).abs() < 1e-3,
                "ppp={ppp}: the row pitch is {pixels}px, not a whole number of pixels"
            );
        }
    }
}
