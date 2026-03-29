use thiserror::Error;

use crate::domain::{Embedding, UserId};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Failed(String),
}

pub trait TemplateStore: Send + Sync {
    /// All stored templates for the user, or `None` if there is no enrollment.
    fn load_all(&self, user: &UserId) -> Result<Option<Vec<Embedding>>, StoreError>;

    /// Replace the full template list for the user.
    fn save_all(&self, user: &UserId, templates: &[Embedding]) -> Result<(), StoreError>;
}
