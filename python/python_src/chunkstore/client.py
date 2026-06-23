from __future__ import annotations

from pathlib import Path

from chunkstore.store import ChunkStore


class ChunkClient:
    """Convenience client for upload/download/delete operations."""

    def __init__(self, store: ChunkStore) -> None:
        self.store = store

    def upload_file(self, file_id: str, data: bytes) -> list[str]:
        return self.store.ingest(file_id, data)

    def upload_file_cdc(self, file_id: str, data: bytes) -> list[str]:
        return self.store.ingest_cdc(file_id, data)

    def upload_file_path(self, file_id: str, path: str | Path) -> list[str]:
        return self.store.ingest_file_path(file_id, path)

    def download_file(self, file_id: str) -> bytes:
        return self.store.read(file_id)

    def delete_file(self, file_id: str) -> None:
        self.store.delete(file_id)
