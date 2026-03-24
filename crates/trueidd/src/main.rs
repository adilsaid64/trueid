use std::fs;
use std::path::Path;
use std::sync::Arc;

use trueid_core::Embedding;
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
    let video = Arc::new(adapters::MockVideoSource::default_gray());
    let embedder = Arc::new(adapters::MockEmbedder::new(Embedding(vec![
        1.0, 0.0, 0.0,
    ])));
    let template_store = Arc::new(adapters::MemoryTemplateStore::empty());
    let matcher = Arc::new(adapters::CosineMatcher::new(0.99));

    let app = Arc::new(TrueIdApp::new(
        health,
        biometric,
        video,
        embedder,
        template_store,
        matcher,
    ));

    ipc::run_unix_socket(SOCKET_PATH, app)
}
