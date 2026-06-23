"""FastAPI upload/download/delete/stats example for chunkstore."""

from __future__ import annotations

import tempfile
from pathlib import Path

from fastapi import FastAPI, HTTPException, Response
from fastapi.responses import JSONResponse

from chunkstore import ChunkClient, ChunkStore, FilesystemBackend

DATA_DIR = Path(tempfile.gettempdir()) / "chunkstore-fastapi"
backend = FilesystemBackend(DATA_DIR / "chunks")
store = ChunkStore.open(backend)
client = ChunkClient(store)

app = FastAPI(title="chunkstore-fastapi-example")


@app.post("/files/{file_id}")
async def upload_file(file_id: str, body: bytes) -> JSONResponse:
    digests = client.upload_file(file_id, body)
    stats = store.stats()
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
        data = client.download_file(file_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return Response(content=data, media_type="application/octet-stream")


@app.delete("/files/{file_id}")
async def delete_file(file_id: str) -> JSONResponse:
    try:
        client.delete_file(file_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return JSONResponse({"deleted": file_id})


@app.get("/stats")
async def stats() -> JSONResponse:
    s = store.stats()
    return JSONResponse(
        {
            "total_bytes": s.total_bytes,
            "stored_bytes": s.stored_bytes,
            "savings_pct": s.savings_pct,
        }
    )
