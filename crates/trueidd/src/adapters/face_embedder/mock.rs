use trueid_core::ports::{FaceEmbedError, FaceEmbedder};
use trueid_core::{Embedding, Frame};

pub struct MockFaceEmbedder {
    embedding: Embedding,
}

impl MockFaceEmbedder {
    pub fn new(embedding: Embedding) -> Self {
        Self { embedding }
    }
}

impl FaceEmbedder for MockFaceEmbedder {
    fn embed(&self, _frame: &Frame) -> Result<Embedding, FaceEmbedError> {
        Ok(self.embedding.clone())
    }
}
