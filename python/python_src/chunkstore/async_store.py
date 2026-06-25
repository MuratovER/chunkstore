from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path
from typing import BinaryIO

from chunkstore._native import Stats
from chunkstore.async_bridge import AsyncBackendSyncAdapter
from chunkstore.store import ChunkStore


def _is_async_backend(backend: object) -> bool:
    return callable(getattr(backend, "aget", None))


def _is_fs_backend(backend: object) -> bool:
    return hasattr(backend, "root")


class AsyncChunkStore:
    """Async wrapper around :class:`ChunkStore` for asyncio services (FastAPI, etc.).

    Sync backends (``FilesystemBackend``, ``S3Backend``) run in ``asyncio.to_thread`` so
    Rust chunking does not block the event loop. Async backends (``AsyncS3Backend``) use
    a dedicated bridge thread for Rust ↔ async I/O.
    """

    def __init__(
        self,
        store: ChunkStore,
        *,
        adapter: AsyncBackendSyncAdapter | None = None,
        backend: object | None = None,
    ) -> None:
        self._store = store
        self._adapter = adapter
        self._backend = backend

    @classmethod
    async def memory(cls) -> AsyncChunkStore:
        store = await asyncio.to_thread(ChunkStore.memory)
        return cls(store)

    @classmethod
    async def open(cls, backend: object) -> AsyncChunkStore:
        adapter: AsyncBackendSyncAdapter | None = None
        open_backend: object = backend

        if _is_fs_backend(backend):
            pass
        elif _is_async_backend(backend):
            if hasattr(backend, "start"):
                await backend.start()
            adapter = AsyncBackendSyncAdapter(backend)  # type: ignore[arg-type]
            open_backend = adapter
        elif hasattr(backend, "start"):
            await backend.start()

        store = await asyncio.to_thread(ChunkStore.open, open_backend)
        return cls(store, adapter=adapter, backend=backend)

    @classmethod
    @asynccontextmanager
    async def open_ctx(cls, backend: object) -> AsyncIterator[AsyncChunkStore]:
        store = await cls.open(backend)
        try:
            yield store
        finally:
            await store.aclose()

    async def __aenter__(self) -> AsyncChunkStore:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.aclose()

    async def aclose(self) -> None:
        if self._adapter is not None:
            self._adapter.close()
            self._adapter = None
        if self._backend is not None and hasattr(self._backend, "aclose"):
            await self._backend.aclose()
        self._backend = None

    async def ingest(self, file_id: str, data: bytes) -> list[str]:
        return await asyncio.to_thread(self._store.ingest, file_id, data)

    async def ingest_cdc(self, file_id: str, data: bytes) -> list[str]:
        return await asyncio.to_thread(self._store.ingest_cdc, file_id, data)

    async def ingest_fixed(self, file_id: str, data: bytes, chunk_size: int) -> list[str]:
        return await asyncio.to_thread(self._store.ingest_fixed, file_id, data, chunk_size)

    async def ingest_file_path(self, file_id: str, path: str | Path) -> list[str]:
        return await asyncio.to_thread(self._store.ingest_file_path, file_id, path)

    async def read(self, file_id: str) -> bytes:
        return await asyncio.to_thread(self._store.read, file_id)

    async def read_to_writer(self, file_id: str, writer: BinaryIO) -> None:
        await asyncio.to_thread(self._store.read_to_writer, file_id, writer)

    async def read_to_path(self, file_id: str, path: str | Path) -> None:
        await asyncio.to_thread(self._store.read_to_path, file_id, path)

    async def iter_chunks(self, file_id: str) -> AsyncIterator[bytes]:
        digests = await asyncio.to_thread(self._store.file_digests, file_id)
        for digest in digests:
            yield await asyncio.to_thread(self._store.read_chunk, digest)

    async def delete(self, file_id: str) -> None:
        await asyncio.to_thread(self._store.delete, file_id)

    async def stats(self) -> Stats:
        return await asyncio.to_thread(self._store.stats)

    async def ingest_reader(
        self,
        file_id: str,
        reader: BinaryIO,
        chunk_size: int = 4 * 1024 * 1024,
    ) -> list[str]:
        return await asyncio.to_thread(self._store.ingest_reader, file_id, reader, chunk_size)
