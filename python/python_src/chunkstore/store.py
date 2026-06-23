from __future__ import annotations

from pathlib import Path
from typing import BinaryIO

from chunkstore._native import ChunkStoreHandle


class ChunkStore:
    """High-level store handle backed by the Rust core."""

    def __init__(self, handle: ChunkStoreHandle) -> None:
        self._handle = handle

    @classmethod
    def memory(cls) -> ChunkStore:
        return cls(ChunkStoreHandle.memory())

    @classmethod
    def open(cls, backend: object) -> ChunkStore:
        return cls(ChunkStoreHandle.open(backend))

    def ingest(self, file_id: str, data: bytes) -> list[str]:
        return self._handle.ingest(file_id, data)

    def ingest_cdc(self, file_id: str, data: bytes) -> list[str]:
        return self._handle.ingest_cdc(file_id, data)

    def ingest_fixed(self, file_id: str, data: bytes, chunk_size: int) -> list[str]:
        return self._handle.ingest_fixed(file_id, data, chunk_size)

    def ingest_file_path(self, file_id: str, path: str | Path) -> list[str]:
        data = Path(path).read_bytes()
        return self.ingest(file_id, data)

    def read(self, file_id: str) -> bytes:
        return self._handle.read(file_id)

    def delete(self, file_id: str) -> None:
        self._handle.delete(file_id)

    def stats(self):
        return self._handle.stats()

    def ingest_reader(self, file_id: str, reader: BinaryIO, chunk_size: int = 4 * 1024 * 1024) -> list[str]:
        data = reader.read()
        return self.ingest_fixed(file_id, data, chunk_size)
