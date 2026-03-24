#[derive(Debug, Clone, PartialEq)]
pub struct Embedding(pub Vec<f32>);

impl Embedding {
    pub fn as_slice(&self) -> &[f32] {
        &self.0
    }

    pub fn dim(&self) -> usize {
        self.0.len()
    }
}
