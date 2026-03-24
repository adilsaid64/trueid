use thiserror::Error;

use crate::domain::{Embedding, Frame};

#[derive(Debug, Error)]
pub enum EmbedError {
    #[error("{0}")]
    Failed(String),
}

pub trait Embedder: Send + Sync {
    fn embed(&self, frame: &Frame) -> Result<Embedding, EmbedError>;
}
