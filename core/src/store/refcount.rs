use std::collections::HashMap;

use crate::error::ChunkStoreError;

/// Reference counts per chunk digest.
#[derive(Debug, Default)]
pub struct RefCount {
    counts: HashMap<String, u64>,
}

impl RefCount {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, digest: &str, count: u64) {
        if count == 0 {
            self.counts.remove(digest);
        } else {
            self.counts.insert(digest.to_string(), count);
        }
    }

    pub fn increment(&mut self, digest: &str) -> Result<u64, ChunkStoreError> {
        let entry = self.counts.entry(digest.to_string()).or_insert(0);
        *entry += 1;
        Ok(*entry)
    }

    pub fn decrement(&mut self, digest: &str) -> Result<u64, ChunkStoreError> {
        let count = self
            .counts
            .get_mut(digest)
            .ok_or_else(|| ChunkStoreError::not_found(format!("refcount {digest}")))?;
        *count -= 1;
        let remaining = *count;
        if remaining == 0 {
            self.counts.remove(digest);
        }
        Ok(remaining)
    }

    pub fn get(&self, digest: &str) -> Option<u64> {
        self.counts.get(digest).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.counts
            .iter()
            .map(|(digest, count)| (digest.as_str(), *count))
    }
}
