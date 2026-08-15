//! Driving adapters: clients call `TrueIdApp` through this Unix-socket server.
//!
//! CLI (`trueid-ctl`) and PAM (`trueid-pam`) live in their own crates and speak
//! the same protocol (`trueid-ipc`).

mod ipc;

pub use ipc::run_unix_socket;
