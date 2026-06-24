"""FastAPI backup storage example: gzip dumps in chunkstore + SQLite date catalog.

Architecture (application pattern, not a Restic/Borg replacement)::

    upload / directory scan
            → tar.gz bytes
            → chunkstore.ingest("backup/{uuid}", compressed)   # dedup layer
            → catalog.insert(id, created_at, format, …)        # search by date

chunkstore holds blobs; SQLite holds *when* and *what* each backup was.
See README.md for curl examples and env vars.
"""

from __future__ import annotations

import os
import tempfile
import uuid
from datetime import datetime, timezone
from pathlib import Path

from fastapi import FastAPI, File, Form, HTTPException, Query, Response, UploadFile
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field

from backup import decompress_archive, directory_to_gzip_tar, resolve_scan_path, upload_to_gzip
from catalog import BackupCatalog, BackupRecord
from chunkstore import ChunkStore, FilesystemBackend

# --- Paths (override via env for local dev / tests) ---
# Suffix ``-data`` avoids colliding with a binary named like the example dir.
DATA_DIR = Path(os.environ.get("CHUNKSTORE_DATA_DIR", Path(tempfile.gettempdir()) / "chunkstore-backup-data"))
CATALOG_PATH = Path(os.environ.get("BACKUP_CATALOG_PATH", DATA_DIR / "catalog.db"))
SOURCE_ROOT = Path(os.environ.get("BACKUP_SOURCE_ROOT", DATA_DIR / "sources"))
MAX_UPLOAD_BYTES = int(os.environ.get("MAX_UPLOAD_BYTES", "64000000"))

DATA_DIR.mkdir(parents=True, exist_ok=True)
SOURCE_ROOT.mkdir(parents=True, exist_ok=True)

# chunkstore: chunks under DATA_DIR/chunks, manifests under _manifest/
backend = FilesystemBackend(DATA_DIR / "chunks")
store = ChunkStore.open(backend)
catalog = BackupCatalog(CATALOG_PATH)

app = FastAPI(title="chunkstore-fastapi-backup-example")


class ScanRequest(BaseModel):
    """Body for POST /backups/scan — path is relative to BACKUP_SOURCE_ROOT."""

    path: str = Field(..., min_length=1, description="Relative path under BACKUP_SOURCE_ROOT")
    label: str | None = None


def _stats_payload() -> dict[str, float | int]:
    stats = store.stats()
    return {
        "total_bytes": stats.total_bytes,
        "stored_bytes": stats.stored_bytes,
        "savings_pct": stats.savings_pct,
    }


def _persist_backup(
    *,
    compressed: bytes,
    label: str | None,
    source: str,
    source_path: str | None,
    fmt: str,
    file_count: int,
    uncompressed_bytes: int,
) -> dict[str, object]:
    """Write compressed blob to chunkstore, then register metadata in SQLite.

    Order matters: ingest first, catalog second. On catalog failure we delete
    the chunkstore entry to avoid orphaned blobs with no catalog row.
    """
    backup_id = str(uuid.uuid4())
    chunkstore_id = f"backup/{backup_id}"
    created_at = datetime.now(timezone.utc).isoformat()

    store.ingest(chunkstore_id, compressed)
    record = BackupRecord(
        id=backup_id,
        chunkstore_id=chunkstore_id,
        created_at=created_at,
        label=label,
        source=source,  # type: ignore[arg-type]
        source_path=source_path,
        format=fmt,  # type: ignore[arg-type]
        file_count=file_count,
        uncompressed_bytes=uncompressed_bytes,
        compressed_bytes=len(compressed),
    )
    try:
        catalog.insert(record)
    except Exception as exc:
        # Roll back blob if metadata write failed.
        try:
            store.delete(chunkstore_id)
        except OSError:
            pass
        raise HTTPException(status_code=500, detail=f"catalog insert failed: {exc}") from exc

    return {
        "id": backup_id,
        "created_at": created_at,
        "format": fmt,
        "compressed_bytes": len(compressed),
        "stats": _stats_payload(),
    }


@app.post("/backups/upload")
async def upload_backup(
    file: UploadFile = File(...),
    label: str | None = Form(default=None),
) -> JSONResponse:
    """Create a backup from an uploaded file (multipart form field ``file``)."""
    data = await file.read()
    if len(data) > MAX_UPLOAD_BYTES:
        raise HTTPException(status_code=413, detail=f"upload exceeds {MAX_UPLOAD_BYTES} bytes")

    filename = file.filename or "upload.bin"
    compressed, fmt, uncompressed_bytes, file_count = upload_to_gzip(filename, data)
    payload = _persist_backup(
        compressed=compressed,
        label=label,
        source="upload",
        source_path=filename,
        fmt=fmt,
        file_count=file_count,
        uncompressed_bytes=uncompressed_bytes,
    )
    return JSONResponse(payload)


@app.post("/backups/scan")
async def scan_backup(body: ScanRequest) -> JSONResponse:
    """Create a backup by packing a directory under BACKUP_SOURCE_ROOT."""
    try:
        target = resolve_scan_path(SOURCE_ROOT, body.path)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    compressed, uncompressed_bytes, file_count = directory_to_gzip_tar(target)
    if file_count == 0:
        raise HTTPException(status_code=400, detail="directory is empty")
    if len(compressed) > MAX_UPLOAD_BYTES:
        raise HTTPException(status_code=413, detail=f"backup exceeds {MAX_UPLOAD_BYTES} bytes")

    payload = _persist_backup(
        compressed=compressed,
        label=body.label,
        source="directory",
        source_path=body.path,
        fmt="tar+gzip",
        file_count=file_count,
        uncompressed_bytes=uncompressed_bytes,
    )
    return JSONResponse(payload)


@app.get("/backups")
async def list_backups(
    from_date: str | None = Query(default=None, alias="from"),
    to_date: str | None = Query(default=None, alias="to"),
    limit: int = Query(default=50, ge=1, le=500),
) -> JSONResponse:
    """List backups, optionally filtered by ``created_at`` (catalog, not chunkstore)."""
    records = catalog.list_by_date(from_value=from_date, to_value=to_date, limit=limit)
    return JSONResponse({"backups": [record.to_dict() for record in records]})


@app.get("/backups/{backup_id}")
async def get_backup(backup_id: str) -> JSONResponse:
    """Return catalog metadata for one backup (no blob data)."""
    record = catalog.get(backup_id)
    if record is None:
        raise HTTPException(status_code=404, detail="backup not found")
    return JSONResponse(record.to_dict())


@app.get("/backups/{backup_id}/archive")
async def download_archive(backup_id: str) -> Response:
    """Download the compressed blob exactly as stored in chunkstore."""
    record = catalog.get(backup_id)
    if record is None:
        raise HTTPException(status_code=404, detail="backup not found")
    try:
        data = store.read(record.chunkstore_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return Response(content=data, media_type="application/gzip")


@app.get("/backups/{backup_id}/download")
async def download_backup(backup_id: str) -> Response:
    """Download after one gzip decompress (tar or raw inner file)."""
    record = catalog.get(backup_id)
    if record is None:
        raise HTTPException(status_code=404, detail="backup not found")
    try:
        compressed = store.read(record.chunkstore_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc

    payload = decompress_archive(compressed, record.format)
    media_type = "application/x-tar" if record.format == "tar+gzip" else "application/octet-stream"
    return Response(content=payload, media_type=media_type)


@app.delete("/backups/{backup_id}")
async def delete_backup(backup_id: str) -> JSONResponse:
    """Remove blob from chunkstore (with refcount GC), then drop catalog row."""
    record = catalog.get(backup_id)
    if record is None:
        raise HTTPException(status_code=404, detail="backup not found")
    try:
        # Blob first: if this fails, keep catalog so the id remains discoverable.
        store.delete(record.chunkstore_id)
    except OSError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    catalog.delete(backup_id)
    return JSONResponse({"deleted": backup_id})


@app.get("/stats")
async def stats() -> JSONResponse:
    """chunkstore dedup ratio across all stored backup blobs."""
    return JSONResponse(_stats_payload())
