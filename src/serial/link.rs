//! The live serial connection: open/close, a reader thread, a writer thread and
//! modem control lines.
//!
//! Writes go through a bounded queue owned by a dedicated thread.  That matters
//! because `write_all` on a serial port blocks when hardware flow control is on
//! and the peer has de-asserted CTS; doing that on the UI thread would freeze
//! the window.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use serialport::{ClearBuffer, SerialPort};

use super::config::{OpenPlan, SerialConfig};

/// How long a blocking read waits before looping so it can notice shutdown.
const READ_TIMEOUT: Duration = Duration::from_millis(50);
/// Capacity of the outgoing byte queue.
const WRITE_QUEUE: usize = 512;

/// Something that happened on the link, reported to the UI thread.
#[derive(Debug, Clone)]
pub enum SerialEvent {
    /// Bytes arrived from the device.
    Data(Vec<u8>),
    /// Bytes were handed to the driver.
    Written(usize),
    /// A write failed (the port is likely gone).
    WriteFailed(String),
    /// The link died; the port must be closed.
    Lost(String),
}

/// State of the input handshake lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    pub cts: bool,
    pub dsr: bool,
    pub dcd: bool,
    pub ri: bool,
}

/// An open serial connection.
pub struct SerialLink {
    /// Control handle, kept for modem lines and cloning.
    port: Option<Box<dyn SerialPort>>,
    /// Config the port was opened with.
    config: Option<SerialConfig>,
    /// Warnings produced while opening.
    notes: Vec<String>,
    events_rx: Receiver<SerialEvent>,
    events_tx: Sender<SerialEvent>,
    write_tx: Option<Sender<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl SerialLink {
    /// A closed link.
    pub fn new() -> Self {
        let (events_tx, events_rx) = unbounded();
        Self {
            port: None,
            config: None,
            notes: Vec::new(),
            events_rx,
            events_tx,
            write_tx: None,
            stop: Arc::new(AtomicBool::new(false)),
            reader: None,
            writer: None,
        }
    }

    /// Whether a port is currently open.
    pub fn is_open(&self) -> bool {
        self.port.is_some()
    }

    /// Warnings gathered while opening (e.g. 1.5 stop bits not supported).
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Open `config`.  Any previously open port is closed first.
    pub fn open(&mut self, config: &SerialConfig) -> Result<(), String> {
        if self.is_open() {
            self.close();
        }
        let plan: OpenPlan = config.to_plan();

        let port = serialport::new(config.port.clone(), plan.baud)
            .data_bits(plan.data_bits)
            .stop_bits(plan.stop_bits)
            .parity(plan.parity)
            .flow_control(plan.flow)
            .timeout(READ_TIMEOUT)
            .open()
            .map_err(|err| crate::i18n::tr2("无法打开 {}: {}", &config.port, err))?;

        if let Some(note) = plan.note.clone() {
            self.notes.push(note);
        }

        // Apply the requested initial handshake levels.  Failures here are not
        // fatal: many virtual/USB bridges do not implement every line.
        let mut port = port;
        if let Err(err) = port.write_data_terminal_ready(config.dtr) {
            self.notes.push(crate::i18n::tr1("设置 DTR 失败: {}", err));
        }
        if let Err(err) = port.write_request_to_send(config.rts) {
            self.notes.push(crate::i18n::tr1("设置 RTS 失败: {}", err));
        }

        // Flush anything the driver buffered before we attached.
        let _ = port.clear(ClearBuffer::All);

        let reader_port = port
            .try_clone()
            .map_err(|err| crate::i18n::tr1("无法复制串口句柄: {}", err))?;
        let writer_port = port
            .try_clone()
            .map_err(|err| crate::i18n::tr1("无法复制串口句柄: {}", err))?;

        let stop = Arc::new(AtomicBool::new(false));
        let (write_tx, write_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(WRITE_QUEUE);

        self.reader = Some(spawn_reader(
            reader_port,
            self.events_tx.clone(),
            Arc::clone(&stop),
        ));
        self.writer = Some(spawn_writer(
            writer_port,
            write_rx,
            self.events_tx.clone(),
            Arc::clone(&stop),
        ));

        self.port = Some(port);
        self.config = Some(config.clone());
        self.write_tx = Some(write_tx);
        self.stop = stop;
        Ok(())
    }

    /// Close the port and stop both worker threads.
    pub fn close(&mut self) {
        if !self.is_open() && self.reader.is_none() {
            return;
        }
        // Dropping the control handle first means the modem-line calls the UI
        // might still issue have nothing to talk to.
        self.port = None;
        self.config = None;

        self.stop.store(true, Ordering::Relaxed);
        // Dropping the sender terminates the writer loop.
        self.write_tx = None;

        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.writer.take() {
            let _ = handle.join();
        }

        // Drain stale events so a reopened port does not inherit them.
        while self.events_rx.try_recv().is_ok() {}
        self.notes.clear();
    }

    /// Queue bytes for transmission.  Never blocks.
    ///
    /// Returns `Err` when the queue is saturated, which the caller should treat
    /// as back-pressure rather than a hard failure.
    pub fn send(&self, bytes: Vec<u8>) -> Result<(), String> {
        let Some(tx) = self.write_tx.as_ref() else {
            return Err(crate::i18n::tr("串口未打开").to_owned());
        };
        if bytes.is_empty() {
            return Ok(());
        }
        match tx.try_send(bytes) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                Err(crate::i18n::tr("发送队列已满，请降低发送速率").to_owned())
            }
            Err(TrySendError::Disconnected(_)) => Err(crate::i18n::tr("串口已关闭").to_owned()),
        }
    }

    /// Non-blocking read of pending link events.
    pub fn poll(&self) -> Vec<SerialEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.events_rx.try_recv() {
            out.push(event);
        }
        out
    }

    /// Sample the handshake input lines.
    ///
    /// Returns `None` when no port is open.  Individual lines that the driver
    /// cannot report read back as `false` rather than failing the whole sample.
    pub fn signals(&mut self) -> Option<Signals> {
        let port = self.port.as_mut()?;
        Some(Signals {
            cts: port.read_clear_to_send().unwrap_or(false),
            dsr: port.read_data_set_ready().unwrap_or(false),
            dcd: port.read_carrier_detect().unwrap_or(false),
            ri: port.read_ring_indicator().unwrap_or(false),
        })
    }

    /// Drive DTR.
    pub fn set_dtr(&mut self, level: bool) -> Result<(), String> {
        let port = self
            .port
            .as_mut()
            .ok_or_else(|| crate::i18n::tr("串口未打开").to_owned())?;
        port.write_data_terminal_ready(level)
            .map_err(|e| e.to_string())
    }

    /// Drive RTS.
    pub fn set_rts(&mut self, level: bool) -> Result<(), String> {
        let port = self
            .port
            .as_mut()
            .ok_or_else(|| crate::i18n::tr("串口未打开").to_owned())?;
        port.write_request_to_send(level).map_err(|e| e.to_string())
    }
}

impl Default for SerialLink {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SerialLink {
    fn drop(&mut self) {
        self.close();
    }
}

fn spawn_reader(
    mut port: Box<dyn SerialPort>,
    events: Sender<SerialEvent>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("serial-reader".to_owned())
        .spawn(move || {
            let mut buf = vec![0u8; 8192];
            while !stop.load(Ordering::Relaxed) {
                match port.read(&mut buf) {
                    Ok(0) => {
                        // No progress; a zero-length read on a serial port is
                        // not EOF, it just means the driver had nothing.
                        thread::sleep(Duration::from_millis(5));
                    }
                    Ok(n) => {
                        if events.send(SerialEvent::Data(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        if is_timeout(&err) {
                            continue;
                        }
                        // Anything else means the device went away.
                        let _ = events
                            .send(SerialEvent::Lost(crate::i18n::tr1("串口读取失败: {}", err)));
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn serial reader thread")
}

fn spawn_writer(
    mut port: Box<dyn SerialPort>,
    queue: Receiver<Vec<u8>>,
    events: Sender<SerialEvent>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("serial-writer".to_owned())
        .spawn(move || {
            // `recv` returns Err once every sender is dropped, which is how we
            // learn that the link was closed.
            while let Ok(chunk) = queue.recv() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let len = chunk.len();
                match port.write_all(&chunk) {
                    Ok(()) => {
                        let _ = port.flush();
                        if events.send(SerialEvent::Written(len)).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = events.send(SerialEvent::WriteFailed(crate::i18n::tr1(
                            "发送失败: {}",
                            &err,
                        )));
                        if is_fatal(&err) {
                            break;
                        }
                    }
                }
            }
        })
        .expect("failed to spawn serial writer thread")
}

/// Whether a read/write error is just the poll timeout expiring.
///
/// `SerialPort::read` and `Write` report plain `std::io::Error`, so this works
/// on the IO error kind rather than the `serialport` wrapper type.
fn is_timeout(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

/// Whether an error means the port is unusable.
fn is_fatal(err: &io::Error) -> bool {
    !matches!(
        err.kind(),
        io::ErrorKind::TimedOut
            | io::ErrorKind::WouldBlock
            | io::ErrorKind::Interrupted
            | io::ErrorKind::BrokenPipe
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_link_is_closed() {
        let mut link = SerialLink::new();
        assert!(!link.is_open());
        assert!(link.signals().is_none());
        assert!(link.send(vec![1, 2, 3]).is_err());
        assert!(link.poll().is_empty());
    }

    #[test]
    fn opening_a_missing_port_reports_an_error() {
        let mut link = SerialLink::new();
        let config = SerialConfig {
            port: "/dev/definitely-not-a-real-port-xyz".to_owned(),
            ..Default::default()
        };
        let err = link.open(&config).unwrap_err();
        assert!(err.contains("无法打开"), "{err}");
        assert!(!link.is_open());
    }

    #[test]
    fn close_is_idempotent() {
        let mut link = SerialLink::new();
        link.close();
        link.close();
        assert!(!link.is_open());
    }
}

/// End-to-end tests against a real pseudo-terminal.
///
/// These exercise the whole path — opening a device, the reader thread, the
/// writer thread and the event channel — rather than mocking the driver, which
/// is where the interesting failure modes live.
#[cfg(all(test, unix))]
mod loopback_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::fd::FromRawFd;
    use std::time::Instant;

    /// Serialises PTY allocation.
    ///
    /// `ptsname(3)` returns a pointer into a *static* buffer, so two threads
    /// calling it concurrently can observe each other's result and end up with
    /// the same slave path.  That is not hypothetical: without this lock the
    /// parallel test run reproducibly failed with "unable to acquire exclusive
    /// lock" because two links were opening the same tty.  `openpty` itself is
    /// thread-safe; only the name lookup needs protecting.
    static PTY_ALLOC: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A pseudo-terminal pair: reads and writes go to `master`.
    struct Pty {
        master: std::fs::File,
        slave_path: String,
    }

    impl Pty {
        fn open() -> Option<Self> {
            // Held only while the descriptors and name are resolved.
            let _guard = PTY_ALLOC.lock().unwrap_or_else(|e| e.into_inner());

            let mut master: libc::c_int = -1;
            let mut slave: libc::c_int = -1;
            // SAFETY: openpty writes two valid descriptors on success, which we
            // check via the return code before using them.
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
            // SAFETY: `master` is a valid descriptor we now exclusively own.
            let master_file = unsafe { std::fs::File::from_raw_fd(master) };
            // SAFETY: `slave` is a valid descriptor we now exclusively own.
            let slave_file = unsafe { std::fs::File::from_raw_fd(slave) };

            // SAFETY: `master` is still open at this point.
            let name = unsafe { libc::ptsname(master) };
            if name.is_null() {
                return None;
            }
            // SAFETY: ptsname returns a NUL-terminated string valid until the
            // next ptsname call on this descriptor.
            let slave_path = unsafe { std::ffi::CStr::from_ptr(name) }
                .to_string_lossy()
                .into_owned();

            // Non-blocking reads let `read_available` poll without hanging if
            // the device under test never answers.
            // SAFETY: `master` is a valid descriptor.
            unsafe {
                let flags = libc::fcntl(master, libc::F_GETFL);
                if flags >= 0 {
                    libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }

            // Drop the slave handle so the link under test is the only opener,
            // which is exactly the situation on a real machine.
            drop(slave_file);
            Some(Self {
                master: master_file,
                slave_path,
            })
        }

        /// Read whatever is available, without blocking indefinitely.
        fn read_available(&mut self, want: usize, timeout: Duration) -> Vec<u8> {
            let deadline = Instant::now() + timeout;
            let mut out = Vec::new();
            let mut buf = [0u8; 256];
            while Instant::now() < deadline && out.len() < want {
                match self.master.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => out.extend_from_slice(&buf[..n]),
                    Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(ref err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
                thread::sleep(Duration::from_millis(5));
            }
            out
        }
    }

    /// Collect received bytes from the link's event channel.
    fn drain_data(link: &SerialLink, want: usize, timeout: Duration) -> Vec<u8> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        while Instant::now() < deadline && out.len() < want {
            for event in link.poll() {
                if let SerialEvent::Data(bytes) = event {
                    out.extend_from_slice(&bytes);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        out
    }

    fn config_for(port: &str) -> SerialConfig {
        SerialConfig {
            port: port.to_owned(),
            baud: 115200,
            ..Default::default()
        }
    }

    #[test]
    fn opens_a_real_tty_and_reports_it() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        link.open(&config_for(&pty.slave_path))
            .expect("opening a pty slave should succeed");
        assert!(link.is_open());
        // DTR/RTS may be unsupported on a pty; either way it must not be fatal
        // and any complaint must be surfaced as a note rather than an error.
        let _notes = link.take_notes();
        link.close();
        assert!(!link.is_open());
    }

    #[test]
    fn receives_bytes_written_to_the_peer() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        link.open(&config_for(&pty.slave_path)).expect("open");

        let mut pty = pty;
        pty.master.write_all(b"AT+GMR\r\n").expect("write to peer");
        pty.master.flush().expect("flush");

        let got = drain_data(&link, 8, Duration::from_secs(3));
        assert_eq!(got, b"AT+GMR\r\n", "received {got:02X?}");
    }

    #[test]
    fn transmits_bytes_to_the_peer() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        link.open(&config_for(&pty.slave_path)).expect("open");

        link.send(b"OK".to_vec()).expect("queue bytes");

        let mut pty = pty;
        let got = pty.read_available(2, Duration::from_secs(3));
        assert_eq!(got, b"OK", "peer received {got:02X?}");
    }

    #[test]
    fn reports_written_byte_counts() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        link.open(&config_for(&pty.slave_path)).expect("open");
        link.send(vec![0u8; 64]).expect("queue bytes");

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut written = 0usize;
        while Instant::now() < deadline && written < 64 {
            for event in link.poll() {
                if let SerialEvent::Written(count) = event {
                    written += count;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(written, 64, "the writer thread must account for every byte");
    }

    #[test]
    fn a_closed_link_accepts_nothing_further() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        link.open(&config_for(&pty.slave_path)).expect("open");
        link.close();
        assert!(link.send(vec![1]).is_err());
    }

    #[test]
    fn reopening_the_same_port_works() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        for round in 0..3 {
            link.open(&config_for(&pty.slave_path))
                .unwrap_or_else(|err| panic!("round {round}: {err}"));
            assert!(link.is_open(), "round {round}");
            link.send(vec![round as u8]).expect("send");
            link.close();
        }
    }

    #[test]
    fn handshake_lines_do_not_wedge_the_link() {
        let Some(pty) = Pty::open() else {
            eprintln!("skipping: openpty unavailable in this environment");
            return;
        };
        let mut link = SerialLink::new();
        link.open(&config_for(&pty.slave_path)).expect("open");
        // A pty may reject these with ENOTTY; the call must return rather than
        // fail hard, and the link must stay usable either way.
        let _ = link.set_dtr(false);
        let _ = link.set_rts(false);
        let _ = link.signals();
        link.send(b"still alive".to_vec()).expect("send");
    }
}
