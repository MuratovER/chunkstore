from __future__ import annotations

from pathlib import Path
from typing import BinaryIO

from chunkstore.async_store import AsyncChunkStore


class AsyncChunkClient:
    """Async convenience client for upload/download/delete operations.

    Thin wrappers over :class:`AsyncChunkStore` with upload-oriented names.
    """

    def __init__(self, store: AsyncChunkStore) -> None:
        self.store = store

    async def upload_file(self, file_id: str, data: bytes) -> list[str]:
        return await self.store.ingest(file_id, data)

    async def upload_file_cdc(self, file_id: str, data: bytes) -> list[str]:
        return await self.store.ingest_cdc(file_id, data)

    async def upload_file_path(self, file_id: str, path: str | Path) -> list[str]:
        return await self.store.ingest_file_path(file_id, path)

    async def download_file(self, file_id: str) -> bytes:
        return await self.store.read(file_id)

    async def download_file_to(self, file_id: str, writer: BinaryIO) -> None:
        await self.store.read_to_writer(file_id, writer)

    async def delete_file(self, file_id: str) -> None:
        await self.store.delete(file_id)
