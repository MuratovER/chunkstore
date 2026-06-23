use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use chunkstore::chunker::{chunk_cdc, Chunker, FixedChunker};
use chunkstore::{ChunkStore, MemoryBackend};

fn bench_fixed_chunking(c: &mut Criterion) {
    let data = vec![0u8; 16 * 1024 * 1024];
    c.bench_function("fixed_4mb_chunks_16mb", |b| {
        b.iter(|| {
            let mut chunker = FixedChunker::new(4 * 1024 * 1024);
            let _ = chunker.feed(black_box(&data));
            let _ = chunker.finish();
        });
    });
}

fn bench_cdc_chunking(c: &mut Criterion) {
    let data: Vec<u8> = (0..16 * 1024 * 1024)
        .map(|i: usize| {
            let x = i.wrapping_mul(0x9E37_79B9);
            ((x >> 24) ^ (x >> 16) ^ (x >> 8)) as u8
        })
        .collect();
    c.bench_function("cdc_default_16mb", |b| {
        b.iter(|| black_box(chunk_cdc(black_box(&data)).expect("cdc")));
    });
}

fn bench_chunk_size_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixed_chunk_size_sweep");
    let data = vec![7u8; 8 * 1024 * 1024];
    for chunk_size in [64 * 1024, 256 * 1024, 1024 * 1024, 4 * 1024 * 1024] {
        group.bench_with_input(
            BenchmarkId::from_parameter(chunk_size),
            &chunk_size,
            |b, &chunk_size| {
                b.iter(|| {
                    let mut chunker = FixedChunker::new(chunk_size);
                    let _ = chunker.feed(black_box(&data));
                    let _ = chunker.finish();
                });
            },
        );
    }
    group.finish();
}

fn bench_dedup_pool_ingest(c: &mut Criterion) {
    let pool: Vec<Vec<u8>> = (0..20).map(|idx| vec![idx as u8; 256 * 1024]).collect();
    let payloads: Vec<Vec<u8>> = (0..100)
        .map(|file_id| {
            let mut out = Vec::new();
            for pick in 0..(1 + file_id % 3) {
                out.extend_from_slice(&pool[(file_id + pick) % pool.len()]);
            }
            out
        })
        .collect();

    c.bench_function("dedup_pool_100_files", |b| {
        b.iter(|| {
            let store = ChunkStore::open(MemoryBackend::new()).expect("open");
            for (idx, payload) in payloads.iter().enumerate() {
                store
                    .ingest(&format!("f-{idx}"), black_box(payload))
                    .expect("ingest");
            }
            black_box(store.stats().expect("stats"));
        });
    });
}

criterion_group!(
    benches,
    bench_fixed_chunking,
    bench_cdc_chunking,
    bench_chunk_size_sweep,
    bench_dedup_pool_ingest
);
criterion_main!(benches);
