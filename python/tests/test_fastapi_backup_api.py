"""Integration smoke test for examples/fastapi-backup (requires fastapi + httpx)."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

pytest.importorskip("fastapi")
pytest.importorskip("httpx")
from fastapi.testclient import TestClient  # noqa: E402

EXAMPLE_DIR = Path(__file__).resolve().parents[2] / "examples" / "fastapi-backup"


@pytest.fixture
def backup_client(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> TestClient:
    """Import main after pointing env at an isolated temp data dir."""
    data = tmp_path / "data"
    sources = data / "sources" / "docs"
    sources.mkdir(parents=True)
    (sources / "note.txt").write_text("hello")

    monkeypatch.setenv("CHUNKSTORE_DATA_DIR", str(data))
    monkeypatch.setenv("BACKUP_CATALOG_PATH", str(data / "catalog.db"))
    monkeypatch.setenv("BACKUP_SOURCE_ROOT", str(data / "sources"))

    sys.path.insert(0, str(EXAMPLE_DIR))
    if "main" in sys.modules:
        del sys.modules["main"]
    if "catalog" in sys.modules:
        del sys.modules["catalog"]
    if "backup" in sys.modules:
        del sys.modules["backup"]

    import main  # noqa: E402

    return TestClient(main.app)


def test_upload_scan_list_download_delete(backup_client: TestClient) -> None:
    upload = backup_client.post(
        "/backups/upload",
        files={"file": ("doc.txt", b"payload", "text/plain")},
        data={"label": "nightly"},
    )
    assert upload.status_code == 200
    backup_id = upload.json()["id"]

    scan = backup_client.post("/backups/scan", json={"path": "docs", "label": "docs"})
    assert scan.status_code == 200

    listed = backup_client.get("/backups", params={"from": "2020-01-01", "to": "2030-01-01"})
    assert listed.status_code == 200
    assert len(listed.json()["backups"]) == 2

    archive = backup_client.get(f"/backups/{backup_id}/archive")
    assert archive.status_code == 200
    assert len(archive.content) > 0

    download = backup_client.get(f"/backups/{backup_id}/download")
    assert download.status_code == 200
    assert len(download.content) > 0

    deleted = backup_client.delete(f"/backups/{backup_id}")
    assert deleted.status_code == 200
    assert backup_client.get(f"/backups/{backup_id}").status_code == 404


def test_scan_rejects_empty_directory(backup_client: TestClient) -> None:
    import os
    from pathlib import Path

    root = Path(os.environ["BACKUP_SOURCE_ROOT"])
    (root / "empty").mkdir(exist_ok=True)
    response = backup_client.post("/backups/scan", json={"path": "empty"})
    assert response.status_code == 400


def test_scan_rejects_path_escape(backup_client: TestClient) -> None:
    response = backup_client.post("/backups/scan", json={"path": "../outside"})
    assert response.status_code == 400
