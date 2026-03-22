use std::fs;
use std::path::Path;
use std::sync::Arc;

use trueid_core::TrueIdApp;
use trueid_ipc::SOCKET_PATH;

mod adapters;
mod ipc;

fn main() -> std::io::Result<()> {
    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let health = Arc::new(adapters::DefaultHealth);
    let biometric = Arc::new(adapters::DefaultBiometric);
    let app = Arc::new(TrueIdApp::new(health, biometric));

    ipc::run_unix_socket(SOCKET_PATH, app)
}
