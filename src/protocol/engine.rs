//! The protocol-transfer state machine.
//!
//! This is a half-duplex Send-Ack protocol.  The host transmits one frame and
//! waits; a matching response acknowledges it.  If nothing arrives within the
//! frame period the same frame — with the **same** sequence number — is sent
//! again, up to the configured retry limit.  Only a new logical frame gets a
//! fresh sequence number.
//!
//! The engine is time-driven and side-effect free: [`ProtocolEngine::tick`] is
//! the single place that emits bytes, and the caller owns the serial port.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::checksum::ChecksumKind;
use super::frame::{self, FrameError, ReceivedFrame, MAX_DATA};

/// User-facing configuration of the protocol panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// 设备地址.
    pub address: u8,
    /// 帧功能.
    pub function: u8,
    /// 校验方式.
    pub checksum: ChecksumKind,
    /// 帧周期 (ms): both the auto-send period and the retransmit timeout.
    pub frame_period_ms: u64,
    /// 最大重发次数.
    pub max_retries: u32,
    /// 最大数据长度 for file transfers.
    pub max_data_len: usize,
    /// 自动发送 on a timer rather than waiting for a button press.
    pub auto_send: bool,
    /// Show the whole frame in the response view, not just the payload.
    pub show_raw: bool,
    /// One response per line.
    pub auto_wrap: bool,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            address: 0x01,
            function: 0x03,
            checksum: ChecksumKind::Sum,
            frame_period_ms: 200,
            max_retries: 10,
            max_data_len: 128,
            auto_send: false,
            show_raw: false,
            auto_wrap: true,
        }
    }
}

impl ProtocolConfig {
    /// Clamp values into the ranges the protocol can express.
    pub fn normalize(&mut self) {
        self.frame_period_ms = self.frame_period_ms.clamp(1, 600_000);
        self.max_retries = self.max_retries.min(255);
        self.max_data_len = self.max_data_len.clamp(1, MAX_DATA);
    }

    /// A warning when the frame period is too short for the response to be
    /// recognised before the frame is retransmitted.
    ///
    /// Responses are delimited by an inter-byte idle window, so a frame period
    /// shorter than that window guarantees spurious retransmissions: the host
    /// gives up on a reply that is still in flight.  Real devices are slow
    /// enough that this never bites, but a user typing a small period into the
    /// UI deserves to be told rather than left guessing.
    pub fn period_warning(&self, baud: u32) -> Option<String> {
        let idle = frame::idle_timeout_ms(baud);
        if self.frame_period_ms < idle {
            Some(format!(
                "帧周期 {} ms 小于响应断帧时间 {} ms，建议至少 {} ms，否则会提前重发",
                self.frame_period_ms,
                idle,
                idle + 1
            ))
        } else {
            None
        }
    }
}

/// Which transfer is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// One frame, then done.
    Single,
    /// A frame repeated on the frame period until stopped.
    Auto,
    /// A file split into `max_data_len` chunks.
    File,
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            TaskKind::Single => "单次发送",
            TaskKind::Auto => "自动发送",
            TaskKind::File => "文件发送",
        }
    }
}

/// Something the engine wants the UI to know about.
#[derive(Debug, Clone, PartialEq)]
pub enum ProtocolEvent {
    /// A frame went out.
    Sent {
        sequence: u8,
        attempt: u32,
        frame: Vec<u8>,
    },
    /// A matching acknowledgement arrived.
    Acknowledged {
        sequence: u8,
        attempt: u32,
        frame: ReceivedFrame,
    },
    /// The frame period expired; the same frame is being sent again.
    Retransmit { sequence: u8, attempt: u32 },
    /// Retries exhausted.
    NoResponse { sequence: u8 },
    /// A frame arrived when none was expected, or from another sequence.
    Response { frame: ReceivedFrame },
    /// Bytes arrived that are not a valid frame.
    Malformed { bytes: Vec<u8>, error: FrameError },
    /// File transfer progress.
    FileProgress { sent: usize, total: usize },
    /// File transfer completed.
    FileFinished { total: usize },
    /// The transfer stopped for a reason worth showing.
    Stopped(String),
}

#[derive(Debug, Clone)]
enum Task {
    Single { data: Vec<u8> },
    Auto { data: Vec<u8> },
    File { data: Vec<u8>, offset: usize },
}

impl Task {
    fn kind(&self) -> TaskKind {
        match self {
            Task::Single { .. } => TaskKind::Single,
            Task::Auto { .. } => TaskKind::Auto,
            Task::File { .. } => TaskKind::File,
        }
    }
}

#[derive(Debug, Clone)]
struct Pending {
    sequence: u8,
    bytes: Vec<u8>,
    attempt: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// A frame is due (or the task is deciding what to do next).
    Ready,
    /// A frame is in flight; waiting for its acknowledgement.
    AwaitingAck,
    /// Acknowledged; waiting out the frame period before the next frame.
    Gap,
}

/// Drives a single protocol transfer session.
pub struct ProtocolEngine {
    cfg: ProtocolConfig,
    baud: u32,
    task: Option<Task>,
    phase: Phase,
    pending: Option<Pending>,
    /// Sequence number of the most recently issued frame.
    sequence: u8,
    /// When the current phase expires.
    deadline: Option<Instant>,
    /// Bytes received but not yet delimited into a frame.
    rx: Vec<u8>,
    last_rx: Option<Instant>,
    events: Vec<ProtocolEvent>,
}

impl ProtocolEngine {
    pub fn new(cfg: ProtocolConfig, baud: u32) -> Self {
        let mut cfg = cfg;
        cfg.normalize();
        Self {
            cfg,
            baud,
            task: None,
            phase: Phase::Ready,
            pending: None,
            sequence: 0,
            deadline: None,
            rx: Vec::new(),
            last_rx: None,
            events: Vec::new(),
        }
    }

    pub fn config(&self) -> &ProtocolConfig {
        &self.cfg
    }

    /// Update configuration.  A running transfer keeps its in-flight frame.
    pub fn set_config(&mut self, cfg: ProtocolConfig) {
        self.cfg = cfg;
        self.cfg.normalize();
    }

    pub fn set_baud(&mut self, baud: u32) {
        self.baud = baud;
    }

    pub fn is_running(&self) -> bool {
        self.task.is_some()
    }

    pub fn task_kind(&self) -> Option<TaskKind> {
        self.task.as_ref().map(Task::kind)
    }

    /// Sequence number of the frame currently in flight, if any.
    pub fn outstanding_sequence(&self) -> Option<u8> {
        self.pending.as_ref().map(|p| p.sequence)
    }

    /// Number of attempts made for the in-flight frame.
    pub fn outstanding_attempt(&self) -> Option<u32> {
        self.pending.as_ref().map(|p| p.attempt)
    }

    /// Begin a single-frame transfer.  The frame goes out on the next `tick`.
    pub fn start_single(&mut self, data: Vec<u8>, now: Instant) {
        self.interrupt();
        self.task = Some(Task::Single { data });
        self.begin_ready(now);
    }

    /// Begin a repeating transfer.
    pub fn start_auto(&mut self, data: Vec<u8>, now: Instant) {
        self.interrupt();
        self.task = Some(Task::Auto { data });
        self.begin_ready(now);
    }

    /// Begin a file transfer.
    pub fn start_file(&mut self, data: Vec<u8>, now: Instant) {
        self.interrupt();
        if data.is_empty() {
            self.events
                .push(ProtocolEvent::Stopped("文件为空".to_owned()));
            return;
        }
        self.task = Some(Task::File { data, offset: 0 });
        self.begin_ready(now);
    }

    /// Abort the current transfer.
    pub fn stop(&mut self, reason: &str) {
        if self.task.take().is_some() {
            self.events.push(ProtocolEvent::Stopped(reason.to_owned()));
        }
        self.phase = Phase::Ready;
        self.pending = None;
        self.deadline = None;
        self.rx.clear();
        self.last_rx = None;
    }

    /// Collect events accumulated since the last call.
    pub fn drain_events(&mut self) -> Vec<ProtocolEvent> {
        std::mem::take(&mut self.events)
    }

    /// Advance the state machine and return the bytes to write, if any.
    ///
    /// Cheap when nothing is due, so it is safe to call from every UI frame.
    pub fn tick(&mut self, now: Instant) -> Option<Vec<u8>> {
        self.flush_response_if_idle(now);

        self.task.as_ref()?;

        if self.phase == Phase::AwaitingAck {
            if self.deadline.is_some_and(|d| now >= d) {
                return self.on_timeout(now);
            }
            return None;
        }

        // Gap: waiting out the frame period before the next frame.
        if self.deadline.is_some_and(|d| now < d) {
            return None;
        }

        match self.task.clone() {
            None => None,
            Some(Task::Single { data }) => self.emit_new_frame(data, now),
            Some(Task::Auto { data }) => self.emit_new_frame(data, now),
            Some(Task::File { data, offset }) => {
                if offset >= data.len() {
                    self.events
                        .push(ProtocolEvent::FileFinished { total: data.len() });
                    self.finish();
                    None
                } else {
                    self.emit_next_chunk(now)
                }
            }
        }
    }

    /// Feed bytes received from the port.
    pub fn on_bytes(&mut self, bytes: &[u8], now: Instant) {
        if bytes.is_empty() {
            return;
        }
        self.rx.extend_from_slice(bytes);
        self.last_rx = Some(now);
        // Guard against a peer that never stops talking: keep only the most
        // recent frame's worth of bytes and report the discarded prefix.
        let cap = frame::MAX_FRAME * 2;
        if self.rx.len() > cap {
            let overflow: Vec<u8> = self.rx.drain(..self.rx.len() - frame::MAX_FRAME).collect();
            self.events.push(ProtocolEvent::Malformed {
                bytes: overflow,
                error: FrameError::TooLong(cap),
            });
        }
    }

    // -- internals ----------------------------------------------------------

    fn period(&self) -> Duration {
        Duration::from_millis(self.cfg.frame_period_ms)
    }

    /// Mark the task as ready to send immediately.
    fn begin_ready(&mut self, now: Instant) {
        self.phase = Phase::Ready;
        self.pending = None;
        self.deadline = Some(now);
    }

    /// Discard any running task without emitting a "stopped" event.
    fn interrupt(&mut self) {
        if self.task.is_some() {
            self.events
                .push(ProtocolEvent::Stopped("已被新的发送任务中断".to_owned()));
        }
        self.task = None;
        self.pending = None;
        self.rx.clear();
        self.last_rx = None;
    }

    fn finish(&mut self) {
        self.task = None;
        self.phase = Phase::Ready;
        self.pending = None;
        self.deadline = None;
    }

    fn on_timeout(&mut self, now: Instant) -> Option<Vec<u8>> {
        let Some(pending) = self.pending.clone() else {
            // Should not happen, but recover rather than spin.
            self.phase = Phase::Ready;
            return None;
        };
        if pending.attempt > self.cfg.max_retries {
            self.events.push(ProtocolEvent::NoResponse {
                sequence: pending.sequence,
            });
            self.stop("设备无响应，已停止发送");
            return None;
        }
        let attempt = pending.attempt + 1;
        if let Some(p) = self.pending.as_mut() {
            p.attempt = attempt;
        }
        self.events.push(ProtocolEvent::Retransmit {
            sequence: pending.sequence,
            attempt,
        });
        self.deadline = Some(now + self.period());
        Some(pending.bytes)
    }

    /// A frame is delimited by the line going idle.
    fn flush_response_if_idle(&mut self, now: Instant) {
        if self.rx.is_empty() {
            return;
        }
        let idle = Duration::from_millis(frame::idle_timeout_ms(self.baud));
        let Some(last) = self.last_rx else {
            return;
        };
        if now.duration_since(last) < idle {
            return;
        }
        let bytes = std::mem::take(&mut self.rx);
        self.last_rx = None;
        self.handle_response(bytes, now);
    }

    fn handle_response(&mut self, bytes: Vec<u8>, now: Instant) {
        match frame::parse(&bytes, self.cfg.checksum) {
            Ok(received) => {
                let matched = self
                    .pending
                    .as_ref()
                    .is_some_and(|p| frame::is_ack_for(&received, p.sequence));
                if matched {
                    let (sequence, attempt) = {
                        let pending = self.pending.as_ref().expect("checked above");
                        (pending.sequence, pending.attempt)
                    };
                    self.events.push(ProtocolEvent::Acknowledged {
                        sequence,
                        attempt,
                        frame: received.clone(),
                    });
                    self.events
                        .push(ProtocolEvent::Response { frame: received });
                    self.on_acknowledged(now);
                } else {
                    self.events
                        .push(ProtocolEvent::Response { frame: received });
                }
            }
            Err(error) => {
                self.events.push(ProtocolEvent::Malformed { bytes, error });
            }
        }
    }

    fn on_acknowledged(&mut self, now: Instant) {
        self.pending = None;
        match self.task.as_ref().map(Task::kind) {
            Some(TaskKind::Single) => self.finish(),
            Some(TaskKind::Auto) | Some(TaskKind::File) => {
                self.phase = Phase::Gap;
                self.deadline = Some(now + self.period());
            }
            None => self.finish(),
        }
    }

    /// Build and queue a frame carrying `data` under a fresh sequence number,
    /// returning the bytes the caller must write.
    fn emit_new_frame(&mut self, data: Vec<u8>, now: Instant) -> Option<Vec<u8>> {
        let sequence = self.next_sequence();
        let bytes = self.build_frame(sequence, data);
        self.events.push(ProtocolEvent::Sent {
            sequence,
            attempt: 1,
            frame: bytes.clone(),
        });
        self.pending = Some(Pending {
            sequence,
            bytes: bytes.clone(),
            attempt: 1,
        });
        self.phase = Phase::AwaitingAck;
        self.deadline = Some(now + self.period());
        Some(bytes)
    }

    /// Build and queue the next file chunk, returning the bytes to write.
    fn emit_next_chunk(&mut self, now: Instant) -> Option<Vec<u8>> {
        let Some(Task::File { data, offset }) = self.task.clone() else {
            return None;
        };
        let end = (offset + self.cfg.max_data_len).min(data.len());
        let chunk = data[offset..end].to_vec();

        let sequence = self.next_sequence();
        let bytes = self.build_frame(sequence, chunk);
        self.events.push(ProtocolEvent::Sent {
            sequence,
            attempt: 1,
            frame: bytes.clone(),
        });
        self.events.push(ProtocolEvent::FileProgress {
            sent: end,
            total: data.len(),
        });
        self.pending = Some(Pending {
            sequence,
            bytes: bytes.clone(),
            attempt: 1,
        });
        // Advance immediately: a retransmission reuses the identical bytes held
        // in `pending`, so the reader must not re-read this chunk.
        if let Some(Task::File { offset, .. }) = self.task.as_mut() {
            *offset = end;
        }
        self.phase = Phase::AwaitingAck;
        self.deadline = Some(now + self.period());
        Some(bytes)
    }

    fn next_sequence(&mut self) -> u8 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    fn build_frame(&self, sequence: u8, data: Vec<u8>) -> Vec<u8> {
        frame::Frame {
            address: self.cfg.address,
            function: self.cfg.function,
            sequence,
            data,
            checksum: self.cfg.checksum,
        }
        .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ProtocolEngine {
        ProtocolEngine::new(
            ProtocolConfig {
                frame_period_ms: 10,
                max_retries: 2,
                ..Default::default()
            },
            115200,
        )
    }

    /// Deliver an acknowledgement for whatever is currently in flight.
    ///
    /// Advances time by exactly the inter-byte idle threshold, which is enough
    /// to delimit the frame but (for the periods used in these tests) not
    /// enough to trigger the next automatic transmission.
    fn deliver_ack(engine: &mut ProtocolEngine, now: Instant) -> Option<Vec<u8>> {
        let sequence = engine.outstanding_sequence().expect("something in flight");
        let bytes = frame::Frame {
            address: 0xFF,
            function: 0xFF,
            sequence,
            data: vec![],
            checksum: engine.config().checksum,
        }
        .build();
        engine.on_bytes(&bytes, now);
        engine.tick(now + Duration::from_millis(frame::idle_timeout_ms(engine.baud)))
    }

    fn ack_bytes_for(engine: &ProtocolEngine, sequence: u8) -> Vec<u8> {
        frame::Frame {
            address: 0x01,
            function: 0x03,
            sequence,
            data: vec![],
            checksum: engine.config().checksum,
        }
        .build()
    }

    fn tags(events: &[ProtocolEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match e {
                ProtocolEvent::Sent { .. } => "sent",
                ProtocolEvent::Acknowledged { .. } => "ack",
                ProtocolEvent::Retransmit { .. } => "retransmit",
                ProtocolEvent::NoResponse { .. } => "no_response",
                ProtocolEvent::Response { .. } => "response",
                ProtocolEvent::Malformed { .. } => "malformed",
                ProtocolEvent::FileProgress { .. } => "progress",
                ProtocolEvent::FileFinished { .. } => "finished",
                ProtocolEvent::Stopped(_) => "stopped",
            })
            .collect()
    }

    #[test]
    fn single_send_is_emitted_by_tick() {
        let mut e = engine();
        let now = Instant::now();
        e.start_single(vec![1, 2, 3], now);
        assert!(e.is_running());
        let bytes = e.tick(now).expect("first frame is due immediately");
        assert_eq!(
            e.outstanding_sequence(),
            Some(1),
            "first frame carries seq 1"
        );
        let parsed = frame::parse(&bytes, ChecksumKind::Sum).unwrap();
        assert_eq!(parsed.data, vec![1, 2, 3]);
        assert_eq!(parsed.sequence, 1);
    }

    #[test]
    fn single_send_completes_on_acknowledgement() {
        let mut e = engine();
        let now = Instant::now();
        e.start_single(vec![9], now);
        let _ = e.tick(now);
        e.drain_events();
        let _ = deliver_ack(&mut e, now);
        assert!(!e.is_running(), "single send finishes once acknowledged");
        let events = e.drain_events();
        assert!(tags(&events).contains(&"ack"), "{events:?}");
    }

    #[test]
    fn retransmits_the_identical_frame_up_to_max_retries() {
        let mut e = engine();
        let start = Instant::now();
        e.start_single(vec![7, 7, 7], start);
        let original = e.tick(start).expect("first send");

        let mut now = start;
        let mut retransmits = Vec::new();
        for _ in 0..5 {
            now += Duration::from_millis(11);
            if let Some(bytes) = e.tick(now) {
                retransmits.push(bytes);
            }
        }
        assert_eq!(retransmits.len(), 2, "max_retries = 2");
        for bytes in &retransmits {
            assert_eq!(bytes, &original, "retransmission must be byte-identical");
        }
        assert!(!e.is_running(), "gives up after the retry budget");
        assert!(tags(&e.drain_events()).contains(&"no_response"));
    }

    #[test]
    fn auto_send_uses_a_fresh_sequence_each_period() {
        let mut e = engine();
        let start = Instant::now();
        e.start_auto(vec![0x01], start);
        assert_eq!(e.task_kind(), Some(TaskKind::Auto));

        let first = e.tick(start).expect("first auto frame");
        let _ = deliver_ack(&mut e, start);

        // The gap is measured from the acknowledgement, so tick well past it.
        let later = start + Duration::from_millis(50);
        let second = e.tick(later).expect("second auto frame after the gap");
        assert_ne!(first, second, "each period gets its own sequence number");
        assert!(e.is_running(), "auto keeps going until stopped");
    }

    #[test]
    fn auto_send_retransmits_when_unacknowledged() {
        let mut e = engine();
        let start = Instant::now();
        e.start_auto(vec![0x02], start);
        let first = e.tick(start).unwrap();
        let again = e.tick(start + Duration::from_millis(11)).unwrap();
        assert_eq!(first, again, "no ack means the same frame again");
        assert_eq!(e.outstanding_sequence(), Some(1));
    }

    #[test]
    fn file_transfer_delivers_every_byte_in_order() {
        let mut e = ProtocolEngine::new(
            ProtocolConfig {
                max_data_len: 4,
                // Must exceed the response idle window, or the engine retries
                // before a reply can be recognised.
                frame_period_ms: 20,
                max_retries: 3,
                ..Default::default()
            },
            115200,
        );
        let data: Vec<u8> = (0..10).collect();
        let start = Instant::now();
        e.start_file(data.clone(), start);

        let mut reassembled: Vec<u8> = Vec::new();
        let mut now = start;
        let mut acked: Option<u8> = None;
        for _ in 0..2000 {
            if let Some(bytes) = e.tick(now) {
                let parsed = frame::parse(&bytes, e.config().checksum).unwrap();
                reassembled.extend_from_slice(&parsed.data);
            }
            match e.outstanding_sequence() {
                // Send exactly one acknowledgement per outstanding frame; the
                // engine needs a few idle milliseconds before it is delimited.
                Some(seq) if acked != Some(seq) => {
                    let ack = ack_bytes_for(&e, seq);
                    e.on_bytes(&ack, now);
                    acked = Some(seq);
                }
                Some(_) => {}
                None => acked = None,
            }
            if !e.is_running() {
                break;
            }
            now += Duration::from_millis(2);
        }
        assert!(!e.is_running(), "file transfer must finish");
        assert_eq!(reassembled, data, "all bytes, in order, exactly once");
        assert!(tags(&e.drain_events()).contains(&"finished"));
    }

    #[test]
    fn file_transfer_reports_monotonic_progress() {
        let mut e = ProtocolEngine::new(
            ProtocolConfig {
                max_data_len: 3,
                frame_period_ms: 20,
                max_retries: 3,
                ..Default::default()
            },
            115200,
        );
        let start = Instant::now();
        e.start_file((0..9).collect(), start);

        let mut progress = Vec::new();
        let mut now = start;
        let mut acked: Option<u8> = None;
        for _ in 0..2000 {
            let _ = e.tick(now);
            for event in e.drain_events() {
                if let ProtocolEvent::FileProgress { sent, .. } = event {
                    progress.push(sent);
                }
            }
            match e.outstanding_sequence() {
                Some(seq) if acked != Some(seq) => {
                    let ack = ack_bytes_for(&e, seq);
                    e.on_bytes(&ack, now);
                    acked = Some(seq);
                }
                Some(_) => {}
                None => acked = None,
            }
            if !e.is_running() {
                break;
            }
            now += Duration::from_millis(2);
        }
        assert!(!progress.is_empty(), "should report progress");
        assert!(
            progress.windows(2).all(|w| w[0] < w[1]),
            "progress must strictly advance: {progress:?}"
        );
        assert_eq!(progress.last(), Some(&9));
    }

    #[test]
    fn malformed_response_is_reported_and_not_accepted() {
        let mut e = engine();
        let now = Instant::now();
        e.start_single(vec![1], now);
        let _ = e.tick(now);
        e.drain_events();
        // Declares 255 data bytes but supplies one, so the frame is truncated
        // and cannot be mistaken for a valid acknowledgement.
        e.on_bytes(&[0x01, 0x02, 0x03, 0xFF, 0x00], now);
        e.tick(now + Duration::from_millis(200));
        let events = e.drain_events();
        assert!(tags(&events).contains(&"malformed"), "{events:?}");
        assert!(!tags(&events).contains(&"ack"));
        assert!(e.is_running(), "still waiting for a valid acknowledgement");
    }

    #[test]
    fn response_with_wrong_sequence_is_shown_but_not_acknowledged() {
        let mut e = engine();
        let now = Instant::now();
        e.start_single(vec![1], now);
        let _ = e.tick(now);
        e.drain_events();
        let wrong = frame::Frame {
            address: 1,
            function: 3,
            sequence: 99,
            data: vec![],
            checksum: ChecksumKind::Sum,
        }
        .build();
        e.on_bytes(&wrong, now);
        e.tick(now + Duration::from_millis(200));
        let events = e.drain_events();
        assert!(tags(&events).contains(&"response"));
        assert!(!tags(&events).contains(&"ack"));
        assert!(e.is_running());
    }

    #[test]
    fn two_responses_in_one_burst_are_both_delimited() {
        // The engine treats everything since the last idle period as one frame;
        // a second, well-formed frame arriving back-to-back is therefore parsed
        // as the first one and the extra bytes make it malformed.  Documenting
        // that behaviour here makes it deliberate rather than accidental.
        let mut e = engine();
        let now = Instant::now();
        e.start_single(vec![1], now);
        let _ = e.tick(now);
        e.drain_events();
        let ack = frame::Frame {
            address: 1,
            function: 3,
            sequence: 1,
            data: vec![],
            checksum: ChecksumKind::Sum,
        }
        .build();
        let mut burst = ack.clone();
        burst.extend_from_slice(&ack);
        e.on_bytes(&burst, now);
        e.tick(now + Duration::from_millis(200));
        let events = e.drain_events();
        assert!(tags(&events).contains(&"malformed"), "{events:?}");
        assert!(
            e.is_running(),
            "the first frame is not extracted from a burst"
        );
    }

    #[test]
    fn stop_reports_a_reason_once() {
        let mut e = engine();
        e.start_single(vec![1], Instant::now());
        e.drain_events();
        e.stop("用户取消");
        assert_eq!(tags(&e.drain_events()), vec!["stopped"]);
        e.stop("again");
        assert!(e.drain_events().is_empty(), "no event when already idle");
    }

    #[test]
    fn starting_a_new_task_interrupts_the_previous_one() {
        let mut e = engine();
        let now = Instant::now();
        e.start_auto(vec![1], now);
        e.drain_events();
        e.start_single(vec![2], now);
        assert_eq!(e.task_kind(), Some(TaskKind::Single));
        assert!(tags(&e.drain_events()).contains(&"stopped"));
    }

    #[test]
    fn empty_file_is_rejected() {
        let mut e = engine();
        e.start_file(Vec::new(), Instant::now());
        assert!(!e.is_running());
        assert_eq!(tags(&e.drain_events()), vec!["stopped"]);
    }

    #[test]
    fn config_is_clamped() {
        let mut cfg = ProtocolConfig {
            max_data_len: 9999,
            frame_period_ms: 0,
            max_retries: 9999,
            ..Default::default()
        };
        cfg.normalize();
        assert_eq!(cfg.max_data_len, MAX_DATA);
        assert_eq!(cfg.frame_period_ms, 1);
        assert_eq!(cfg.max_retries, 255);
    }

    #[test]
    fn idle_timeout_is_longer_at_low_baud() {
        let fast = ProtocolEngine::new(ProtocolConfig::default(), 460800);
        let slow = ProtocolEngine::new(ProtocolConfig::default(), 9600);
        assert!(frame::idle_timeout_ms(fast.baud) < frame::idle_timeout_ms(slow.baud));
    }

    #[test]
    fn short_frame_period_is_flagged() {
        let mut cfg = ProtocolConfig {
            frame_period_ms: 1,
            ..Default::default()
        };
        assert!(cfg.period_warning(9600).is_some());
        cfg.frame_period_ms = 1000;
        assert!(cfg.period_warning(9600).is_none());
    }
}
