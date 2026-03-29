use trueid_core::ports::EmbeddingMatcher;
use trueid_core::Embedding;

pub struct CosineMatcher {
    threshold: f32,
}

impl CosineMatcher {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return None;
    }
    Some(dot / (na * nb))
}

impl EmbeddingMatcher for CosineMatcher {
    fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool {
        self.similarity(probe, enrolled)
            .is_some_and(|s| s >= self.threshold)
    }

    fn similarity(&self, probe: &Embedding, enrolled: &Embedding) -> Option<f32> {
        cosine_similarity(probe.as_slice(), enrolled.as_slice())
    }
}
