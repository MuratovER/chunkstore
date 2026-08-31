from __future__ import annotations

import io

import pytest

from chunkstore import AsyncChunkStore, ChunkStore, FilesystemBackend


def test_ingest_reader_streams(tmp_path) -> None:
    store = ChunkStore.open(FilesystemBackend(tmp_path / "chunks"))
    payload = b"stream-me-" + b"x" * 20_000
    store.ingest_reader("doc", io.BytesIO(payload), 4096)
    assert store.read("doc") == payload


def test_ingest_file_path_streams(tmp_path) -> None:
    src = tmp_path / "src.bin"
    payload = b"from-disk-" + b"y" * 12_000
    src.write_bytes(payload)
    store = ChunkStore.open(FilesystemBackend(tmp_path / "chunks"))
    store.ingest_file_path("doc", src)
    assert store.read("doc") == payload


@pytest.mark.asyncio
async def test_ingest_aiter_streams(tmp_path) -> None:
    async def blocks():
        yield b"async-"
        yield b"z" * 8_192

    async with await AsyncChunkStore.open(FilesystemBackend(tmp_path / "chunks")) as store:
        await store.ingest_aiter("doc", blocks(), chunk_size=1024)
        assert await store.read("doc") == b"async-" + b"z" * 8_192
