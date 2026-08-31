use crate::error::ChunkStoreError;

/// Optional transform applied to **blob bytes** on put/get.
///
/// The content digest remains SHA-256 of the plaintext so deduplication is
/// unchanged. Implementations must be deterministic: `decode(encode(p)) == p`.
pub trait ChunkCodec: Send + Sync {
    fn encode(&self, plaintext: &[u8]) -> Result<Vec<u8>, ChunkStoreError>;
    fn decode(&self, stored: &[u8]) -> Result<Vec<u8>, ChunkStoreError>;
}

/// No-op codec (store plaintext). Prefer `None` on [`super::ChunkStore`] to skip copies.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityCodec;

impl ChunkCodec for IdentityCodec {
    fn encode(&self, plaintext: &[u8]) -> Result<Vec<u8>, ChunkStoreError> {
        Ok(plaintext.to_vec())
    }

    fn decode(&self, stored: &[u8]) -> Result<Vec<u8>, ChunkStoreError> {
        Ok(stored.to_vec())
    }
}

impl std::fmt::Debug for dyn ChunkCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("dyn ChunkCodec")
    }
}
