//! Serial port access: configuration, discovery and the live link.

pub mod config;
pub mod link;
pub mod port;

pub use config::{DataBitsSel, FlowSel, ParitySel, SerialConfig, StopBitsSel};
pub use link::{SerialEvent, SerialLink, Signals};
pub use port::PortWatcher;
