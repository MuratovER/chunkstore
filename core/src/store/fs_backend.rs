use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::ChunkStoreError;
use crate::store::ChunkBackend;

static WRITE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Filesystem-backed chunk and metadata storage.
#[derive(Debug, Clone)]
pub struct FsBackend {
    root: PathBuf,
}

impl FsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, ChunkStoreError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn key_to_path(&self, key: &str) -> Result<PathBuf, ChunkStoreError> {
        if key.is_empty() {
            return Err(ChunkStoreError::invalid_argument("key cannot be empty"));
        }
        if key.contains('\\') {
            return Err(ChunkStoreError::invalid_argument(
                "key cannot contain backslashes",
            ));
        }

        let rel = Path::new(key);
        for component in rel.components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir => {}
                _ => {
                    return Err(ChunkStoreError::invalid_argument(format!(
                        "invalid key path component in {key}"
                    )));
                }
            }
        }

        let path = self.root.join(rel);
        if !path.starts_with(&self.root) {
            return Err(ChunkStoreError::invalid_argument(format!(
                "key escapes backend root: {key}"
            )));
        }
        Ok(path)
    }

    fn write_atomic(path: &Path, data: &[u8]) -> Result<(), ChunkStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file_name = path
            .file_name()
            .ok_or_else(|| ChunkStoreError::invalid_argument("path has no file name".to_string()))?
            .to_string_lossy();
        let unique = WRITE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = path.with_file_name(format!("{file_name}.{unique}.tmp"));
        {
            let mut file = fs::File::create(&tmp)?;
            file.write_all(data)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Count chunk blobs (64-char hex files at repository root).
    pub fn chunk_blob_count(&self) -> Result<usize, ChunkStoreError> {
        let mut count = 0;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_chunk_digest(&name) {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn chunk_exists(&self, digest: &str) -> Result<bool, ChunkStoreError> {
        Ok(self.key_to_path(digest)?.is_file())
    }
}

fn is_chunk_digest(name: &str) -> bool {
    name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit())
}

impl ChunkBackend for FsBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ChunkStoreError> {
        let path = self.key_to_path(key)?;
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(fs::read(path)?))
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<(), ChunkStoreError> {
        let path = self.key_to_path(key)?;
        Self::write_atomic(&path, data)
    }

    fn exists(&self, key: &str) -> Result<bool, ChunkStoreError> {
        Ok(self.key_to_path(key)?.exists())
    }

    fn delete(&self, key: &str) -> Result<(), ChunkStoreError> {
        let path = self.key_to_path(key)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_traversal_keys() {
        let dir = tempdir().unwrap();
        let backend = FsBackend::new(dir.path()).unwrap();
        assert!(backend.put("../escape", b"x").is_err());
    }

    #[test]
    fn roundtrip_chunk_blob() {
        let dir = tempdir().unwrap();
        let backend = FsBackend::new(dir.path()).unwrap();
        let digest = "a".repeat(64);
        backend.put(&digest, b"chunk-bytes").unwrap();
        assert!(backend.exists(&digest).unwrap());
        assert_eq!(backend.get(&digest).unwrap(), Some(b"chunk-bytes".to_vec()));
        backend.delete(&digest).unwrap();
        assert!(!backend.exists(&digest).unwrap());
    }
}
