mod codec;
mod fs_backend;
mod fsck;
mod manifest;
mod persistence;
#[cfg(feature = "postgres")]
mod postgres_meta;
mod refcount;
mod stats;

pub use codec::{ChunkCodec, IdentityCodec};
pub use fs_backend::FsBackend;
pub use fsck::FsckReport;
pub use manifest::Manifest;
#[cfg(feature = "postgres")]
pub use postgres_meta::PostgresMetadata;
pub use refcount::{ChunkRef, RefCount};
pub use stats::Stats;

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::chunker::{for_each_chunk, CdcChunker, Chunker, FixedChunker};
use crate::error::ChunkStoreError;
use crate::hasher::{digest_hex, verify_digest};
use crate::store::fsck::{collect_orphans, is_chunk_digest};
use crate::store::persistence::{
    load_manifests, load_refcounts, remove_manifest, remove_refcount, save_manifest, save_refcount,
};

/// Key-value backend for chunk blobs and metadata persistence.
pub trait ChunkBackend: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ChunkStoreError>;
    fn put(&self, key: &str, data: &[u8]) -> Result<(), ChunkStoreError>;
    fn exists(&self, key: &str) -> Result<bool, ChunkStoreError>;
    fn delete(&self, key: &str) -> Result<(), ChunkStoreError>;

    /// List keys with the given prefix. `Ok(None)` if the backend cannot list.
    fn list_keys(&self, prefix: &str) -> Result<Option<Vec<String>>, ChunkStoreError> {
        let _ = prefix;
        Ok(None)
    }
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

    fn list_keys(&self, prefix: &str) -> Result<Option<Vec<String>>, ChunkStoreError> {
        self.as_ref().list_keys(prefix)
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

    fn list_keys(&self, prefix: &str) -> Result<Option<Vec<String>>, ChunkStoreError> {
        let data = self.data.lock().map_err(|_| ChunkStoreError::LockError)?;
        Ok(Some(
            data.keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect(),
        ))
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
    codec: Option<Arc<dyn ChunkCodec>>,
    #[cfg(feature = "postgres")]
    pg: Option<PostgresMetadata>,
}

impl<B: ChunkBackend> ChunkStore<B> {
    /// Open a store and load metadata (`_manifest/`, `_refcount/`) from the backend.
    pub fn open(backend: B) -> Result<Self, ChunkStoreError> {
        let manifest = load_manifests(&backend)?;
        let mut refcount = load_refcounts(&backend)?;
        backfill_refcount_sizes(&backend, &mut refcount)?;
        let store = Self {
            backend,
            manifest: Mutex::new(manifest),
            refcount: Mutex::new(refcount),
            stats: Mutex::new(Stats::default()),
            blob: Mutex::new(()),
            codec: None,
            #[cfg(feature = "postgres")]
            pg: None,
        };
        store.rebuild_stats()?;
        Ok(store)
    }

    /// Open with a per-chunk blob codec (encryption-at-rest hook).
    pub fn open_with_codec(
        backend: B,
        codec: Arc<dyn ChunkCodec>,
    ) -> Result<Self, ChunkStoreError> {
        let mut store = Self::open(backend)?;
        store.codec = Some(codec);
        Ok(store)
    }

    /// Open with Postgres as the source of truth for manifests and refcounts.
    ///
    /// Chunk blobs still live on `backend`. See `docs/CONSISTENCY.md`.
    #[cfg(feature = "postgres")]
    pub fn open_postgres(backend: B, dsn: &str) -> Result<Self, ChunkStoreError> {
        let pg = PostgresMetadata::connect(dsn)?;
        let manifest = pg.load_manifests()?;
        let refcount = pg.load_refcounts()?;
        let store = Self {
            backend,
            manifest: Mutex::new(manifest),
            refcount: Mutex::new(refcount),
            stats: Mutex::new(Stats::default()),
            blob: Mutex::new(()),
            codec: None,
            pg: Some(pg),
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
        if chunk_size == 0 {
            return Err(ChunkStoreError::invalid_argument(
                "chunk_size must be greater than 0",
            ));
        }
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
        if chunk_size == 0 {
            return Err(ChunkStoreError::invalid_argument(
                "chunk_size must be greater than 0",
            ));
        }
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

    /// Stream a filesystem path into the store without loading the whole file.
    pub fn ingest_path(
        &self,
        file_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut file = std::fs::File::open(path.as_ref())?;
        self.ingest_reader_fixed(file_id, &mut file, FixedChunker::default_chunk_size())
    }

    /// Stream a filesystem path with CDC chunking.
    pub fn ingest_path_cdc(
        &self,
        file_id: &str,
        path: impl AsRef<Path>,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut file = std::fs::File::open(path.as_ref())?;
        self.ingest_reader_cdc(file_id, &mut file)
    }

    fn ingest_reader_with_chunker<R: Read>(
        &self,
        file_id: &str,
        reader: &mut R,
        chunker: &mut dyn Chunker,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut session = IngestSession::new();
        for_each_chunk(reader, chunker, |chunk| session.accept(self, chunk))?;
        session.commit(self, file_id)
    }

    fn ingest_with_chunker(
        &self,
        file_id: &str,
        data: &[u8],
        chunker: &mut dyn Chunker,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut session = IngestSession::new();
        for chunk in chunker.feed(data)? {
            session.accept(self, chunk)?;
        }
        for chunk in chunker.finish()? {
            session.accept(self, chunk)?;
        }
        session.commit(self, file_id)
    }

    pub fn ingest_chunks(
        &self,
        file_id: &str,
        chunks: &[Vec<u8>],
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut session = IngestSession::new();
        for chunk in chunks {
            session.accept(self, chunk.clone())?;
        }
        session.commit(self, file_id)
    }

    fn store_chunk(&self, digest: &str, data: &[u8]) -> Result<bool, ChunkStoreError> {
        let stored = if let Some(codec) = &self.codec {
            codec.encode(data)?
        } else {
            Vec::new()
        };
        let blob = if self.codec.is_some() {
            stored.as_slice()
        } else {
            data
        };
        let size = data.len() as u64;

        let exists = {
            let _blob = self.blob.lock().map_err(|_| ChunkStoreError::LockError)?;
            let exists = self.backend.exists(digest)?;
            if !exists {
                self.backend.put(digest, blob)?;
            }
            exists
        };

        #[cfg(feature = "postgres")]
        if self.pg.is_some() {
            return Ok(exists);
        }

        {
            let mut refcount = self
                .refcount
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            let count = refcount.increment(digest, size)?;
            let stored_size = refcount.size_of(digest).unwrap_or(size);
            save_refcount(&self.backend, digest, count, stored_size)?;
        }

        let mut stats = self.stats.lock().map_err(|_| ChunkStoreError::LockError)?;
        if exists {
            stats.add_reference(size);
        } else {
            stats.add_unique_chunk(size);
        }

        Ok(exists)
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
        #[cfg(feature = "postgres")]
        if let Some(pg) = &self.pg {
            return pg
                .manifest_digests(file_id)?
                .ok_or_else(|| ChunkStoreError::not_found(format!("file {file_id}")));
        }
        let manifest = self
            .manifest
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?;
        manifest.get(file_id).map(<[String]>::to_vec)
    }

    fn peek_digests(&self, file_id: &str) -> Result<Option<Vec<String>>, ChunkStoreError> {
        #[cfg(feature = "postgres")]
        if let Some(pg) = &self.pg {
            return pg.manifest_digests(file_id);
        }
        let manifest = self
            .manifest
            .lock()
            .map_err(|_| ChunkStoreError::LockError)?;
        Ok(manifest.try_get(file_id).map(<[String]>::to_vec))
    }

    fn read_chunk_by_digest(&self, digest: &str) -> Result<Vec<u8>, ChunkStoreError> {
        let stored = self
            .backend
            .get(digest)?
            .ok_or_else(|| ChunkStoreError::not_found(format!("chunk {digest}")))?;
        let chunk = if let Some(codec) = &self.codec {
            codec.decode(&stored)?
        } else {
            stored
        };
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
        #[cfg(feature = "postgres")]
        if let Some(pg) = &self.pg {
            let digests = pg
                .manifest_digests(file_id)?
                .ok_or_else(|| ChunkStoreError::not_found(format!("file {file_id}")))?;
            let gc = pg.commit_delete(file_id, &digests)?;
            for digest in gc {
                self.backend.delete(&digest)?;
            }
            {
                let mut manifest = self
                    .manifest
                    .lock()
                    .map_err(|_| ChunkStoreError::LockError)?;
                let _ = manifest.remove(file_id);
            }
            return Ok(());
        }

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
                save_refcount(&self.backend, digest, remaining, size)?;
            }

            self.stats
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?
                .remove_reference(size);
        }
        Ok(())
    }

    fn chunk_size(&self, digest: &str) -> Result<u64, ChunkStoreError> {
        #[cfg(feature = "postgres")]
        if let Some(pg) = &self.pg {
            if let Some(size) = pg.chunk_size(digest)? {
                return Ok(size);
            }
        }
        {
            let refcount = self
                .refcount
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            if let Some(size) = refcount.size_of(digest) {
                if size > 0 {
                    return Ok(size);
                }
            }
        }
        let data = self
            .backend
            .get(digest)?
            .ok_or_else(|| ChunkStoreError::not_found(format!("chunk {digest}")))?;
        let len = if let Some(codec) = &self.codec {
            codec.decode(&data)?.len() as u64
        } else {
            data.len() as u64
        };
        Ok(len)
    }

    fn rebuild_stats(&self) -> Result<(), ChunkStoreError> {
        #[cfg(feature = "postgres")]
        if let Some(pg) = &self.pg {
            let stats = pg.compute_stats()?;
            *self.stats.lock().map_err(|_| ChunkStoreError::LockError)? = stats;
            return Ok(());
        }

        let mut stats = Stats::default();
        let refs: Vec<(String, u64, u64)> = {
            let refcount = self
                .refcount
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            refcount
                .iter_refs()
                .map(|(digest, meta)| (digest.to_string(), meta.count, meta.size))
                .collect()
        };

        for (digest, count, stored_size) in refs {
            let size = if stored_size > 0 {
                stored_size
            } else {
                self.chunk_size(&digest)?
            };
            stats.stored_bytes += size;
            stats.total_bytes += size * count;
        }

        *self.stats.lock().map_err(|_| ChunkStoreError::LockError)? = stats;
        Ok(())
    }

    pub fn stats(&self) -> Result<Stats, ChunkStoreError> {
        #[cfg(feature = "postgres")]
        if let Some(pg) = &self.pg {
            return pg.compute_stats();
        }
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

    /// Scan blob keys vs refcounts. When `repair` is true, unlink orphan blobs.
    pub fn fsck(&self, repair: bool) -> Result<FsckReport, ChunkStoreError> {
        let live: Vec<String> = {
            #[cfg(feature = "postgres")]
            if let Some(pg) = &self.pg {
                let refs = pg.list_refcounts()?;
                refs.into_iter().map(|(d, _, _)| d).collect()
            } else {
                self.refcount
                    .lock()
                    .map_err(|_| ChunkStoreError::LockError)?
                    .iter()
                    .map(|(d, _)| d.to_string())
                    .collect()
            }
            #[cfg(not(feature = "postgres"))]
            {
                self.refcount
                    .lock()
                    .map_err(|_| ChunkStoreError::LockError)?
                    .iter()
                    .map(|(d, _)| d.to_string())
                    .collect()
            }
        };

        let mut report = FsckReport::default();
        let live_set: std::collections::HashSet<&str> = live.iter().map(String::as_str).collect();

        for digest in &live {
            if !self.backend.exists(digest)? {
                report.missing_blobs.push(digest.clone());
            }
        }
        report.orphan_blobs = collect_orphans(&self.backend, &live_set)?;
        if repair {
            for key in &report.orphan_blobs {
                self.backend.delete(key)?;
                report.repaired += 1;
            }
        }
        Ok(report)
    }
}

#[derive(Debug)]
struct IngestSession {
    digests: Vec<String>,
    sizes: Vec<u64>,
    file_bytes: u64,
}

impl IngestSession {
    fn new() -> Self {
        Self {
            digests: Vec::new(),
            sizes: Vec::new(),
            file_bytes: 0,
        }
    }

    fn accept<B: ChunkBackend>(
        &mut self,
        store: &ChunkStore<B>,
        chunk: Vec<u8>,
    ) -> Result<(), ChunkStoreError> {
        self.file_bytes += chunk.len() as u64;
        let digest = digest_hex(&chunk);
        store.store_chunk(&digest, &chunk)?;
        self.sizes.push(chunk.len() as u64);
        self.digests.push(digest);
        Ok(())
    }

    fn commit<B: ChunkBackend>(
        self,
        store: &ChunkStore<B>,
        file_id: &str,
    ) -> Result<Vec<String>, ChunkStoreError> {
        let old = store.peek_digests(file_id)?;

        #[cfg(feature = "postgres")]
        if let Some(pg) = &store.pg {
            let gc = pg.commit_ingest(
                file_id,
                &self.digests,
                self.file_bytes,
                &self.sizes,
                old.as_deref(),
            )?;
            for digest in gc {
                store.backend.delete(&digest)?;
            }
            {
                let mut manifest = store
                    .manifest
                    .lock()
                    .map_err(|_| ChunkStoreError::LockError)?;
                manifest.insert(file_id, self.digests.clone(), self.file_bytes)?;
            }
            return Ok(self.digests);
        }

        {
            let mut manifest = store
                .manifest
                .lock()
                .map_err(|_| ChunkStoreError::LockError)?;
            manifest.insert(file_id, self.digests.clone(), self.file_bytes)?;
        }
        save_manifest(&store.backend, file_id, &self.digests, self.file_bytes)?;

        if let Some(old) = old {
            store.release_digests(&old)?;
        }
        Ok(self.digests)
    }
}

fn backfill_refcount_sizes<B: ChunkBackend>(
    backend: &B,
    refcount: &mut RefCount,
) -> Result<(), ChunkStoreError> {
    let stale: Vec<(String, u64)> = refcount
        .iter_refs()
        .filter(|(_, meta)| meta.size == 0 && meta.count > 0)
        .map(|(digest, meta)| (digest.to_string(), meta.count))
        .collect();
    for (digest, count) in stale {
        let Some(data) = backend.get(&digest)? else {
            continue;
        };
        if !is_chunk_digest(&digest) {
            continue;
        }
        let size = data.len() as u64;
        refcount.set(&digest, count, size);
        save_refcount(backend, &digest, count, size)?;
    }
    Ok(())
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

    #[test]
    fn ingest_reader_matches_buffer_ingest() {
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        let payload = vec![0x11u8; 10_000];
        let mut cursor = std::io::Cursor::new(payload.clone());
        store
            .ingest_reader_fixed("streamed", &mut cursor, 1024)
            .unwrap();
        assert_eq!(store.read("streamed").unwrap(), payload);
    }

    #[test]
    fn ingest_path_streams_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        let payload = vec![0x22u8; 8000];
        std::fs::write(&path, &payload).unwrap();
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        store.ingest_path("from-disk", &path).unwrap();
        assert_eq!(store.read("from-disk").unwrap(), payload);
    }

    #[test]
    fn fsck_repairs_orphan_blobs() {
        let backend = Arc::new(MemoryBackend::new());
        let store = ChunkStore::open(Arc::clone(&backend)).unwrap();
        store.ingest("doc", b"keep-me").unwrap();
        let orphan = "ab".repeat(32);
        backend.put(&orphan, b"leaked").unwrap();

        let report = store.fsck(true).unwrap();
        assert!(report.orphan_blobs.contains(&orphan));
        assert_eq!(report.repaired, 1);
        assert!(!backend.exists(&orphan).unwrap());
        assert_eq!(store.read("doc").unwrap(), b"keep-me");
    }

    struct XorCodec;

    impl ChunkCodec for XorCodec {
        fn encode(&self, plaintext: &[u8]) -> Result<Vec<u8>, ChunkStoreError> {
            Ok(plaintext.iter().map(|b| b ^ 0x5A).collect())
        }

        fn decode(&self, stored: &[u8]) -> Result<Vec<u8>, ChunkStoreError> {
            Ok(stored.iter().map(|b| b ^ 0x5A).collect())
        }
    }

    #[test]
    fn codec_roundtrip_verifies_plaintext_digest() {
        let store = ChunkStore::open_with_codec(MemoryBackend::new(), Arc::new(XorCodec)).unwrap();
        store.ingest("secret", b"plaintext-chunk").unwrap();
        assert_eq!(store.read("secret").unwrap(), b"plaintext-chunk");
    }

    #[test]
    fn gc_chunks_increments_on_delete() {
        let store = ChunkStore::open(MemoryBackend::new()).unwrap();
        store.ingest("only", b"gc-me").unwrap();
        store.delete("only").unwrap();
        assert_eq!(store.stats().unwrap().gc_chunks, 1);
    }
}
