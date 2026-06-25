# FastAPI backup storage example

Demonstrates an **application pattern** (not a Restic/Borg replacement):

- **chunkstore** stores deduplicated gzip-compressed backup blobs
- **SQLite catalog** stores metadata and supports **date-range search**

chunkstore has no `created_at` or `list files` API — the catalog is your app's responsibility.

## Architecture

```
HTTP upload / directory scan
        │
        ▼
  backup.py ──► tar.gz bytes
        │
        ├──────────────────────────┐
        ▼                          ▼
  chunkstore.ingest()        catalog.insert()
  (dedup blob storage)       (id, created_at, format, …)
        │                          │
        └──────── read/delete ◄────┘
```

| Module | Role |
|--------|------|
| [`main.py`](main.py) | FastAPI routes, env config, orchestration (sync `ChunkStore`; async migration TODO) |
| [`backup.py`](backup.py) | Pack directory/upload into gzip; decompress on download |
| [`catalog.py`](catalog.py) | SQLite index; **date-range queries** (`list_by_date`) |

**Two backup sources**

1. **Upload** — client sends a file; plain files are wrapped in tar.gz, pre-compressed `.tar.gz` uploads are stored as-is.
2. **Scan** — server packs a directory under `BACKUP_SOURCE_ROOT` (relative path only, no path traversal).

**Date search** — `GET /backups?from=2025-06-01&to=2025-07-01` queries SQLite, not chunkstore. The `to` date is exclusive: the example above returns all of June 2025 UTC.

## Run

```bash
cd python
maturin develop --release
pip install ".[fastapi]"

# from repository root
PYTHONPATH=examples/fastapi-backup uvicorn main:app --host 0.0.0.0 --port 8081 --reload
```

Environment (optional):

| Variable | Default |
|----------|---------|
| `CHUNKSTORE_DATA_DIR` | `$TMPDIR/chunkstore-backup-data` |
| `BACKUP_CATALOG_PATH` | `{DATA_DIR}/catalog.db` |
| `BACKUP_SOURCE_ROOT` | `{DATA_DIR}/sources` |
| `MAX_UPLOAD_BYTES` | `64000000` (64 MiB) |
| `PORT` | `8081` |

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/backups/upload` | Multipart upload → tar.gz (or as-is for `.tar.gz`) |
| `POST` | `/backups/scan` | Scan relative path under `BACKUP_SOURCE_ROOT` |
| `GET` | `/backups?from=&to=` | List backups in date range |
| `GET` | `/backups/{id}` | Metadata |
| `GET` | `/backups/{id}/archive` | Compressed blob from chunkstore |
| `GET` | `/backups/{id}/download` | Decompressed tar (or raw for gzip uploads) |
| `DELETE` | `/backups/{id}` | Delete blob + catalog entry |
| `GET` | `/stats` | chunkstore dedup metrics |

## curl examples

```bash
# Upload a file
curl -X POST http://localhost:8081/backups/upload \
  -F "file=@document.pdf" -F "label=nightly"

# Prepare a directory scan target
mkdir -p /tmp/chunkstore-backup-data/sources/docs
echo "hello" > /tmp/chunkstore-backup-data/sources/docs/note.txt

curl -X POST http://localhost:8081/backups/scan \
  -H "Content-Type: application/json" \
  -d '{"path":"docs","label":"docs-snapshot"}'

# List backups for June 2025
curl "http://localhost:8081/backups?from=2025-06-01&to=2025-07-01"

# Download compressed archive
curl -o backup.tar.gz "http://localhost:8081/backups/{id}/archive"

# Download decompressed tar
curl -o backup.tar "http://localhost:8081/backups/{id}/download"

curl http://localhost:8081/stats
```

## Tests

```bash
cd python
pytest tests/test_backup_catalog.py -q
PYTHONPATH=../examples/fastapi-backup pytest ../examples/fastapi-backup/test_backup.py -q
```

## Notes

- **Demo only:** `store.ingest` loads full blobs in memory — not for multi-GB backups without streaming (see roadmap v0.3).
- **Dedup:** Re-uploading an identical dump reuses chunks (~100% savings). Unique gzip dumps rarely deduplicate across files.
- **Date search:** Implemented in SQLite (`created_at`), not in chunkstore.
