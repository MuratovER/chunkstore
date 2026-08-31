use crate::error::ChunkStoreError;
use crate::store::ChunkBackend;

/// Result of a consistency scan over chunk blobs vs refcounts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FsckReport {
    /// Digests with a refcount but no blob on the backend.
    pub missing_blobs: Vec<String>,
    /// 64-char blob keys on the backend with no refcount (safe to delete).
    pub orphan_blobs: Vec<String>,
    /// Number of orphan blobs deleted when `repair` was true.
    pub repaired: u64,
}

pub(crate) fn is_chunk_digest(name: &str) -> bool {
    name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn collect_orphans<B: ChunkBackend>(
    backend: &B,
    live: &std::collections::HashSet<&str>,
) -> Result<Vec<String>, ChunkStoreError> {
    let Some(keys) = backend.list_keys("")? else {
        return Ok(Vec::new());
    };
    Ok(keys
        .into_iter()
        .filter(|key| is_chunk_digest(key) && !live.contains(key.as_str()))
        .collect())
}
