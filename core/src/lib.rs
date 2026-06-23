//! Content-addressed chunk storage core.
//!
//! Files are split into chunks, each addressed by full SHA-256 (64-char hex).
//! Identical bytes share one stored chunk with reference counting and GC on delete.

pub mod chunker;
pub mod error;
pub mod ffi;
pub mod hasher;
pub mod store;

pub use chunker::{chunk_cdc, CdcChunker, CdcConfig, Chunker, FixedChunker};
pub use error::ChunkStoreError;
pub use hasher::{digest_bytes, digest_hex, verify_digest};
pub use store::{ChunkBackend, ChunkStore, FsBackend, MemoryBackend, Stats};
