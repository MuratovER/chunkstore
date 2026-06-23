mod common;

use std::sync::Arc;
use std::thread;

use chunkstore::FsBackend;

use common::{
    dedup_savings, ingest_from_reader_fixed, make_20mb_with_prefix_insert, make_shared_block_files,
    memory_store, shared_memory_store, temp_fs_store, write_temp_file,
};

#[test]
fn scenario_01_unique_file_download() {
    let store = memory_store();
    let digests = store.ingest("doc", b"unique-payload").unwrap();
    assert_eq!(digests.len(), 1);
    assert_eq!(store.chunk_refcount(&digests[0]).unwrap(), Some(1));
    assert_eq!(store.read("doc").unwrap(), b"unique-payload");
}

#[test]
fn scenario_02_duplicate_file_dedups() {
    let store = memory_store();
    store.ingest("a", b"same-payload").unwrap();
    store.ingest("b", b"same-payload").unwrap();
    let stats = store.stats().unwrap();
    assert!(stats.savings_pct() > 0.0);
    assert_eq!(stats.stored_bytes * 2, stats.total_bytes);
}

#[test]
fn scenario_03_partial_overlap_reuses_prefix() {
    let store = memory_store();
    let chunk_size = 64;
    let prefix = vec![1u8; chunk_size * 2];
    let mut file_a = prefix.clone();
    file_a.extend_from_slice(b"AAA");
    let mut file_b = prefix.clone();
    file_b.extend_from_slice(b"BBBBB");

    let da = store.ingest_fixed("a", &file_a, chunk_size).unwrap();
    let db = store.ingest_fixed("b", &file_b, chunk_size).unwrap();

    assert_eq!(&da[0..2], &db[0..2]);
    assert_ne!(da[2], db[2]);
}

#[test]
fn scenario_04_delete_one_of_two_keeps_shared_chunk() {
    let (_dir, store) = temp_fs_store();
    store.ingest("a", b"shared-prefix").unwrap();
    let digests_b = store.ingest("b", b"shared-prefix").unwrap();
    let shared = digests_b[0].clone();

    store.delete("a").unwrap();
    assert_eq!(store.chunk_refcount(&shared).unwrap(), Some(1));
    assert!(store.read("b").unwrap() == b"shared-prefix");
}

#[test]
fn scenario_05_delete_last_file_gcs_chunk_on_fs() {
    let (dir, store) = temp_fs_store();
    let digests = store.ingest("only", b"gc-me").unwrap();
    let digest = digests[0].clone();
    let root = dir.path().to_path_buf();

    store.delete("only").unwrap();
    drop(store);

    let backend = FsBackend::new(&root).unwrap();
    assert!(!backend.chunk_exists(&digest).unwrap());
}

#[test]
fn scenario_06_concurrent_upload_single_chunk() {
    let (_backend, store) = shared_memory_store();
    let store = Arc::new(store);
    let payload = b"concurrent-same";

    let handles: Vec<_> = (0..2)
        .map(|i| {
            let store = Arc::clone(&store);
            let id = format!("doc-{i}");
            thread::spawn(move || store.ingest(&id, payload).unwrap())
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results[0][0], results[1][0]);
    assert_eq!(store.chunk_refcount(&results[0][0]).unwrap(), Some(2));
    let stats = store.stats().unwrap();
    assert!(stats.savings_pct() > 0.0);
}

#[test]
fn scenario_07_streaming_upload_from_disk() {
    let store = memory_store();
    let data = vec![5u8; 256 * 1024 + 17];
    let (_dir, file) = write_temp_file(&data);
    let digests = ingest_from_reader_fixed(&store, "streamed", file, 64 * 1024);
    assert!(!digests.is_empty());
    assert_eq!(store.read("streamed").unwrap(), data);
}

#[test]
fn scenario_08_cdc_beats_fixed_on_prefix_insert() {
    let fixed_store = memory_store();
    let (base, edited) = make_20mb_with_prefix_insert();

    fixed_store.ingest("base", &base).unwrap();
    fixed_store.ingest("edited", &edited).unwrap();
    let fixed_savings = dedup_savings(&fixed_store);

    let cdc_store = memory_store();
    cdc_store.ingest_cdc("base", &base).unwrap();
    cdc_store.ingest_cdc("edited", &edited).unwrap();
    let cdc_savings = dedup_savings(&cdc_store);

    assert!(fixed_savings < 5.0, "fixed savings={fixed_savings}");
    assert!(cdc_savings > 30.0, "cdc savings={cdc_savings}");
}

#[test]
fn scenario_09_shared_binary_block_savings() {
    let store = memory_store();
    let (a, b) = make_shared_block_files(4 * 1024 * 1024);
    store.ingest("a", &a).unwrap();
    store.ingest("b", &b).unwrap();
    let savings = dedup_savings(&store);
    assert!(savings >= 40.0, "savings={savings}");
}
