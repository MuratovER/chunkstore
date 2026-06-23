use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use chunkstore::{ChunkStore, MemoryBackend};

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

fn bench_dedup_pool(c: &mut Criterion) {
    let pool = build_pool(200, 4 * 1024 * 1024);
    let payloads: Vec<Vec<u8>> = (0..1000)
        .map(|file_id| {
            let picks = [
                file_id % 200,
                (file_id * 3 + 1) % 200,
                (file_id * 7 + 2) % 200,
                (file_id * 11 + 3) % 200,
            ];
            let take = 1 + (file_id % 4);
            assemble_file(&pool, &picks[..take])
        })
        .collect();

    c.bench_function("dedup_pool_200x4mb_1000_files", |b| {
        b.iter(|| {
            let store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
            for (idx, payload) in payloads.iter().enumerate() {
                store
                    .ingest(&format!("file-{idx}"), black_box(payload))
                    .expect("ingest");
            }
            black_box(store.stats().expect("stats").savings_pct())
        });
    });
}

fn bench_prefix_insert(c: &mut Criterion) {
    let size = 20 * 1024 * 1024;
    let base: Vec<u8> = (0..size)
        .map(|i: usize| {
            let x = i.wrapping_mul(0x9E37_79B9);
            ((x >> 24) ^ (x >> 16) ^ (x >> 8)) as u8
        })
        .collect();
    let mut edited = vec![0xAB];
    edited.extend_from_slice(&base);

    c.bench_function("prefix_insert_fixed", |b| {
        b.iter(|| {
            let store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
            store.ingest("base", black_box(&base)).expect("ingest");
            store.ingest("edited", black_box(&edited)).expect("ingest");
            black_box(store.stats().expect("stats").savings_pct())
        });
    });

    c.bench_function("prefix_insert_cdc", |b| {
        b.iter(|| {
            let store = ChunkStore::open(Arc::new(MemoryBackend::new())).expect("open");
            store.ingest_cdc("base", black_box(&base)).expect("ingest");
            store
                .ingest_cdc("edited", black_box(&edited))
                .expect("ingest");
            black_box(store.stats().expect("stats").savings_pct())
        });
    });
}

criterion_group!(benches, bench_dedup_pool, bench_prefix_insert);
criterion_main!(benches);
