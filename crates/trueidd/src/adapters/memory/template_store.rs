use std::collections::HashMap;
use std::sync::Mutex;

use trueid_core::ports::{StoreError, TemplateStore};
use trueid_core::{Embedding, UserId};

pub struct MemoryTemplateStore {
    inner: Mutex<HashMap<UserId, Embedding>>,
}

impl MemoryTemplateStore {
    pub fn empty() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl TemplateStore for MemoryTemplateStore {
    fn load(&self, user: &UserId) -> Result<Option<Embedding>, StoreError> {
        Ok(self.inner.lock().unwrap().get(user).cloned())
    }

    fn save(&self, user: &UserId, embedding: &Embedding) -> Result<(), StoreError> {
        self.inner.lock().unwrap().insert(*user, embedding.clone());
        Ok(())
    }
}
