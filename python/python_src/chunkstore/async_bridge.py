from __future__ import annotations

import asyncio
import threading
from collections.abc import Coroutine
from typing import Any, TypeVar

from chunkstore.async_backend import AsyncBackend

_T = TypeVar("_T")


class AsyncBackendSyncAdapter:
    """Sync backend that delegates to an :class:`AsyncBackend` on a dedicated event-loop thread.

    Used by the Rust core (via PyO3) when the application backend is async. Rust calls
    sync ``get``/``put`` from a worker thread; this adapter forwards to ``aget``/``aput``
    via ``asyncio.run_coroutine_threadsafe`` on a separate loop thread to avoid deadlocks
    with the main asyncio loop (e.g. FastAPI / uvicorn).
    """

    def __init__(self, backend: AsyncBackend) -> None:
        self._backend = backend
        self._loop = asyncio.new_event_loop()
        self._thread = threading.Thread(target=self._run_loop, name="chunkstore-async-bridge", daemon=True)
        self._thread.start()

    def _run_loop(self) -> None:
        asyncio.set_event_loop(self._loop)
        self._loop.run_forever()

    def _run(self, coro: Coroutine[Any, Any, _T]) -> _T:
        if threading.current_thread() is self._thread:
            raise RuntimeError("AsyncBackendSyncAdapter cannot be called from its own loop thread")
        future = asyncio.run_coroutine_threadsafe(coro, self._loop)
        return future.result()

    def close(self) -> None:
        if not self._loop.is_running():
            return
        self._loop.call_soon_threadsafe(self._loop.stop)
        self._thread.join(timeout=5.0)

    def get(self, key: str) -> bytes | None:
        return self._run(self._backend.aget(key))

    def put(self, key: str, data: bytes) -> None:
        self._run(self._backend.aput(key, data))

    def exists(self, key: str) -> bool:
        return self._run(self._backend.aexists(key))

    def delete(self, key: str) -> None:
        self._run(self._backend.adelete(key))
