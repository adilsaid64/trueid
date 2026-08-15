//! TrueID daemon: composition root plus adapters around `trueid-core`.
//!
//! Load config, wire outbound adapters into [`trueid_core::TrueIdApp`], then
//! serve the inbound Unix-socket adapter.

use std::fs;
use std::path::Path;

use trueid_ipc::SOCKET_PATH;

mod adapters;
mod composition;
mod config;

fn init_tracing(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new(level))
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
}

fn main() -> std::io::Result<()> {
    let cfg = config::load_config()?;
    init_tracing(&cfg.logging.level);

    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let app = composition::build(&cfg)?;
    adapters::inbound::run_unix_socket(SOCKET_PATH, app)
}
