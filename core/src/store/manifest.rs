use std::collections::HashMap;

use crate::error::ChunkStoreError;

/// Maps file IDs to ordered chunk digests.
#[derive(Debug, Default)]
pub struct Manifest {
    files: HashMap<String, Vec<String>>,
    file_bytes: HashMap<String, u64>,
}

impl Manifest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_get(&self, file_id: &str) -> Option<&[String]> {
        self.files.get(file_id).map(Vec::as_slice)
    }

    pub fn insert(
        &mut self,
        file_id: &str,
        digests: Vec<String>,
        file_bytes: u64,
    ) -> Result<(), ChunkStoreError> {
        if file_id.is_empty() {
            return Err(ChunkStoreError::invalid_argument("file_id cannot be empty"));
        }
        self.files.insert(file_id.to_string(), digests);
        self.file_bytes.insert(file_id.to_string(), file_bytes);
        Ok(())
    }

    pub fn get(&self, file_id: &str) -> Result<&[String], ChunkStoreError> {
        self.files
            .get(file_id)
            .map(Vec::as_slice)
            .ok_or_else(|| ChunkStoreError::not_found(format!("file {file_id}")))
    }

    pub fn remove(&mut self, file_id: &str) -> Result<Vec<String>, ChunkStoreError> {
        self.file_bytes.remove(file_id);
        self.files
            .remove(file_id)
            .ok_or_else(|| ChunkStoreError::not_found(format!("file {file_id}")))
    }
}
