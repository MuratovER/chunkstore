from __future__ import annotations

import io

import pytest

from chunkstore import AsyncChunkStore, FilesystemBackend
from chunkstore.async_backend import AsyncBackend


class MemoryAsyncBackend:
    """In-memory async backend for unit tests."""

    def __init__(self) -> None:
        self._data: dict[str, bytes] = {}

    async def aget(self, key: str) -> bytes | None:
        return self._data.get(key)

    async def aput(self, key: str, data: bytes) -> None:
        self._data[key] = data

    async def aexists(self, key: str) -> bool:
        return key in self._data

    async def adelete(self, key: str) -> None:
        self._data.pop(key, None)


@pytest.mark.asyncio
async def test_memory_async_backend_roundtrip() -> None:
    backend = MemoryAsyncBackend()
    async with await AsyncChunkStore.open(backend) as store:
        digests = await store.ingest("doc", b"async-payload")
        assert len(digests) == 1
        assert await store.read("doc") == b"async-payload"


@pytest.mark.asyncio
async def test_memory_store_roundtrip() -> None:
    async with await AsyncChunkStore.memory() as store:
        await store.ingest("doc", b"memory-async")
        assert await store.read("doc") == b"memory-async"


@pytest.mark.asyncio
async def test_filesystem_backend_smoke(tmp_path) -> None:
    async with await AsyncChunkStore.open(FilesystemBackend(tmp_path / "chunks")) as store:
        await store.ingest("doc", b"fs-async")
        assert await store.read("doc") == b"fs-async"


@pytest.mark.asyncio
async def test_iter_chunks_matches_read(tmp_path) -> None:
    async with await AsyncChunkStore.open(FilesystemBackend(tmp_path / "chunks")) as store:
        payload = b"iter-" + b"z" * 8_192
        digests = await store.ingest_fixed("doc", payload, 2_048)
        assert len(digests) > 1
        chunks = [chunk async for chunk in store.iter_chunks("doc")]
        assert len(chunks) == len(digests)
        assert b"".join(chunks) == payload
        assert await store.read("doc") == payload


@pytest.mark.asyncio
async def test_read_to_writer(tmp_path) -> None:
    async with await AsyncChunkStore.open(FilesystemBackend(tmp_path / "chunks")) as store:
        payload = b"writer-" + b"w" * 4_096
        await store.ingest_fixed("doc", payload, 1_024)
        buffer = io.BytesIO()
        await store.read_to_writer("doc", buffer)
        assert buffer.getvalue() == payload


@pytest.mark.asyncio
async def test_delete_and_stats(tmp_path) -> None:
    async with await AsyncChunkStore.open(FilesystemBackend(tmp_path / "chunks")) as store:
        await store.ingest("a", b"shared")
        await store.ingest("b", b"shared")
        stats = await store.stats()
        assert stats.savings_pct > 0.0
        await store.delete("a")
        assert await store.read("b") == b"shared"


def test_async_backend_protocol() -> None:
    assert isinstance(MemoryAsyncBackend(), AsyncBackend)
