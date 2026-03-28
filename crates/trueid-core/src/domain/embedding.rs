#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    pub fn dim(&self) -> usize {
        self.0.len()
    }

    /// Element-wise mean of same-dimension embeddings (e.g. enroll burst).
    pub fn try_average(embeddings: &[Self]) -> Option<Self> {
        if embeddings.is_empty() {
            return None;
        }
        let dim = embeddings[0].dim();
        if dim == 0 || embeddings.iter().any(|e| e.dim() != dim) {
            return None;
        }
        let mut acc = vec![0.0f32; dim];
        for e in embeddings {
            for (i, x) in e.0.iter().enumerate() {
                acc[i] += x;
            }
        }
        let n = embeddings.len() as f32;
        for x in &mut acc {
            *x /= n;
        }
        Some(Self(acc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_average_mean() {
        let a = Embedding(vec![0.0, 4.0]);
        let b = Embedding(vec![4.0, 0.0]);
        let m = Embedding::try_average(&[a, b]).unwrap();
        assert_eq!(m.0, vec![2.0, 2.0]);
    }
}
