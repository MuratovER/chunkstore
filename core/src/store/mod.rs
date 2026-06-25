mod fs_backend;
mod manifest;
mod persistence;
mod refcount;
mod stats;

pub use fs_backend::FsBackend;
pub use manifest::Manifest;
pub use refcount::RefCount;
pub use stats::Stats;

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use crate::chunker::{chunk_reader, CdcChunker, Chunker, FixedChunker};
use crate::error::ChunkStoreError;
use crate::hasher::{digest_hex, verify_digest};
use crate::store::persistence::{
    load_manifests, load_refcounts, remove_manifest, remove_refcount, save_manifest, save_refcount,
};

/// Key-value backend for chunk blobs and metadata persistence.
pub trait ChunkBackend: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ChunkStoreError>;
    fn put(&self, key: &str, data: &[u8]) -> Result<(), ChunkStoreError>;
    fn exists(&self, key: &str) -> Result<bool, ChunkStoreError>;
    fn delete(&self, key: &str) -> Result<(), ChunkStoreError>;
}

impl<B: ChunkBackend> ChunkBackend for Arc<B> {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ChunkStoreError> {
        self.as_ref().get(key)
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<(), ChunkStoreError> {
        self.as_ref().put(key, data)
    }

    fn exists(&self, key: &str) -> Result<bool, ChunkStoreError> {
        self.as_ref().exists(key)
    }

    fn delete(&self, key: &str) -> Result<(), ChunkStoreError> {
        self.as_ref().delete(key)
    }
}

/// In-memory backend for unit tests.
#[derive(Debug, Default)]
pub struct MemoryBackend {
    data: Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored keys (chunks + metadata).
    pub fn key_count(&self) -> Result<usize, ChunkStoreError> {
        Ok(self
            .data
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?
            .len())
    }

    pub fn contains_key(&self, key: &str) -> Result<bool, ChunkStoreError> {
        self.exists(key)
    }
}

impl ChunkBackend for MemoryBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ChunkStoreError> {
        Ok(self
            .data
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?
            .get(key)
            .cloned())
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<(), ChunkStoreError> {
        self.data
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?
            .insert(key.to_string(), data.to_vec());
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, ChunkStoreError> {
        Ok(self
            .data
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?
            .contains_key(key))
    }

    fn delete(&self, key: &str) -> Result<(), ChunkStoreError> {
        self.data
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?
            .remove(key);
        Ok(())
    }
}

/// Content-addressed store with manifests, refcounts, and GC.
pub struct ChunkStore<B: ChunkBackend> {
    backend: B,
    manifest: Mutex<Manifest>,
    refcount: Mutex<RefCount>,
    stats: Mutex<Stats>,
    /// Serializes chunk blob exists/put to avoid lost-update races on FS backends.
    blob: Mutex<()>,
}

impl<B: ChunkBackend> ChunkStore<B> {
    /// Open a store and load metadata (`_manifest/`, `_refcount/`) from the backend.
    pub fn open(backend: B) -> Result<Self, ChunkStoreError> {
        let manifest = load_manifests(&backend)?;
        let refcount = load_refcounts(&backend)?;
        let store = Self {
            backend,
            manifest: Mutex::new(manifest),
            refcount: Mutex::new(refcount),
            stats: Mutex::new(Stats::default()),
            blob: Mutex::new(()),
        };
        store.rebuild_stats()?;
        Ok(store)
    }

    pub fn ingest(&self, file_id: &str, data: &[u8]) -> Result<Vec<String>, ChunkStoreError> {
        self.ingest_fixed(file_id, data, FixedChunker::default_chunk_size())
    }

    pub fn ingest_fixed(
        &self,
        file_id: &str,
        data: &[u8],
        chunk_size: usize,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut chunker = FixedChunker::new(chunk_size);
        self.ingest_with_chunker(file_id, data, &mut chunker)
    }

    pub fn ingest_cdc(&self, file_id: &str, data: &[u8]) -> Result<Vec<String>, ChunkStoreError> {
        let mut chunker = CdcChunker::with_defaults()?;
        self.ingest_with_chunker(file_id, data, &mut chunker)
    }

    pub fn ingest_reader_fixed<R: Read>(
        &self,
        file_id: &str,
        reader: &mut R,
        chunk_size: usize,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut chunker = FixedChunker::new(chunk_size);
        self.ingest_reader_with_chunker(file_id, reader, &mut chunker)
    }

    pub fn ingest_reader_cdc<R: Read>(
        &self,
        file_id: &str,
        reader: &mut R,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut chunker = CdcChunker::with_defaults()?;
        self.ingest_reader_with_chunker(file_id, reader, &mut chunker)
    }

    fn ingest_reader_with_chunker<R: Read>(
        &self,
        file_id: &str,
        reader: &mut R,
        chunker: &mut dyn Chunker,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let chunks = chunk_reader(reader, chunker)?;
        self.ingest_chunks(file_id, &chunks)
    }

    fn ingest_with_chunker(
        &self,
        file_id: &str,
        data: &[u8],
        chunker: &mut dyn Chunker,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let chunks = chunker.feed(data)?;
        let mut tail = chunker.finish()?;
        let mut all_chunks = chunks;
        all_chunks.append(&mut tail);
        self.ingest_chunks(file_id, &all_chunks)
    }

    pub fn ingest_chunks(
        &self,
        file_id: &str,
        chunks: &[Vec<u8>],
    ) -> Result<Vec<String>, ChunkStoreError> {
        let old_digests = {
            let manifest = self
                .manifest
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            manifest.try_get(file_id).map(<[String]>::to_vec)
        };
        if let Some(old) = old_digests {
            self.release_digests(&old)?;
            remove_manifest(&self.backend, file_id)?;
        }

        let mut digests = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let digest = digest_hex(chunk);
            self.store_chunk(&digest, chunk)?;
            digests.push(digest);
        }

        let file_bytes: u64 = chunks.iter().map(|c| c.len() as u64).sum();

        {
            let mut manifest = self
                .manifest
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            manifest.insert(file_id, digests.clone(), file_bytes)?;
        }
        save_manifest(&self.backend, file_id, &digests, file_bytes)?;

        Ok(digests)
    }

    fn store_chunk(&self, digest: &str, data: &[u8]) -> Result<(), ChunkStoreError> {
        let size = data.len() as u64;

        let exists = {
            let _blob = self.blob.lock().map_err(|_| ChunkStoreError::LockError)?;
            let exists = self.backend.exists(digest)?;
            if !exists {
                self.backend.put(digest, data)?;
            }
            exists
        };

        {
            let mut refcount = self
                .refcount
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            let count = refcount.increment(digest)?;
            save_refcount(&self.backend, digest, count)?;
        }

        let mut stats = self.stats.lock().map_err(|_| ChunkStoreError::LockError)?;
        if exists {
            stats.add_reference(size);
        } else {
            stats.add_unique_chunk(size);
        }

        Ok(())
    }

    /// Ordered chunk digests for a stored file.
    pub fn file_digests(&self, file_id: &str) -> Result<Vec<String>, ChunkStoreError> {
        self.digests_for_file(file_id)
    }

    /// Fetch and verify a single chunk payload by digest.
    pub fn read_chunk(&self, digest: &str) -> Result<Vec<u8>, ChunkStoreError> {
        self.read_chunk_by_digest(digest)
    }

    fn digests_for_file(&self, file_id: &str) -> Result<Vec<String>, ChunkStoreError> {
        let manifest = self
            .manifest
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?;
        manifest.get(file_id).map(<[String]>::to_vec)
    }

    fn read_chunk_by_digest(&self, digest: &str) -> Result<Vec<u8>, ChunkStoreError> {
        let chunk = self
            .backend
            .get(digest)?
            .ok_or_else(|| ChunkStoreError::not_found(format!("chunk {digest}")))?;
        verify_digest(&chunk, digest)?;
        Ok(chunk)
    }

    /// Stream verified chunk payloads to `writer` without assembling the full file in memory.
    pub fn read_to_writer<W: Write>(
        &self,
        file_id: &str,
        writer: &mut W,
    ) -> Result<(), ChunkStoreError> {
        for digest in self.digests_for_file(file_id)? {
            let chunk = self.read_chunk_by_digest(&digest)?;
            writer.write_all(&chunk)?;
        }
        Ok(())
    }

    pub fn read(&self, file_id: &str) -> Result<Vec<u8>, ChunkStoreError> {
        let mut out = Vec::new();
        self.read_to_writer(file_id, &mut out)?;
        Ok(out)
    }

    pub fn delete(&self, file_id: &str) -> Result<(), ChunkStoreError> {
        let digests = {
            let mut manifest = self
                .manifest
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            manifest.remove(file_id)?
        };
        remove_manifest(&self.backend, file_id)?;
        self.release_digests(&digests)?;
        Ok(())
    }

    fn release_digests(&self, digests: &[String]) -> Result<(), ChunkStoreError> {
        for digest in digests {
            let size = self.chunk_size(digest)?;
            let remaining = {
                let mut refcount = self
                    .refcount
                    .lock()
                    .map_err(|_| ChunkStoreError::LockError)?;
                refcount.decrement(digest)?
            };

            if remaining == 0 {
                self.backend.delete(digest)?;
                remove_refcount(&self.backend, digest)?;
                self.stats
                    .lock()
                    .map_err(|_| ChunkStoreError::LockError)?
                    .remove_unique_chunk(size);
            } else {
                save_refcount(&self.backend, digest, remaining)?;
            }

            self.stats
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?
                .remove_reference(size);
        }
        Ok(())
    }

    fn chunk_size(&self, digest: &str) -> Result<u64, ChunkStoreError> {
        let data = self
            .backend
            .get(digest)?
            .ok_or_else(|| ChunkStoreError::not_found(format!("chunk {digest}")))?;
        Ok(data.len() as u64)
    }

    fn rebuild_stats(&self) -> Result<(), ChunkStoreError> {
        let mut stats = Stats::default();
        let refcount = self
            .refcount
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?;

        for (digest, count) in refcount.iter() {
            let size = self.chunk_size(digest)?;
            stats.stored_bytes += size;
            stats.total_bytes += size * count;
        }

        *self.stats.lock().map_err(|_| ChunkStoreError::LockError)? = stats;
        Ok(())
    }

    pub fn stats(&self) -> Result<Stats, ChunkStoreError> {
        self.stats
            .lock()
            .map(|s| s.clone())
            .map_err(|_| ChunkStoreError::LockError)
    }

    /// Reference count for a chunk digest (for tests and diagnostics).
    pub fn chunk_refcount(&self, digest: &str) -> Result<Option<u64>, ChunkStoreError> {
        Ok(self
            .refcount
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?
            .get(digest))
    }
}

impl<B: ChunkBackend> std::fmt::Debug for ChunkStore<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkStore").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_to_writer_streams_chunks() {
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        let chunk_size = 64usize;
        let payload = vec![0xABu8; chunk_size * 2 + 10];
        store.ingest_fixed("parts", &payload, chunk_size).unwrap();

        let mut streamed = Vec::new();
        store.read_to_writer("parts", &mut streamed).unwrap();
        assert_eq!(streamed, payload);
    }

    #[test]
    fn ingest_and_read_roundtrip() {
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        store.ingest("doc", b"hello world").unwrap();
        assert_eq!(store.read("doc").unwrap(), b"hello world");
    }

    #[test]
    fn duplicate_file_dedups_chunks() {
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        store.ingest("a", b"same").unwrap();
        store.ingest("b", b"same").unwrap();
        let stats = store.stats().unwrap();
        assert!(stats.savings_pct() > 0.0);
    }

    #[test]
    fn reingest_replaces_without_leaking_refcounts() {
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        store.ingest("doc", b"old").unwrap();
        store.ingest("doc", b"new-longer").unwrap();
        assert_eq!(store.read("doc").unwrap(), b"new-longer");
        store.delete("doc").unwrap();
        assert!(store.read("doc").is_err());
    }

    #[test]
    fn metadata_persists_across_reopen() {
        let backend = Arc::new(MemoryBackend::new());
        {
            let store = ChunkStore::open(Arc::clone(&backend)).unwrap();
            store.ingest("doc", b"persisted").unwrap();
        }
        {
            let store = ChunkStore::open(backend).unwrap();
            assert_eq!(store.read("doc").unwrap(), b"persisted");
        }
    }

    #[test]
    fn fs_backend_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        {
            let store = ChunkStore::open(FsBackend::new(&root).unwrap()).unwrap();
            store.ingest("doc", b"on-disk").unwrap();
        }
        {
            let store = ChunkStore::open(FsBackend::new(&root).unwrap()).unwrap();
            assert_eq!(store.read("doc").unwrap(), b"on-disk");
        }
    }
}
