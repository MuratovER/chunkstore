from __future__ import annotations

import io

from chunkstore import ChunkStore, FilesystemBackend


def test_read_to_writer_matches_read(tmp_path) -> None:
    store = ChunkStore.open(FilesystemBackend(tmp_path / "chunks"))
    payload = b"streaming-read-" + b"x" * 10_000
    chunk_size = 4_096
    store.ingest_fixed("doc", payload, chunk_size)

    buffer = io.BytesIO()
    store.read_to_writer("doc", buffer)
    assert buffer.getvalue() == payload
    assert store.read("doc") == payload


def test_iter_chunks_reassembles_file(tmp_path) -> None:
    store = ChunkStore.open(FilesystemBackend(tmp_path / "chunks"))
    payload = b"chunk-iter-" + b"y" * 8_192
    digests = store.ingest_fixed("doc", payload, 2_048)
    assert len(digests) > 1

    chunks = list(store.iter_chunks("doc"))
    assert len(chunks) == len(digests)
    assert b"".join(chunks) == payload


def test_read_to_path(tmp_path) -> None:
    store = ChunkStore.open(FilesystemBackend(tmp_path / "chunks"))
    payload = b"to-disk"
    store.ingest("doc", payload)

    out_path = tmp_path / "out.bin"
    store.read_to_path("doc", out_path)
    assert out_path.read_bytes() == payload
