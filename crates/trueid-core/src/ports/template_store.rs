use thiserror::Error;

use crate::domain::{TemplateBundle, UserId};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("{0}")]
    Failed(String),
}

pub trait TemplateStore: Send + Sync {
    fn load_all(&self, user: &UserId) -> Result<Option<TemplateBundle>, StoreError>;

    fn save_all(&self, user: &UserId, bundle: &TemplateBundle) -> Result<(), StoreError>;
}
