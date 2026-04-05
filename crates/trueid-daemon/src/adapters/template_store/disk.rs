use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use trueid_core::ports::{StoreError, TemplateStore};
use trueid_core::{Embedding, TemplateBundle, UserId};

/// On-disk JSON: `rgb` and `ir` embedding lists.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFile {
    #[serde(default)]
    rgb: Vec<Vec<f32>>,
    #[serde(default)]
    ir: Vec<Vec<f32>>,
}

impl TemplateFile {
    fn into_bundle(self) -> TemplateBundle {
        TemplateBundle {
            rgb: self.rgb.into_iter().map(Embedding).collect(),
            ir: self.ir.into_iter().map(Embedding).collect(),
        }
    }

    fn from_bundle(bundle: &TemplateBundle) -> Self {
        Self {
            rgb: bundle.rgb.iter().map(|e| e.0.clone()).collect(),
            ir: bundle.ir.iter().map(|e| e.0.clone()).collect(),
        }
    }
}

pub struct FileTemplateStore {
    root: PathBuf,
    lock: Mutex<()>,
}

impl FileTemplateStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|e| {
            StoreError::Failed(format!("create template dir {}: {e}", root.display()))
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
    fn load_all(&self, user: &UserId) -> Result<Option<TemplateBundle>, StoreError> {
        let _g = self
            .lock
            .lock()
            .map_err(|_| StoreError::Failed("lock poisoned".into()))?;
        let path = self.path_for(user);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path).map_err(|e| StoreError::Failed(e.to_string()))?;
        let parsed: TemplateFile =
            serde_json::from_str(&raw).map_err(|e| StoreError::Failed(e.to_string()))?;
        let bundle = parsed.into_bundle();
        if bundle.is_empty() {
            return Ok(None);
        }
        Ok(Some(bundle))
    }

    fn save_all(&self, user: &UserId, bundle: &TemplateBundle) -> Result<(), StoreError> {
        let _g = self
            .lock
            .lock()
            .map_err(|_| StoreError::Failed("lock poisoned".into()))?;
        let path = self.path_for(user);
        let body = TemplateFile::from_bundle(bundle);
        let json =
            serde_json::to_vec_pretty(&body).map_err(|e| StoreError::Failed(e.to_string()))?;
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
        let bundle = TemplateBundle {
            rgb: vec![emb.clone()],
            ir: vec![],
        };
        store.save_all(&uid, &bundle).unwrap();
        assert_eq!(store.load_all(&uid).unwrap(), Some(bundle));
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
        let bundle = TemplateBundle {
            rgb: vec![a.clone(), b.clone()],
            ir: vec![],
        };
        store.save_all(&uid, &bundle).unwrap();
        assert_eq!(store.load_all(&uid).unwrap(), Some(bundle));
        let _ = fs::remove_dir_all(&dir);
    }
}
