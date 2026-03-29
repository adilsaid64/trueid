use crate::domain::Embedding;

pub trait EmbeddingMatcher: Send + Sync {
    fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool;

    /// Raw score for structured logging (e.g. cosine similarity in \[-1, 1\]). `None` if unknown or invalid.
    fn similarity(&self, _probe: &Embedding, _enrolled: &Embedding) -> Option<f32> {
        None
    }
}
