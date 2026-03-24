use crate::domain::Embedding;

pub trait EmbeddingMatcher: Send + Sync {
    fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool;
}
