//! Hexagonal adapters for the daemon.
//!
//! - [`inbound`]: driving side (Unix-socket IPC). CLI and PAM are sibling crates.
//! - [`outbound`]: driven side (camera, ONNX, disk, matcher).

pub mod inbound;
pub mod outbound;
