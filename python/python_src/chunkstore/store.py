from __future__ import annotations

from pathlib import Path
from typing import BinaryIO, cast

from chunkstore._native import ChunkStoreHandle, Stats


class ChunkStore:
    """High-level store handle backed by the Rust core.

  API aliases (see also :class:`ChunkClient`):

  +---------------------------+---------------------------+
  | ChunkStore                | ChunkClient               |
  +===========================+===========================+
  | ``ingest``                  | ``upload_file``           |
  | ``ingest_cdc``              | ``upload_file_cdc``       |
  | ``ingest_file_path``        | ``upload_file_path``      |
  | ``read``                    | ``download_file``         |
  | ``delete``                  | ``delete_file``           |
  +---------------------------+---------------------------+

  ``ingest_reader`` and ``ingest_file_path`` load the full input into memory
  before chunking (streaming ingest is planned for v0.3).
  """

    def __init__(self, handle: ChunkStoreHandle) -> None:
        self._handle = handle

    @classmethod
    def memory(cls) -> ChunkStore:
        return cls(ChunkStoreHandle.memory())

    @classmethod
    def open(cls, backend: object) -> ChunkStore:
        return cls(ChunkStoreHandle.open(backend))

    def ingest(self, file_id: str, data: bytes) -> list[str]:
        """Store bytes with default fixed chunking (4 MiB). Alias: ``ChunkClient.upload_file``."""
        return cast(list[str], self._handle.ingest(file_id, data))

    def ingest_cdc(self, file_id: str, data: bytes) -> list[str]:
        """Store bytes with content-defined chunking. Alias: ``ChunkClient.upload_file_cdc``."""
        return cast(list[str], self._handle.ingest_cdc(file_id, data))

    def ingest_fixed(self, file_id: str, data: bytes, chunk_size: int) -> list[str]:
        """Store bytes with a custom fixed chunk size."""
        return cast(list[str], self._handle.ingest_fixed(file_id, data, chunk_size))

    def ingest_file_path(self, file_id: str, path: str | Path) -> list[str]:
        """Read a file from disk and ingest (loads entire file into memory)."""
        data = Path(path).read_bytes()
        return self.ingest(file_id, data)

    def read(self, file_id: str) -> bytes:
        """Reconstruct file bytes. Alias: ``ChunkClient.download_file``."""
        return cast(bytes, self._handle.read(file_id))

    def delete(self, file_id: str) -> None:
        """Delete a file and GC unreferenced chunks. Alias: ``ChunkClient.delete_file``."""
        self._handle.delete(file_id)

    def stats(self) -> Stats:
        return self._handle.stats()

    def ingest_reader(self, file_id: str, reader: BinaryIO, chunk_size: int = 4 * 1024 * 1024) -> list[str]:
        """Read all bytes from ``reader`` and ingest with fixed chunking (not streaming)."""
        data = reader.read()
        return self.ingest_fixed(file_id, data, chunk_size)
