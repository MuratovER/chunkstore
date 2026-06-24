"""SQLite catalog for backup metadata and date-range queries.

chunkstore stores content-addressed chunks keyed by ``file_id`` (here:
``backup/{uuid}``). It has no ``created_at``, no labels, and no API to list
all files or filter by time — that metadata lives in this application layer.

Date search example::

    GET /backups?from=2025-06-01&to=2025-07-01

returns every backup with ``created_at`` in ``[2025-06-01, 2025-07-01)`` UTC.
``created_at`` is stored as ISO 8601 text so lexicographic order matches time.
"""

from __future__ import annotations

import sqlite3
from dataclasses import dataclass
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Literal

# How the blob was produced — needed to restore correctly on download.
BackupFormat = Literal["tar+gzip", "gzip"]
# Whether the backup came from HTTP upload or a directory scan.
BackupSource = Literal["upload", "directory"]

_SCHEMA = """
CREATE TABLE IF NOT EXISTS backups (
  id TEXT PRIMARY KEY,
  chunkstore_id TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  label TEXT,
  source TEXT NOT NULL,
  source_path TEXT,
  format TEXT NOT NULL,
  file_count INTEGER NOT NULL DEFAULT 1,
  uncompressed_bytes INTEGER NOT NULL,
  compressed_bytes INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_backups_created_at ON backups(created_at);
"""


@dataclass(frozen=True)
class BackupRecord:
    """One row in the catalog — maps a public ``id`` to a chunkstore ``file_id``."""

    id: str
    chunkstore_id: str
    created_at: str
    label: str | None
    source: BackupSource
    source_path: str | None
    format: BackupFormat
    file_count: int
    uncompressed_bytes: int
    compressed_bytes: int

    def to_dict(self) -> dict[str, object]:
        return {
            "id": self.id,
            "chunkstore_id": self.chunkstore_id,
            "created_at": self.created_at,
            "label": self.label,
            "source": self.source,
            "source_path": self.source_path,
            "format": self.format,
            "file_count": self.file_count,
            "uncompressed_bytes": self.uncompressed_bytes,
            "compressed_bytes": self.compressed_bytes,
        }


class BackupCatalog:
    """Persistent backup index stored in SQLite."""

    def __init__(self, db_path: str | Path) -> None:
        self._db_path = Path(db_path)
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        # check_same_thread=False: FastAPI may call handlers on different threads.
        self._conn = sqlite3.connect(self._db_path, check_same_thread=False)
        self._conn.row_factory = sqlite3.Row
        self._conn.executescript(_SCHEMA)

    def close(self) -> None:
        self._conn.close()

    def insert(self, record: BackupRecord) -> None:
        self._conn.execute(
            """
            INSERT INTO backups (
              id, chunkstore_id, created_at, label, source, source_path,
              format, file_count, uncompressed_bytes, compressed_bytes
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                record.id,
                record.chunkstore_id,
                record.created_at,
                record.label,
                record.source,
                record.source_path,
                record.format,
                record.file_count,
                record.uncompressed_bytes,
                record.compressed_bytes,
            ),
        )
        self._conn.commit()

    def get(self, backup_id: str) -> BackupRecord | None:
        row = self._conn.execute(
            "SELECT * FROM backups WHERE id = ?",
            (backup_id,),
        ).fetchone()
        return _row_to_record(row) if row else None

    def delete(self, backup_id: str) -> bool:
        cursor = self._conn.execute("DELETE FROM backups WHERE id = ?", (backup_id,))
        self._conn.commit()
        return cursor.rowcount > 0

    def list_by_date(
        self,
        *,
        from_value: str | None = None,
        to_value: str | None = None,
        limit: int = 50,
    ) -> list[BackupRecord]:
        """Return backups newest-first, optionally filtered by ``created_at`` range."""
        clauses: list[str] = []
        params: list[object] = []

        if from_value is not None:
            clauses.append("created_at >= ?")
            params.append(normalize_range_bound(from_value, inclusive_start=True))

        if to_value is not None:
            clauses.append("created_at < ?")
            params.append(normalize_range_bound(to_value, inclusive_start=False))
        else:
            # No upper bound given — only return backups up to now.
            clauses.append("created_at < ?")
            params.append(datetime.now(timezone.utc).isoformat())

        where = f"WHERE {' AND '.join(clauses)}" if clauses else ""
        query = f"""
            SELECT * FROM backups
            {where}
            ORDER BY created_at DESC
            LIMIT ?
        """
        params.append(limit)
        rows = self._conn.execute(query, params).fetchall()
        return [_row_to_record(row) for row in rows]


def normalize_range_bound(value: str, *, inclusive_start: bool) -> str:
    """Normalize a query ``from`` / ``to`` parameter to a UTC ISO string.

    Accepts either a date (``2025-06-01``) or full ISO datetime.

    For dates:
    - ``from`` (inclusive start): midnight UTC on that day.
    - ``to`` (exclusive end): midnight UTC on that day — so
      ``from=2025-06-01&to=2025-07-01`` selects all of June 2025.
    """
    if "T" in value:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=timezone.utc)
        return parsed.astimezone(timezone.utc).isoformat()

    parsed_date = date.fromisoformat(value)
    if inclusive_start:
        dt = datetime(
            parsed_date.year,
            parsed_date.month,
            parsed_date.day,
            tzinfo=timezone.utc,
        )
        return dt.isoformat()

    # Exclusive upper bound: ``to=2025-07-01`` means strictly before July 1st 00:00 UTC.
    dt = datetime(
        parsed_date.year,
        parsed_date.month,
        parsed_date.day,
        tzinfo=timezone.utc,
    )
    return dt.isoformat()


def _row_to_record(row: sqlite3.Row) -> BackupRecord:
    return BackupRecord(
        id=row["id"],
        chunkstore_id=row["chunkstore_id"],
        created_at=row["created_at"],
        label=row["label"],
        source=row["source"],  # type: ignore[arg-type]
        source_path=row["source_path"],
        format=row["format"],  # type: ignore[arg-type]
        file_count=row["file_count"],
        uncompressed_bytes=row["uncompressed_bytes"],
        compressed_bytes=row["compressed_bytes"],
    )
