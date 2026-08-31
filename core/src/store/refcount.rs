use std::collections::HashMap;

use crate::error::ChunkStoreError;

/// Reference count and payload size for one chunk digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRef {
    pub count: u64,
    pub size: u64,
}

/// Reference counts per chunk digest.
#[derive(Debug, Default)]
pub struct RefCount {
    counts: HashMap<String, ChunkRef>,
}

impl RefCount {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, digest: &str, count: u64, size: u64) {
        if count == 0 {
            self.counts.remove(digest);
        } else {
            self.counts
                .insert(digest.to_string(), ChunkRef { count, size });
        }
    }

    /// Increment the refcount, recording `size` the first time the digest is seen.
    pub fn increment(&mut self, digest: &str, size: u64) -> Result<u64, ChunkStoreError> {
        let entry = self
            .counts
            .entry(digest.to_string())
            .or_insert(ChunkRef { count: 0, size });
        if entry.size == 0 {
            entry.size = size;
        } else if size != 0 && entry.size != size {
            return Err(ChunkStoreError::invalid_argument(format!(
                "chunk size mismatch for {digest}: stored {} got {size}",
                entry.size
            )));
        }
        entry.count += 1;
        Ok(entry.count)
    }

    pub fn decrement(&mut self, digest: &str) -> Result<u64, ChunkStoreError> {
        let entry = self
            .counts
            .get_mut(digest)
            .ok_or_else(|| ChunkStoreError::not_found(format!("refcount {digest}")))?;
        entry.count -= 1;
        let remaining = entry.count;
        if remaining == 0 {
            self.counts.remove(digest);
        }
        Ok(remaining)
    }

    pub fn get(&self, digest: &str) -> Option<u64> {
        self.counts.get(digest).map(|r| r.count)
    }

    pub fn size_of(&self, digest: &str) -> Option<u64> {
        self.counts.get(digest).map(|r| r.size)
    }

    pub fn get_ref(&self, digest: &str) -> Option<ChunkRef> {
        self.counts.get(digest).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.counts
            .iter()
            .map(|(digest, meta)| (digest.as_str(), meta.count))
    }

    pub fn iter_refs(&self) -> impl Iterator<Item = (&str, ChunkRef)> {
        self.counts
            .iter()
            .map(|(digest, meta)| (digest.as_str(), *meta))
    }
}
