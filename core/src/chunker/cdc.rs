use fastcdc::v2020::{FastCDC, Normalization};

use crate::chunker::Chunker;
use crate::error::ChunkStoreError;

/// CDC chunker parameters (FastCDC v2020).
#[derive(Debug, Clone, Copy)]
pub struct CdcConfig {
    pub min_size: usize,
    pub avg_size: usize,
    pub max_size: usize,
    pub window_size: usize,
}

impl Default for CdcConfig {
    fn default() -> Self {
        Self {
            min_size: 256 * 1024,
            avg_size: 4 * 1024 * 1024,
            max_size: 8 * 1024 * 1024,
            window_size: 64,
        }
    }
}

fn chunk_bytes(data: &[u8], config: CdcConfig) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
    if config.min_size == 0 || config.avg_size < config.min_size {
        return Err(ChunkStoreError::invalid_argument(
            "invalid CDC size parameters",
        ));
    }
    if config.max_size < config.avg_size {
        return Err(ChunkStoreError::invalid_argument(
            "max_size must be >= avg_size",
        ));
    }

    let mut chunks = Vec::new();
    for entry in FastCDC::with_level(
        data,
        config.min_size,
        config.avg_size,
        config.max_size,
        Normalization::Level1,
    ) {
        chunks.push(data[entry.offset..entry.offset + entry.length].to_vec());
    }
    Ok(chunks)
}

/// Content-defined chunker backed by FastCDC v2020.
#[derive(Debug)]
pub struct CdcChunker {
    config: CdcConfig,
    buffer: Vec<u8>,
}

impl CdcChunker {
    pub fn new(config: CdcConfig) -> Result<Self, ChunkStoreError> {
        if config.window_size == 0 {
            return Err(ChunkStoreError::invalid_argument("window_size must be > 0"));
        }
        let _ = config.min_size;
        Ok(Self {
            config,
            buffer: Vec::new(),
        })
    }

    pub fn with_defaults() -> Result<Self, ChunkStoreError> {
        Self::new(CdcConfig::default())
    }

    fn chunk_buffer(&self, data: &[u8]) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
        chunk_bytes(data, self.config)
    }
}

impl Chunker for CdcChunker {
    fn feed(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
        self.buffer.extend_from_slice(data);
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let data = std::mem::take(&mut self.buffer);
        self.chunk_buffer(&data)
    }
}

/// Chunk bytes with default CDC settings.
pub fn chunk_cdc(data: &[u8]) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
    chunk_bytes(data, CdcConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdc_emits_non_empty_chunks() {
        let data = vec![0u8; 512 * 1024];
        let chunks = chunk_cdc(&data).unwrap();
        assert!(!chunks.is_empty());
    }

    #[test]
    fn cdc_chunks_are_non_empty() {
        let data = vec![42u8; 3 * 1024 * 1024];
        let chunks = chunk_cdc(&data).unwrap();
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn cdc_shared_chunks_with_prefix_insert() {
        let (base, edited) = prefix_insert_payload();
        let base_chunks = chunk_cdc(&base).unwrap();
        let edited_chunks = chunk_cdc(&edited).unwrap();
        let shared = shared_chunk_count(&base_chunks, &edited_chunks);

        assert!(
            shared > base_chunks.len() / 2,
            "shared={shared} of {}",
            base_chunks.len()
        );
    }

    #[test]
    fn cdc_prefix_insert_savings_beat_fixed() {
        let (base, edited) = prefix_insert_payload();

        let mut fixed = crate::chunker::FixedChunker::new(4 * 1024 * 1024);
        let fa = {
            let mut c = fixed.feed(&base).unwrap();
            c.extend(fixed.finish().unwrap());
            c
        };
        let mut fixed2 = crate::chunker::FixedChunker::new(4 * 1024 * 1024);
        let fb = {
            let mut c = fixed2.feed(&edited).unwrap();
            c.extend(fixed2.finish().unwrap());
            c
        };
        let fixed_shared = fa.iter().filter(|c| fb.iter().any(|x| x == *c)).count();

        let ba = chunk_cdc(&base).unwrap();
        let bb = chunk_cdc(&edited).unwrap();
        let cdc_shared = shared_chunk_count(&ba, &bb);

        assert!(fixed_shared <= 1, "fixed_shared={fixed_shared}");
        assert!(cdc_shared > fixed_shared, "cdc_shared={cdc_shared}");
    }

    fn prefix_insert_payload() -> (Vec<u8>, Vec<u8>) {
        let size = 20 * 1024 * 1024;
        let base: Vec<u8> = (0..size)
            .map(|i: usize| {
                let x = i.wrapping_mul(0x9E37_79B9);
                ((x >> 24) ^ (x >> 16) ^ (x >> 8)) as u8
            })
            .collect();
        let mut edited = vec![0xAB];
        edited.extend_from_slice(&base);
        (base, edited)
    }

    fn shared_chunk_count(a: &[Vec<u8>], b: &[Vec<u8>]) -> usize {
        a.iter()
            .filter(|chunk| b.iter().any(|c| c.as_slice() == chunk.as_slice()))
            .count()
    }
}
