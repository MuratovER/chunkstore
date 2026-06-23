use sha2::{Digest, Sha256};

use crate::error::ChunkStoreError;

/// Compute SHA-256 digest of `data` as a 64-character lowercase hex string.
pub fn digest_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

/// Compute SHA-256 digest of `data` as raw 32 bytes.
pub fn digest_bytes(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Verify that `data` matches the expected hex digest.
pub fn verify_digest(data: &[u8], expected_hex: &str) -> Result<(), ChunkStoreError> {
    let actual = digest_hex(data);
    if actual == expected_hex {
        Ok(())
    } else {
        Err(ChunkStoreError::DigestMismatch {
            expected: expected_hex.to_string(),
            actual,
        })
    }
}

mod hex {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let bytes = bytes.as_ref();
        let mut out = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_64_char_hex() {
        let d = digest_hex(b"hello");
        assert_eq!(d.len(), 64);
        assert!(d.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn verify_accepts_matching_digest() {
        let d = digest_hex(b"chunk");
        assert!(verify_digest(b"chunk", &d).is_ok());
    }

    #[test]
    fn verify_rejects_mismatch() {
        let err = verify_digest(b"wrong", "00".repeat(32).as_str()).unwrap_err();
        assert!(matches!(err, ChunkStoreError::DigestMismatch { .. }));
    }
}
