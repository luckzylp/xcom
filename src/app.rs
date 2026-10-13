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
use crate::settings::Settings;
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

/// A multi-send sequence in progress.
///
/// Entries are sent one at a time with each entry's own delay applied after it,
/// and a completed round waits `round_gap_ms` before the next one.  Pacing the
/// sequence rather than firing everything at once is what makes per-entry
/// delays and the round gap possible.
pub(crate) struct MultiRun {
    page: usize,
    queue: Vec<usize>,
    position: usize,
    /// When the next entry may go out.
    next_at: Instant,
    /// Whether to start another round after this one.
    looping: bool,
    /// Delay between rounds, from the settings.
    round_gap: Duration,
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
    pub(crate) show_side_panel: bool,
    pub(crate) show_about: bool,

    /// Handshake line states, sampled periodically.
    pub(crate) signals: Signals,
    signals_at: Instant,

    /// Last theme applied, so styling only runs on change.
    applied_theme: Option<crate::theme::ThemeMode>,
    /// Last language applied to the window title.
    applied_lang: Option<crate::i18n::Lang>,
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
            show_side_panel: true,
            show_about: false,
            signals: Signals::default(),
            signals_at: Instant::now(),
            applied_theme: None,
            applied_lang: None,
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

    /// Report the CJK font status once, after the first frame.
    fn note_font_status(&mut self) {
        if self.font_warning_shown {
            return;
        }
        self.font_warning_shown = true;
        match crate::fonts::installed_family() {
            Some(family) => log::info!("界面字体: {family}"),
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
        if let Some(last) = self.messages.last_mut() {
            if last.text == text {
                last.at = Instant::now();
                return;
            }
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
        if let Err(err) = self.link.send(payload) {
            self.error(err);
            return;
        }
        let buffer = self.settings.send.buffer.clone();
        self.settings.send.remember(&buffer);
        self.mark_settings_dirty();
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

        for event in self.link.poll() {
            match event {
                SerialEvent::Data(bytes) => {
                    saw_data = true;
                    self.rx_bytes += bytes.len() as u64;
                    if self.show_paused {
                        // Freeze the display: buffer instead of rendering.
                        self.pending_rx.extend_from_slice(&bytes);
                    } else {
                        self.ingest(&bytes, epoch_ms);
                    }
                    if self.protocol.is_running() || self.protocol_panel_visible() {
                        self.protocol.on_bytes(&bytes, now);
                    }
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

        if saw_data && !self.show_paused {
            ctx.request_repaint();
        }

        // Handshake lines change slowly; sampling them at 5 Hz is plenty and
        // avoids ioctl traffic on every frame.
        if self.link.is_open() && self.signals_at.elapsed() >= Duration::from_millis(200) {
            self.signals_at = Instant::now();
            if let Some(signals) = self.link.signals() {
                self.signals = signals;
            }
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
        self.show_side_panel && self.side_tab == SideTab::Protocol
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

    /// Start sending the checked entries of `page`, in order.
    ///
    /// `looping` repeats the sequence, waiting the configured round gap after
    /// each complete pass.
    pub(crate) fn start_multi_run(&mut self, page: usize, looping: bool, now: Instant) {
        let queue = self.settings.multi.enabled_indices(page);
        if queue.is_empty() {
            self.warn(tr("当前页没有已勾选的发送条目"));
            self.settings.multi.loop_enabled = false;
            return;
        }
        self.multi_cycles = 0;
        self.multi_run = Some(MultiRun {
            page,
            queue,
            position: 0,
            next_at: now,
            looping,
            round_gap: Duration::from_millis(self.settings.multi.loop_period_ms.max(1)),
        });
    }

    /// Abandon any sequence in progress.
    pub(crate) fn stop_multi_run(&mut self, reason: &str) {
        if self.multi_run.take().is_some() {
            self.info(reason.to_owned());
        }
    }

    pub(crate) fn multi_run_active(&self) -> bool {
        self.multi_run.is_some()
    }

    /// Advance the running sequence.  Sends at most one entry per call.
    fn pump_multi(&mut self, ctx: &Context, now: Instant) {
        let Some(run) = self.multi_run.as_ref() else {
            return;
        };
        if !self.link.is_open() {
            self.multi_run = None;
            self.warn(tr("串口已关闭，多条发送已停止"));
            return;
        }
        if now < run.next_at {
            ctx.request_repaint_after(run.next_at - now);
            return;
        }

        // Copy out what the send needs, then let go of the borrow.
        let page = run.page;
        let index = run.queue[run.position];
        let end_of_round = run.position + 1 == run.queue.len();
        let queue_len = run.queue.len();
        let looping = run.looping;
        let round_gap = run.round_gap;

        self.send_multi_entry(page, index, true);
        let delay = Duration::from_millis(self.settings.multi.delay_after(page, index));

        let mut stop_looping = false;
        if end_of_round && looping {
            self.multi_cycles += 1;
            stop_looping = self
                .settings
                .multi
                .loop_count
                .is_some_and(|limit| self.multi_cycles >= limit);
        }

        if let Some(run) = self.multi_run.as_mut() {
            if end_of_round {
                run.position = 0;
                run.next_at = now + if looping { round_gap } else { Duration::ZERO };
            } else {
                run.position += 1;
                run.next_at = now + delay;
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
        } else if end_of_round && !looping {
            self.multi_run = None;
            let message = crate::i18n::tr1("已发送 {} 条已勾选条目", queue_len);
            self.ok(message);
        }
        ctx.request_repaint_after(Duration::from_millis(5));
    }

    /// Encode and send one multi-send entry.  Returns whether it was queued.
    pub(crate) fn send_multi_entry(&mut self, page: usize, index: usize, from_loop: bool) -> bool {
        let Some(entry) = self.settings.multi.entry(page, index) else {
            return false;
        };
        if !entry.is_sendable() {
            if !from_loop {
                self.warn(crate::i18n::tr1("条目 {} 为空", index));
            }
            return false;
        }
        let encoded = match self.settings.multi.encode_entry(entry) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.error(crate::i18n::tr2("条目 {} 编码失败: {}", index, err));
                return false;
            }
        };
        let label = index.to_string();
        self.send_multi_entry_bytes(encoded, &crate::i18n::tr1("发送条目 {}", label))
    }

    /// Queue an already-encoded multi-send payload, reporting whether it went.
    fn send_multi_entry_bytes(&mut self, bytes: Vec<u8>, what: &str) -> bool {
        if !self.link.is_open() {
            self.error(format!("{}: {}", tr("串口未打开"), what));
            return false;
        }
        match self.link.send(bytes) {
            Ok(()) => true,
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

        panels::top_bar(self, ui);
        panels::status_bar(self, ui);
        panels::left_panel(self, ui);
        if self.show_side_panel {
            panels::side_panel(self, ui);
        }
        panels::center_panel(self, ui);
        panels::apply_splitters(self, ui);
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
    let options = LogViewOptions {
        auto_scroll: app.settings.auto_scroll,
        font_size: app.settings.log_font_size,
        line_spacing: app.settings.line_spacing,
    };

    // Tell the store how many columns fit, so text wrapping matches the panel.
    let font = egui::FontId::monospace(app.settings.log_font_size);
    let char_width = ui.ctx().fonts_mut(|f| f.glyph_width(&font, 'M'));
    if char_width > 0.0 && app.settings.display.wrap {
        let cols = (ui.available_width() / char_width).floor().max(8.0) as usize;
        app.update_wrap_width(cols, ui.ctx());
    }
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
        let slot = std::cell::Cell::new(egui::Rect::NOTHING);
        let mut output = ctx.run_ui(raw, |ui| {
            panels::top_bar(app, ui);
            panels::status_bar(app, ui);
            panels::left_panel(app, ui);
            if app.show_side_panel {
                panels::side_panel(app, ui);
            }
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

    #[test]
    fn dragging_the_settings_separator_widens_the_panel() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.show_side_panel = false;

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
    fn shrinking_the_settings_panel_persists() {
        // Reported: dragging the left separator *left* clipped the panel's
        // content during the gesture and then snapped back on release, while
        // the receive area never grew.
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.show_side_panel = false;

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
    fn shrinking_the_tool_panel_persists_in_every_language() {
        // Reported: in English the tool panel snapped back when shrunk; in
        // Chinese it did not, which points at something width-dependent in the
        // translated strings.
        let _guard = crate::i18n::test_lock();
        for language in [crate::i18n::Lang::ZhCn, crate::i18n::Lang::EnUs] {
            crate::i18n::set_lang(language);
            let ctx = egui::Context::default();
            let mut app = XcomApp::headless(crate::settings::Settings::default());
            app.side_tab = SideTab::Multi;

            let before = full_frame(&ctx, &mut app, input());
            // The right panel's left edge; shrinking it moves this right.
            let grab = egui::pos2(before.max.x, before.center().y);
            drag(&ctx, &mut app, grab, egui::pos2(grab.x + 90.0, grab.y));

            let mut after = full_frame(&ctx, &mut app, input());
            for _ in 0..5 {
                after = full_frame(&ctx, &mut app, input());
            }
            assert!(
                after.max.x > before.max.x + 50.0,
                "{language:?}: the tool panel snapped back: {} -> {}",
                before.max.x,
                after.max.x
            );
        }
        crate::i18n::set_lang(crate::i18n::Lang::ZhCn);
    }

    #[test]
    fn the_tool_panel_splitter_moves_both_ways() {
        // The right-hand panel is anchored to the window's right edge, so its
        // size is `fixed_edge - pointer.x`.  Getting that sign wrong makes the
        // panel jump to its minimum and then refuse to widen.
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
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
    fn the_tool_panel_splitter_stays_within_its_range() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        let rect = full_frame(&ctx, &mut app, input());

        // Drag far past the maximum, then far past the minimum.
        drag(
            &ctx,
            &mut app,
            egui::pos2(rect.max.x, rect.center().y),
            egui::pos2(rect.min.x, rect.center().y),
        );
        assert_eq!(app.settings.right_panel_width, panels::RIGHT_PANEL_MAX);

        let rect = full_frame(&ctx, &mut app, input());
        drag(
            &ctx,
            &mut app,
            egui::pos2(rect.max.x, rect.center().y),
            egui::pos2(rect.max.x + 4000.0, rect.center().y),
        );
        assert_eq!(app.settings.right_panel_width, panels::RIGHT_PANEL_MIN);
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
                    std::ptr::null(),
                    std::ptr::null(),
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
    fn timed_send_repeats_on_its_period() {
        let Some(mut pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.serial.port = pty.slave_path.clone();
        app.link
            .open(&app.settings.serial)
            .expect("opening a pty should succeed");

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
        app.show_side_panel = false;
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
    fn renders_one_multi_send_entry_per_page() {
        let ctx = egui::Context::default();
        let mut app = seeded();
        for page in 0..crate::multi::PAGES {
            app.settings.multi.page = page;
            for index in 0..crate::multi::ENTRIES_PER_PAGE {
                if let Some(entry) = app.settings.multi.entry_mut(page, index) {
                    entry.text = format!("CMD{index}");
                    entry.note = "备注".to_owned();
                    entry.enabled = index % 2 == 0;
                    entry.hex = index % 3 == 0;
                }
            }
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
        for (size, spacing) in [(8.0, 0.0), (28.0, 10.0)] {
            let mut app = seeded();
            app.settings.log_font_size = size;
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
            let entry = app.settings.multi.entry_mut(0, index).expect("entry");
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
        app.start_multi_run(0, false, Instant::now());
        assert!(!app.multi_run_active(), "nothing to send");
        assert_eq!(app.messages.len(), 1);
        assert_eq!(app.messages[0].level, Level::Warning);
    }

    #[test]
    fn a_sequence_stops_when_the_port_is_not_open() {
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        arm(&mut app, &[("A", 0), ("B", 0)]);
        app.start_multi_run(0, false, Instant::now());
        assert!(app.multi_run_active());
        app.pump_multi(&ctx, Instant::now());
        assert!(!app.multi_run_active(), "closed port stops the sequence");
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
        app.link
            .open(&app.settings.serial)
            .expect("opening a pty should succeed");
        arm(&mut app, &[("A", 60), ("B", 0)]);

        let start = Instant::now();
        app.start_multi_run(0, false, start);

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
    fn looping_waits_the_round_gap_and_stops_at_the_configured_count() {
        let Some(mut pty) = TestPty::open() else {
            eprintln!("skipping: openpty unavailable");
            return;
        };
        let ctx = egui::Context::default();
        let mut app = XcomApp::headless(crate::settings::Settings::default());
        app.settings.serial.port = pty.slave_path.clone();
        app.link
            .open(&app.settings.serial)
            .expect("opening a pty should succeed");
        arm(&mut app, &[("A", 0)]);
        app.settings.multi.loop_count = Some(2);
        app.settings.multi.loop_period_ms = 50;
        app.settings.multi.loop_enabled = true;

        let start = Instant::now();
        app.start_multi_run(0, true, start);

        // Round 1.
        app.pump_multi(&ctx, start);
        // Inside the round gap: nothing more yet.
        app.pump_multi(&ctx, start + Duration::from_millis(20));
        let first = pty.read_available(Duration::from_millis(80));
        assert_eq!(String::from_utf8_lossy(&first), "A", "gap not respected");
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
            let entry = app.settings.multi.entry_mut(0, index).expect("entry");
            entry.text = text.to_owned();
            entry.enabled = enabled;
        }
        assert_eq!(app.settings.multi.enabled_indices(0), vec![0, 3]);
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
}
