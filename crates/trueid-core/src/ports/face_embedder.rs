use thiserror::Error;

use crate::domain::{Embedding, Frame};

#[derive(Debug, Error)]
pub enum FaceEmbedError {
    #[error("{0}")]
    Failed(String),
}

pub trait FaceEmbedder: Send + Sync {
    fn embed(&self, frame: &Frame) -> Result<Embedding, FaceEmbedError>;
}
