"""Content-addressed chunk storage with deduplication."""

from chunkstore._native import ChunkStoreHandle, Stats
from chunkstore.backend import FilesystemBackend, S3Backend
from chunkstore.client import ChunkClient
from chunkstore.store import ChunkStore

__all__ = [
    "ChunkClient",
    "ChunkStore",
    "ChunkStoreHandle",
    "FilesystemBackend",
    "S3Backend",
    "Stats",
]

__version__ = "0.1.1"
