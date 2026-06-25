"""Content-addressed chunk storage with deduplication."""

from __future__ import annotations

from typing import TYPE_CHECKING

from chunkstore._native import ChunkStoreHandle, Stats
from chunkstore.async_backend import AsyncBackend
from chunkstore.async_client import AsyncChunkClient
from chunkstore.async_store import AsyncChunkStore
from chunkstore.backend import FilesystemBackend
from chunkstore.client import ChunkClient
from chunkstore.store import ChunkStore

if TYPE_CHECKING:
    from chunkstore.async_s3_backend import AsyncS3Backend
    from chunkstore.backend import S3Backend

__all__ = [
    "AsyncBackend",
    "AsyncChunkClient",
    "AsyncChunkStore",
    "AsyncS3Backend",
    "ChunkClient",
    "ChunkStore",
    "ChunkStoreHandle",
    "FilesystemBackend",
    "S3Backend",
    "Stats",
]

__version__ = "0.3.0"


def __getattr__(name: str) -> object:
    if name == "S3Backend":
        from chunkstore.backend import S3Backend

        return S3Backend
    if name == "AsyncS3Backend":
        from chunkstore.async_s3_backend import AsyncS3Backend

        return AsyncS3Backend
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(__all__))
