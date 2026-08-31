mod cdc;
mod fixed;

pub use cdc::{chunk_cdc, CdcChunker, CdcConfig};
pub use fixed::FixedChunker;

use std::io::Read;

use crate::error::ChunkStoreError;

/// Streaming chunker interface.
pub trait Chunker {
    /// Feed the next block of input. Returns completed chunks.
    fn feed(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, ChunkStoreError>;

    /// Flush remaining buffered data as a final chunk (if any).
    fn finish(&mut self) -> Result<Vec<Vec<u8>>, ChunkStoreError>;
}

/// Stream chunks from `reader` into `on_chunk` without retaining all payloads.
pub fn for_each_chunk<R, F>(
    reader: &mut R,
    chunker: &mut dyn Chunker,
    mut on_chunk: F,
) -> Result<(), ChunkStoreError>
where
    R: Read,
    F: FnMut(Vec<u8>) -> Result<(), ChunkStoreError>,
{
    let mut buf = [0u8; 64 * 1024];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for chunk in chunker.feed(&buf[..n])? {
            on_chunk(chunk)?;
        }
    }

    for chunk in chunker.finish()? {
        on_chunk(chunk)?;
    }
    Ok(())
}

/// Chunk all bytes from `reader` using the given chunker.
pub fn chunk_reader<R: Read>(
    reader: &mut R,
    chunker: &mut dyn Chunker,
) -> Result<Vec<Vec<u8>>, ChunkStoreError> {
    let mut chunks = Vec::new();
    for_each_chunk(reader, chunker, |chunk| {
        chunks.push(chunk);
        Ok(())
    })?;
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_chunks_entire_buffer() {
        let mut chunker = FixedChunker::new(4);
        let chunks = chunker.feed(b"abcdefghij").unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], b"abcd");
        assert_eq!(chunks[1], b"efgh");
        let tail = chunker.finish().unwrap();
        assert_eq!(tail, vec![b"ij".to_vec()]);
    }
}
