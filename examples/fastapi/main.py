"""FastAPI upload/download/delete/stats example for chunkstore (async API)."""

from __future__ import annotations

import tempfile
from contextlib import asynccontextmanager
from pathlib import Path
from typing import AsyncIterator

from fastapi import FastAPI, HTTPException, Response
from fastapi.responses import JSONResponse

from chunkstore import AsyncChunkClient, AsyncChunkStore, FilesystemBackend

DATA_DIR = Path(tempfile.gettempdir()) / "chunkstore-fastapi"
backend = FilesystemBackend(DATA_DIR / "chunks")
store: AsyncChunkStore
client: AsyncChunkClient


@asynccontextmanager
async def lifespan(_app: FastAPI) -> AsyncIterator[None]:
    global store, client
    store = await AsyncChunkStore.open(backend)
    client = AsyncChunkClient(store)
    try:
        yield
    finally:
        await store.aclose()


app = FastAPI(title="chunkstore-fastapi-example", lifespan=lifespan)


@app.post("/files/{file_id}")
async def upload_file(file_id: str, body: bytes) -> JSONResponse:
    digests = await client.upload_file(file_id, body)
    stats = await store.stats()
    return JSONResponse(
        {
            "file_id": file_id,
            "digests": digests,
            "stats": {
                "total_bytes": stats.total_bytes,
                "stored_bytes": stats.stored_bytes,
                "savings_pct": stats.savings_pct,
            },
        }
    )


@app.get("/files/{file_id}")
async def download_file(file_id: str) -> Response:
    try:
        data = await client.download_file(file_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return Response(content=data, media_type="application/octet-stream")


@app.delete("/files/{file_id}")
async def delete_file(file_id: str) -> JSONResponse:
    try:
        await client.delete_file(file_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return JSONResponse({"deleted": file_id})


@app.get("/stats")
async def stats() -> JSONResponse:
    s = await store.stats()
    return JSONResponse(
        {
            "total_bytes": s.total_bytes,
            "stored_bytes": s.stored_bytes,
            "savings_pct": s.savings_pct,
        }
    )
