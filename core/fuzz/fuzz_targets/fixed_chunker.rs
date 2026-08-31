#![no_main]

use chunkstore::{Chunker, FixedChunker};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let chunk_size = (usize::from(data[0]) % 64) + 1;
    let mut chunker = FixedChunker::new(chunk_size);
    let _ = chunker.feed(&data[1..]);
    let _ = chunker.finish();
});
