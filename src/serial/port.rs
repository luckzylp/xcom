//! Port discovery, including USB metadata, on a background thread.
//!
//! Enumerating ports touches the OS (on Linux it walks `/sys`, on Windows it
//! queries the setup API), so it must never happen on the UI thread.  A watcher
//! thread polls and only reports the device list when it actually changes,
//! which is also what gives us hot-plug detection for free.

use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender, TryRecvError};
use serialport::SerialPortType;

/// What kind of bus a port sits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortKind {
    Usb {
        vid: u16,
        pid: u16,
        serial: Option<String>,
        manufacturer: Option<String>,
        product: Option<String>,
    },
    Bluetooth,
    Pci,
    Unknown,
}

/// A discovered port plus the metadata the OS was willing to share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDesc {
    pub name: String,
    pub kind: PortKind,
}

impl PortDesc {
    /// Compact label shown in the port list, e.g. `/dev/ttyUSB0 (CH340)`.
    pub fn display(&self) -> String {
        match &self.kind {
            PortKind::Usb {
                product,
                manufacturer,
                ..
            } => {
                let identity = product
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        manufacturer
                            .as_deref()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                    });
                identity.map_or_else(
                    || self.name.clone(),
                    |identity| format!("{} ({identity})", self.name),
                )
            }
            PortKind::Bluetooth => format!("{} ({})", self.name, crate::i18n::tr("蓝牙")),
            PortKind::Pci => format!("{} (PCI)", self.name),
            PortKind::Unknown => self.name.clone(),
        }
    }

    /// Full metadata for the hover tooltip, if any.
    pub fn detail(&self) -> Option<String> {
        match &self.kind {
            PortKind::Usb {
                vid,
                pid,
                serial,
                manufacturer,
                product,
            } => {
                let mut parts: Vec<String> = Vec::new();
                // Prefer a human readable product name over the bare IDs, but
                // keep the IDs because they are what you need for udev rules.
                let mut identity = String::new();
                if let Some(p) = product.as_ref().filter(|s| !s.trim().is_empty()) {
                    identity.push_str(p.trim());
                }
                if let Some(m) = manufacturer.as_ref().filter(|s| !s.trim().is_empty()) {
                    if identity.is_empty() {
                        identity.push_str(m.trim());
                    } else if !identity.contains(m.trim()) {
                        identity = format!("{} {identity}", m.trim());
                    }
                }
                if !identity.is_empty() {
                    parts.push(identity);
                }
                parts.push(format!("{vid:04x}:{pid:04x}"));
                if let Some(sn) = serial.as_ref().filter(|s| !s.trim().is_empty()) {
                    parts.push(format!("SN:{}", sn.trim()));
                }
                Some(parts.join(" "))
            }
            PortKind::Bluetooth => Some(crate::i18n::tr("蓝牙").to_owned()),
            PortKind::Pci => Some("PCI".to_owned()),
            PortKind::Unknown => None,
        }
    }

    /// True when the port looks like a USB serial adapter.
    pub fn is_usb(&self) -> bool {
        matches!(self.kind, PortKind::Usb { .. })
    }
}

/// Convert a `serialport` enumeration entry into our description type.
fn convert(info: serialport::SerialPortInfo) -> PortDesc {
    let kind = match info.port_type {
        SerialPortType::UsbPort(usb) => PortKind::Usb {
            vid: usb.vid,
            pid: usb.pid,
            serial: usb.serial_number,
            manufacturer: usb.manufacturer,
            product: usb.product,
        },
        SerialPortType::BluetoothPort => PortKind::Bluetooth,
        SerialPortType::PciPort => PortKind::Pci,
        SerialPortType::Unknown => PortKind::Unknown,
    };
    PortDesc {
        name: info.port_name,
        kind,
    }
}

/// Enumerate ports once, sorted so `ttyUSB2` precedes `ttyUSB10`.
pub fn enumerate() -> Vec<PortDesc> {
    let mut ports: Vec<PortDesc> = match serialport::available_ports() {
        Ok(list) => list.into_iter().map(convert).collect(),
        Err(err) => {
            log::warn!("枚举串口失败: {err}");
            Vec::new()
        }
    };
    ports.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    ports.dedup_by(|a, b| a.name == b.name);
    ports
}

/// Compare two port names treating embedded digit runs as numbers, so that
/// `COM4` sorts before `COM10` instead of after it.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na = take_number(&mut ai);
                    let nb = take_number(&mut bi);
                    match na.cmp(&nb) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                } else {
                    match ca.cmp(&cb) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        other => return other,
                    }
                }
            }
        }
    }
}

/// Consume a run of ASCII digits as an integer.
fn take_number(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut value: u64 = 0;
    while let Some(&c) = it.peek() {
        match c.to_digit(10) {
            Some(d) => {
                value = value.saturating_mul(10).saturating_add(d as u64);
                it.next();
            }
            None => break,
        }
    }
    value
}

/// Polls the OS and publishes the port list whenever it changes.
pub struct PortWatcher {
    rx: Receiver<Vec<PortDesc>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    /// Most recent list, kept for callers that poll faster than the watcher.
    latest: Vec<PortDesc>,
}

impl PortWatcher {
    /// Start watching.  The first list is published immediately.
    pub fn spawn(interval: Duration) -> Self {
        let (tx, rx): (Sender<Vec<PortDesc>>, Receiver<Vec<PortDesc>>) = unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);

        let handle = thread::Builder::new()
            .name("port-watcher".to_owned())
            .spawn(move || {
                let mut last: Option<Vec<PortDesc>> = None;
                while !thread_stop.load(AtomicOrdering::Relaxed) {
                    let ports = enumerate();
                    if last.as_ref() != Some(&ports) {
                        if tx.send(ports.clone()).is_err() {
                            break; // Receiver dropped: the app is shutting down.
                        }
                        last = Some(ports);
                    }
                    // Sleep in slices so shutdown is prompt.
                    let mut slept = Duration::ZERO;
                    while slept < interval && !thread_stop.load(AtomicOrdering::Relaxed) {
                        let slice = Duration::from_millis(50).min(interval - slept);
                        thread::sleep(slice);
                        slept += slice;
                    }
                }
            })
            .expect("failed to spawn port watcher thread");

        Self {
            rx,
            stop,
            handle: Some(handle),
            latest: Vec::new(),
        }
    }

    /// Latest known port list.
    pub fn ports(&self) -> &[PortDesc] {
        &self.latest
    }

    /// Absorb any pending update.  Returns `true` when the list changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(list) => {
                    self.latest = list;
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        changed
    }

    /// Look up a port by device name.
    pub fn find(&self, name: &str) -> Option<&PortDesc> {
        self.latest.iter().find(|p| p.name == name)
    }
}

impl Drop for PortWatcher {
    fn drop(&mut self) {
        self.stop.store(true, AtomicOrdering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_sort_orders_numeric_suffixes() {
        let mut names = vec![
            "/dev/ttyUSB10".to_owned(),
            "/dev/ttyUSB2".to_owned(),
            "/dev/ttyUSB1".to_owned(),
            "COM4".to_owned(),
            "COM10".to_owned(),
        ];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            vec![
                "/dev/ttyUSB1",
                "/dev/ttyUSB2",
                "/dev/ttyUSB10",
                "COM4",
                "COM10"
            ]
        );
    }

    #[test]
    fn natural_sort_is_total() {
        assert_eq!(natural_cmp("abc", "abc"), Ordering::Equal);
        assert_eq!(natural_cmp("abc", "abcd"), Ordering::Less);
        assert_eq!(natural_cmp("abcd", "abc"), Ordering::Greater);
    }

    #[test]
    fn usb_detail_prefers_product_name() {
        let desc = PortDesc {
            name: "/dev/ttyUSB0".to_owned(),
            kind: PortKind::Usb {
                vid: 0x1a86,
                pid: 0x7523,
                serial: Some("A1B2".to_owned()),
                manufacturer: Some("QinHeng Electronics".to_owned()),
                product: Some("CH340".to_owned()),
            },
        };
        let detail = desc.detail().unwrap();
        assert!(detail.contains("CH340"), "{detail}");
        assert!(detail.contains("1a86:7523"), "{detail}");
        assert!(detail.contains("SN:A1B2"), "{detail}");
        assert!(desc.display().starts_with("/dev/ttyUSB0"));
    }

    #[test]
    fn usb_port_label_stays_compact() {
        let desc = PortDesc {
            name: "/dev/ttyUSB0".to_owned(),
            kind: PortKind::Usb {
                vid: 0x0403,
                pid: 0x6001,
                serial: Some("FTDI-123456".to_owned()),
                manufacturer: Some("FTDI".to_owned()),
                product: Some("FT232R USB UART".to_owned()),
            },
        };

        assert_eq!(desc.display(), "/dev/ttyUSB0 (FT232R USB UART)");
        let tooltip = desc.detail().unwrap();
        assert!(tooltip.contains("0403:6001"), "{tooltip}");
        assert!(tooltip.contains("SN:FTDI-123456"), "{tooltip}");
    }

    #[test]
    fn detail_survives_missing_metadata() {
        let desc = PortDesc {
            name: "/dev/ttyS0".to_owned(),
            kind: PortKind::Unknown,
        };
        assert_eq!(desc.detail(), None);
        assert_eq!(desc.display(), "/dev/ttyS0");
        assert!(!desc.is_usb());
    }
}
