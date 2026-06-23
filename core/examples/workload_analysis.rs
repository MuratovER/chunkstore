use std::sync::Arc;

use chunkstore::{chunk_cdc, ChunkStore, MemoryBackend};

fn build_pool(pool_chunks: usize, chunk_size: usize) -> Vec<Vec<u8>> {
    (0..pool_chunks)
        .map(|idx| {
            (0..chunk_size)
                .map(|i| ((idx * 31 + i) % 251) as u8)
                .collect()
        })
        .collect()
}

fn assemble_file(pool: &[Vec<u8>], picks: &[usize]) -> Vec<u8> {
    let mut out = Vec::new();
    for &idx in picks {
        out.extend_from_slice(&pool[idx % pool.len()]);
    }
    out
}

fn dedup_savings(files: usize, pool_chunks: usize, chunk_size: usize) -> f64 {
    let pool = build_pool(pool_chunks, chunk_size);
    let store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
    for file_id in 0..files {
        let picks = [
            file_id % pool_chunks,
            (file_id * 3 + 1) % pool_chunks,
            (file_id * 7 + 2) % pool_chunks,
        ];
        let take = 1 + (file_id % 3);
        let payload = assemble_file(&pool, &picks[..take]);
        store
            .ingest(&format!("file-{file_id}"), &payload)
            .expect("ingest");
    }
    store.stats().expect("stats").savings_pct()
}

fn prefix_insert_savings() -> (f64, f64) {
    let size = 20 * 1024 * 1024;
    let base: Vec<u8> = (0..size)
        .map(|i: usize| {
            let x = i.wrapping_mul(0x9E37_79B9);
            ((x >> 24) ^ (x >> 16) ^ (x >> 8)) as u8
        })
        .collect();
    let mut edited = vec![0xAB];
    edited.extend_from_slice(&base);

    let fixed_store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
    fixed_store.ingest("base", &base).expect("ingest");
    fixed_store.ingest("edited", &edited).expect("ingest");
    let fixed = fixed_store.stats().expect("stats").savings_pct();

    let cdc_store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
    cdc_store.ingest_cdc("base", &base).expect("ingest");
    cdc_store.ingest_cdc("edited", &edited).expect("ingest");
    let cdc = cdc_store.stats().expect("stats").savings_pct();

    (fixed, cdc)
}

fn random_binary_savings(files: usize, size: usize) -> f64 {
    let store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
    for file_id in 0..files {
        let payload: Vec<u8> = (0..size)
            .map(|i| ((file_id * 17 + i * 13) % 256) as u8)
            .collect();
        store
            .ingest(&format!("rand-{file_id}"), &payload)
            .expect("ingest");
    }
    store.stats().expect("stats").savings_pct()
}

fn version_90_10_savings() -> f64 {
    let shared: Vec<u8> = (0..18 * 1024 * 1024).map(|i| (i % 200) as u8).collect();
    let mut v1 = shared.clone();
    v1.extend((0..2 * 1024 * 1024).map(|i| (i % 17) as u8));
    let mut v2 = shared;
    v2.extend((0..2 * 1024 * 1024).map(|i| (i % 23) as u8));

    let store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
    store.ingest("v1", &v1).expect("ingest");
    store.ingest("v2", &v2).expect("ingest");
    store.stats().expect("stats").savings_pct()
}

fn main() {
    println!("chunkstore workload analysis");
    println!("----------------------------");

    let pool = dedup_savings(1000, 200, 4 * 1024 * 1024);
    println!("dedup pool (200x4MiB, 1000 files): {pool:.1}%");

    for chunk_size in [64 * 1024, 256 * 1024, 1024 * 1024, 4 * 1024 * 1024] {
        let savings = dedup_savings(200, 50, chunk_size);
        println!("chunk size sweep {chunk_size} bytes: {savings:.1}%");
    }

    let random = random_binary_savings(50, 4 * 1024 * 1024);
    println!("random binaries (50 x 4MiB): {random:.1}%");

    let versions = version_90_10_savings();
    println!("versions 90/10: {versions:.1}%");

    let (fixed, cdc) = prefix_insert_savings();
    println!("prefix insert 1 byte (fixed 4MiB): {fixed:.1}%");
    println!("prefix insert 1 byte (CDC): {cdc:.1}%");

    let base_len = 20 * 1024 * 1024;
    let base: Vec<u8> = (0..base_len)
        .map(|i: usize| {
            let x = i.wrapping_mul(0x9E37_79B9);
            ((x >> 24) ^ (x >> 16) ^ (x >> 8)) as u8
        })
        .collect();
    let mut edited = vec![0xAB];
    edited.extend_from_slice(&base);
    let base_chunks = chunk_cdc(&base).expect("cdc");
    let edited_chunks = chunk_cdc(&edited).expect("cdc");
    let shared = base_chunks
        .iter()
        .filter(|c| edited_chunks.iter().any(|x| x.as_slice() == c.as_slice()))
        .count();
    println!(
        "prefix insert shared CDC chunks: {}/{}",
        shared,
        base_chunks.len()
    );
}
