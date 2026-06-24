"""Tests for the FastAPI backup example SQLite catalog."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

EXAMPLE_DIR = Path(__file__).resolve().parents[2] / "examples" / "fastapi-backup"
sys.path.insert(0, str(EXAMPLE_DIR))

from catalog import BackupCatalog, BackupRecord, normalize_range_bound  # noqa: E402


@pytest.fixture
def catalog(tmp_path: Path) -> BackupCatalog:
    cat = BackupCatalog(tmp_path / "catalog.db")
    yield cat
    cat.close()


def _record(
    backup_id: str,
    created_at: str,
    *,
    chunkstore_id: str | None = None,
) -> BackupRecord:
    return BackupRecord(
        id=backup_id,
        chunkstore_id=chunkstore_id or f"backup/{backup_id}",
        created_at=created_at,
        label=None,
        source="upload",
        source_path="file.bin",
        format="tar+gzip",
        file_count=1,
        uncompressed_bytes=10,
        compressed_bytes=8,
    )


def test_insert_and_get(catalog: BackupCatalog) -> None:
    record = _record("id-1", "2025-06-15T12:00:00+00:00")
    catalog.insert(record)
    got = catalog.get("id-1")
    assert got is not None
    assert got.chunkstore_id == "backup/id-1"


def test_list_by_date_range(catalog: BackupCatalog) -> None:
    catalog.insert(_record("a", "2025-06-01T10:00:00+00:00"))
    catalog.insert(_record("b", "2025-06-15T10:00:00+00:00"))
    catalog.insert(_record("c", "2025-07-01T10:00:00+00:00"))

    results = catalog.list_by_date(from_value="2025-06-01", to_value="2025-07-01", limit=10)
    ids = {record.id for record in results}
    assert ids == {"b", "a"}


def test_delete(catalog: BackupCatalog) -> None:
    catalog.insert(_record("gone", "2025-06-01T10:00:00+00:00"))
    assert catalog.delete("gone") is True
    assert catalog.get("gone") is None
    assert catalog.delete("gone") is False


def test_normalize_range_bound_date() -> None:
    start = normalize_range_bound("2025-06-01", inclusive_start=True)
    end = normalize_range_bound("2025-06-02", inclusive_start=False)
    assert start == "2025-06-01T00:00:00+00:00"
    assert end == "2025-06-02T00:00:00+00:00"


def test_normalize_range_bound_datetime() -> None:
    value = normalize_range_bound("2025-06-01T08:30:00Z", inclusive_start=True)
    assert value.startswith("2025-06-01T08:30:00")


def test_list_orders_newest_first(catalog: BackupCatalog) -> None:
    catalog.insert(_record("old", "2025-06-01T10:00:00+00:00"))
    catalog.insert(_record("new", "2025-06-02T10:00:00+00:00"))
    results = catalog.list_by_date(
        from_value="2025-06-01",
        to_value="2025-06-03",
        limit=10,
    )
    assert [record.id for record in results] == ["new", "old"]
