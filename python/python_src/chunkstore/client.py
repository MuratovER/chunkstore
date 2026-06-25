from __future__ import annotations

from pathlib import Path

from typing import BinaryIO

from chunkstore.store import ChunkStore


class ChunkClient:
    """Convenience client for upload/download/delete operations.

  Thin wrappers over :class:`ChunkStore` with upload-oriented names:

  - ``upload_file`` → ``ChunkStore.ingest``
  - ``upload_file_cdc`` → ``ChunkStore.ingest_cdc``
  - ``upload_file_path`` → ``ChunkStore.ingest_file_path``
  - ``download_file`` → ``ChunkStore.read``
  - ``delete_file`` → ``ChunkStore.delete``
  """

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

    def download_file_to(self, file_id: str, writer: BinaryIO) -> None:
        self.store.read_to_writer(file_id, writer)

    def delete_file(self, file_id: str) -> None:
        self.store.delete(file_id)
