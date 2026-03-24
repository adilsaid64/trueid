use thiserror::Error;

use crate::domain::{Embedding, UserId};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Failed(String),
}

pub trait TemplateStore: Send + Sync {
    fn load(&self, user: &UserId) -> Result<Option<Embedding>, StoreError>;

    fn save(&self, user: &UserId, embedding: &Embedding) -> Result<(), StoreError>;
}
