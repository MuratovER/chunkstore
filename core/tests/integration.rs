use std::sync::Arc;

use chunkstore::{ChunkStore, MemoryBackend};

#[test]
fn cross_module_ingest_read_delete() {
    let store = ChunkStore::open(MemoryBackend::new()).unwrap();
    store.ingest("doc", b"integration test payload").unwrap();

    let data = store.read("doc").unwrap();
    assert_eq!(data, b"integration test payload");

    store.delete("doc").unwrap();
    assert!(store.read("doc").is_err());
}

#[test]
fn cdc_ingest_roundtrip() {
    let store = ChunkStore::open(MemoryBackend::new()).unwrap();
    let payload = vec![7u8; 600 * 1024];
    store.ingest_cdc("cdc-doc", &payload).unwrap();
    assert_eq!(store.read("cdc-doc").unwrap(), payload);
}

#[test]
fn persisted_metadata_reopen() {
    let backend = Arc::new(MemoryBackend::new());
    {
        let store = ChunkStore::open(Arc::clone(&backend)).unwrap();
        store.ingest("doc", b"shared").unwrap();
    }
    {
        let store = ChunkStore::open(backend).unwrap();
        assert_eq!(store.read("doc").unwrap(), b"shared");
        let stats = store.stats().unwrap();
        assert_eq!(stats.stored_bytes, stats.total_bytes);
    }
}
