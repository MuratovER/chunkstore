use std::io::{Read, Write};
use std::sync::Arc;

use chunkstore::{ChunkStore, FsBackend, MemoryBackend};
use tempfile::tempdir;

pub fn temp_fs_backend() -> (tempfile::TempDir, FsBackend) {
    let dir = tempdir().expect("tempdir");
    let backend = FsBackend::new(dir.path()).expect("fs backend");
    (dir, backend)
}

pub fn temp_fs_store() -> (tempfile::TempDir, ChunkStore<FsBackend>) {
    let (dir, backend) = temp_fs_backend();
    let store = ChunkStore::open(backend).expect("open store");
    (dir, store)
}

pub fn memory_store() -> ChunkStore<Arc<MemoryBackend>> {
    ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open store")
}

pub fn shared_memory_store() -> (Arc<MemoryBackend>, ChunkStore<Arc<MemoryBackend>>) {
    let backend = Arc::new(MemoryBackend::new());
    let store = ChunkStore::open(Arc::clone(&backend)).expect("open store");
    (backend, store)
}

pub fn shared_fs_store() -> (tempfile::TempDir, Arc<ChunkStore<FsBackend>>) {
    let (dir, backend) = temp_fs_backend();
    let store = Arc::new(ChunkStore::open(backend).expect("open store"));
    (dir, store)
}

pub fn write_temp_file(data: &[u8]) -> (tempfile::TempDir, std::fs::File) {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("upload.bin");
    let mut file = std::fs::File::create(&path).expect("create file");
    file.write_all(data).expect("write file");
    file.sync_all().expect("sync file");
    let read_file = std::fs::File::open(path).expect("open file");
    (dir, read_file)
}

pub fn dedup_savings(store: &ChunkStore<impl chunkstore::ChunkBackend>) -> f64 {
    store.stats().expect("stats").savings_pct()
}

pub fn make_20mb_with_prefix_insert() -> (Vec<u8>, Vec<u8>) {
    let size = 20 * 1024 * 1024;
    let base: Vec<u8> = (0..size)
        .map(|i: usize| {
            let x = i.wrapping_mul(0x9E37_79B9);
            ((x >> 24) ^ (x >> 16) ^ (x >> 8)) as u8
        })
        .collect();
    let mut edited = Vec::with_capacity(base.len() + 1);
    edited.push(0xAB);
    edited.extend_from_slice(&base);
    (base, edited)
}

pub fn make_shared_block_files(block_size: usize) -> (Vec<u8>, Vec<u8>) {
    let shared = vec![0xBBu8; block_size];
    let mut a = shared.clone();
    a.extend_from_slice(b"tail-a");
    let mut b = shared.clone();
    b.extend_from_slice(b"tail-b-longer");
    (a, b)
}

pub fn ingest_from_reader_fixed(
    store: &ChunkStore<impl chunkstore::ChunkBackend>,
    file_id: &str,
    mut reader: impl Read,
    chunk_size: usize,
) -> Vec<String> {
    store
        .ingest_reader_fixed(file_id, &mut reader, chunk_size)
        .expect("ingest from reader")
}
