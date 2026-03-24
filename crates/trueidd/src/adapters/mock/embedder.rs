use trueid_core::ports::{EmbedError, Embedder};
use trueid_core::{Embedding, Frame};

pub struct MockEmbedder {
    embedding: Embedding,
}

impl MockEmbedder {
    pub fn new(embedding: Embedding) -> Self {
        Self { embedding }
    }
}

impl Embedder for MockEmbedder {
    fn embed(&self, _frame: &Frame) -> Result<Embedding, EmbedError> {
        Ok(self.embedding.clone())
    }
}
