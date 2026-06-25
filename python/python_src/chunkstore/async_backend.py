from __future__ import annotations

from typing import Protocol, runtime_checkable


@runtime_checkable
class AsyncBackend(Protocol):
    """Async key-value backend for chunk blobs and metadata."""

    async def aget(self, key: str) -> bytes | None: ...

    async def aput(self, key: str, data: bytes) -> None: ...

    async def aexists(self, key: str) -> bool: ...

    async def adelete(self, key: str) -> None: ...
