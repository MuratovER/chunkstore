use crate::chunker::Chunker;
use crate::error::ChunkStoreError;

/// Fixed-size chunker (default 4 MiB in production configs).
#[derive(Debug)]
pub struct FixedChunker {
    chunk_size: usize,
    buffer: Vec<u8>,
}

impl FixedChunker {
    pub fn new(chunk_size: usize) -> Self {
        debug_assert!(chunk_size > 0, "chunk_size must be > 0");
        Self {
            chunk_size,
            buffer: Vec::new(),
        }
    }

    pub fn default_chunk_size() -> usize {
        4 * 1024 * 1024
    }
}

impl Chunker for FixedChunker {
    fn feed(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
        self.buffer.extend_from_slice(data);
        let mut out = Vec::new();

        while self.buffer.len() >= self.chunk_size {
            let chunk = self.buffer.drain(..self.chunk_size).collect();
            out.push(chunk);
        }

        Ok(out)
    }

    fn finish(&mut self) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![std::mem::take(&mut self.buffer)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_multiple_emits_all_chunks_on_feed() {
        let mut c = FixedChunker::new(3);
        let chunks = c.feed(b"abcdef").unwrap();
        assert_eq!(chunks, vec![b"abc", b"def"]);
        assert!(c.finish().unwrap().is_empty());
    }
}
