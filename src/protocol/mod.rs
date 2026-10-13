//! The 协议传输 (protocol transmission) feature: the Modbus-RTU-derived
//! Send-Ack framing, its checksums, and the transfer state machine.

pub mod checksum;
pub mod engine;
pub mod frame;

pub use checksum::ChecksumKind;
pub use engine::{ProtocolConfig, ProtocolEngine, ProtocolEvent, TaskKind};
pub use frame::ReceivedFrame;
