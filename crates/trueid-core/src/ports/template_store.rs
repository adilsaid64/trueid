use thiserror::Error;

use crate::domain::{TemplateBundle, UserId};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Failed(String),
}

pub trait TemplateStore: Send + Sync {
    /// All stored templates for the user, or `None` if there is no enrollment file.
    fn load_all(&self, user: &UserId) -> Result<Option<TemplateBundle>, StoreError>;

    /// Replace the full template bundle for the user.
    fn save_all(&self, user: &UserId, bundle: &TemplateBundle) -> Result<(), StoreError>;
}
