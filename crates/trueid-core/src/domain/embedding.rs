#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmbeddingSummary {
    pub dim: usize,
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub l2_norm: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(Vec<f32>);

impl Embedding {
    pub fn new(values: Vec<f32>) -> Self {
        Self(values)
    }

    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    pub fn dim(&self) -> usize {
        self.0.len()
    }

    pub fn to_vec(&self) -> Vec<f32> {
        self.0.clone()
    }

    pub fn summary(&self) -> EmbeddingSummary {
        let s = self.as_slice();
        if s.is_empty() {
            return EmbeddingSummary {
                dim: 0,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                l2_norm: 0.0,
            };
        }
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut sum = 0.0f32;
        for &x in s {
            min = min.min(x);
            max = max.max(x);
            sum += x;
        }
        let l2_norm = s.iter().map(|x| x * x).sum::<f32>().sqrt();
        EmbeddingSummary {
            dim: s.len(),
            min,
            max,
            mean: sum / s.len() as f32,
            l2_norm,
        }
    }

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
            for (i, x) in e.as_slice().iter().enumerate() {
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
    fn summary_stats() {
        let e = Embedding::new(vec![0.0, 3.0, 4.0]);
        let s = e.summary();
        assert_eq!(s.dim, 3);
        assert_eq!(s.min, 0.0);
        assert_eq!(s.max, 4.0);
        assert!((s.mean - 7.0 / 3.0).abs() < 1e-5);
        assert!((s.l2_norm - 5.0).abs() < 1e-5);
    }

    #[test]
    fn try_average_mean() {
        let a = Embedding::new(vec![0.0, 4.0]);
        let b = Embedding::new(vec![4.0, 0.0]);
        let m = Embedding::try_average(&[a, b]).unwrap();
        assert_eq!(m.as_slice(), &[2.0, 2.0]);
    }
}
