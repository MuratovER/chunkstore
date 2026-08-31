from __future__ import annotations

from collections.abc import Iterator
from pathlib import Path
from typing import BinaryIO, Protocol, cast

from chunkstore._native import ChunkStoreHandle, Stats


class SupportsRead(Protocol):
    def read(self, n: int = -1) -> bytes: ...


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
  | ``read_to_writer``          | ``download_file_to``      |
  | ``iter_chunks``             | —                         |
  | ``delete``                  | ``delete_file``           |
  +---------------------------+---------------------------+

  ``ingest_reader`` and ``ingest_file_path`` stream from the
  source — they do not load the whole file into Python memory.
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
        """Stream a file from disk and ingest with default fixed chunking."""
        return cast(list[str], self._handle.ingest_path(file_id, str(path)))

    def ingest_file_path_cdc(self, file_id: str, path: str | Path) -> list[str]:
        """Stream a file from disk and ingest with CDC chunking."""
        return cast(list[str], self._handle.ingest_path_cdc(file_id, str(path)))

    def read(self, file_id: str) -> bytes:
        """Reconstruct file bytes. Alias: ``ChunkClient.download_file``."""
        return cast(bytes, self._handle.read(file_id))

    def read_to_writer(self, file_id: str, writer: BinaryIO) -> None:
        """Stream verified chunk payloads to ``writer`` without holding the full file in memory."""
        self._handle.read_to_writer(file_id, writer)

    def read_to_path(self, file_id: str, path: str | Path) -> None:
        """Stream a stored file to a filesystem path."""
        with Path(path).open("wb") as writer:
            self.read_to_writer(file_id, writer)

    def iter_chunks(self, file_id: str) -> Iterator[bytes]:
        """Yield verified chunk payloads one at a time."""
        for digest in self.file_digests(file_id):
            yield self.read_chunk(digest)

    def file_digests(self, file_id: str) -> list[str]:
        """Ordered chunk digests for a stored file."""
        return cast(list[str], self._handle.file_digests(file_id))

    def read_chunk(self, digest: str) -> bytes:
        """Fetch and verify a single chunk payload by digest."""
        return cast(bytes, self._handle.read_chunk(digest))

    def delete(self, file_id: str) -> None:
        """Delete a file and GC unreferenced chunks. Alias: ``ChunkClient.delete_file``."""
        self._handle.delete(file_id)

    def stats(self) -> Stats:
        return self._handle.stats()

    def ingest_reader(
        self, file_id: str, reader: SupportsRead, chunk_size: int = 4 * 1024 * 1024
    ) -> list[str]:
        """Stream bytes from ``reader`` and ingest with fixed chunking."""
        return cast(list[str], self._handle.ingest_reader(file_id, reader, chunk_size))

    def ingest_reader_cdc(self, file_id: str, reader: SupportsRead) -> list[str]:
        """Stream bytes from ``reader`` and ingest with CDC chunking."""
        return cast(list[str], self._handle.ingest_reader_cdc(file_id, reader))
