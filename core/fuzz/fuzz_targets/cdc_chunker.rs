#![no_main]

use chunkstore::{CdcChunker, CdcConfig, Chunker};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let config = CdcConfig {
        min_size: 64,
        avg_size: 256,
        max_size: 1024,
        window_size: 64,
    };
    let Ok(mut chunker) = CdcChunker::new(config) else {
        return;
    };
    let _ = chunker.feed(data);
    let _ = chunker.finish();
});
