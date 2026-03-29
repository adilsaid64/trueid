use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use trueid_core::ports::{StoreError, TemplateStore};
use trueid_core::{Embedding, UserId};

/// On-disk format: `{ "templates": [ [...], ... ] }`.
#[derive(Serialize, Deserialize)]
struct TemplateFile {
    templates: Vec<Vec<f32>>,
}

pub struct FileTemplateStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl FileTemplateStore {
    /// Env `TRUEID_TEMPLATE_DIR`, or `$XDG_DATA_HOME/trueid/templates`.
    pub fn open_default() -> Result<Self, StoreError> {
        Self::open(template_dir()?)
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|e| {
            StoreError::Failed(format!(
                "create template dir {}: {e}",
                root.display()
            ))
        })?;
        Ok(Self {
            root,
            lock: Mutex::new(()),
        })
    }

    fn path_for(&self, user: &UserId) -> PathBuf {
        self.root.join(format!("{}.json", user.0))
    }
}

fn template_dir() -> Result<PathBuf, StoreError> {
    if let Ok(dir) = std::env::var("TRUEID_TEMPLATE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
        })
        .ok_or_else(|| StoreError::Failed("HOME unset (or set TRUEID_TEMPLATE_DIR)".into()))?;
    Ok(base.join("trueid/templates"))
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), StoreError> {
    let tmp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| StoreError::Failed(e.to_string()))?;
        file.write_all(contents)
            .map_err(|e| StoreError::Failed(e.to_string()))?;
        file.sync_all().ok();
    }
    #[cfg(not(unix))]
    {
        fs::write(&tmp, contents).map_err(|e| StoreError::Failed(e.to_string()))?;
    }
    fs::rename(&tmp, path).map_err(|e| StoreError::Failed(e.to_string()))?;
    Ok(())
}

impl TemplateStore for FileTemplateStore {
    fn load_all(&self, user: &UserId) -> Result<Option<Vec<Embedding>>, StoreError> {
        let _g = self.lock.lock().map_err(|_| StoreError::Failed("lock poisoned".into()))?;
        let path = self.path_for(user);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).map_err(|e| StoreError::Failed(e.to_string()))?;
        let parsed: TemplateFile =
            serde_json::from_str(&raw).map_err(|e| StoreError::Failed(e.to_string()))?;
        let templates: Vec<Embedding> = parsed.templates.into_iter().map(Embedding).collect();
        if templates.is_empty() {
            return Ok(None);
        }
        Ok(Some(templates))
    }

    fn save_all(&self, user: &UserId, templates: &[Embedding]) -> Result<(), StoreError> {
        let _g = self.lock.lock().map_err(|_| StoreError::Failed("lock poisoned".into()))?;
        let path = self.path_for(user);
        let body = TemplateFile {
            templates: templates.iter().map(|e| e.0.clone()).collect(),
        };
        let json = serde_json::to_vec_pretty(&body).map_err(|e| StoreError::Failed(e.to_string()))?;
        write_atomic(&path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn roundtrip_save_load() {
        let uid = UserId(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos(),
        );
        let dir = std::env::temp_dir().join(format!("trueid-test-{}", uid.0));
        let _ = fs::remove_dir_all(&dir);
        let store = FileTemplateStore::open(&dir).unwrap();
        let emb = Embedding(vec![0.25, 0.5, 0.75]);
        store.save_all(&uid, &[emb.clone()]).unwrap();
        assert_eq!(store.load_all(&uid).unwrap(), Some(vec![emb]));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_template_roundtrip() {
        let uid = UserId(43);
        let dir = std::env::temp_dir().join("trueid-multi-template-test");
        let _ = fs::remove_dir_all(&dir);
        let store = FileTemplateStore::open(&dir).unwrap();
        let a = Embedding(vec![1.0, 0.0]);
        let b = Embedding(vec![0.0, 1.0]);
        store.save_all(&uid, &[a.clone(), b.clone()]).unwrap();
        assert_eq!(store.load_all(&uid).unwrap(), Some(vec![a, b]));
        let _ = fs::remove_dir_all(&dir);
    }
}
