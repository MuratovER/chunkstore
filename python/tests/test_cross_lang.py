from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

from chunkstore import ChunkStore, FilesystemBackend

FILE_ID = "cross_lang_doc"
REPO_ROOT = Path(__file__).resolve().parents[2]
GO_PKG_DIR = REPO_ROOT / "go" / "chunkstore"


def _payload() -> bytes:
    return bytes((i * 0x9E3779B9) & 0xFF for i in range(8 * 1024))


@pytest.mark.cross_lang
def test_python_write_go_read_delete_python_stats(tmp_path: Path) -> None:
    if shutil.which("go") is None:
        pytest.skip("go toolchain not found")

    root = tmp_path / "chunks"
    backend = FilesystemBackend(root)
    store = ChunkStore.open(backend)

    payload = _payload()
    digests = store.ingest(FILE_ID, payload)
    stats = store.stats()

    sidecar = root / ".cross_lang"
    sidecar.mkdir()
    (sidecar / "expected.bin").write_bytes(payload)
    (sidecar / "stats.json").write_text(
        json.dumps(
            {
                "total_bytes": stats.total_bytes,
                "stored_bytes": stats.stored_bytes,
                "savings_pct": stats.savings_pct,
            }
        ),
        encoding="utf-8",
    )

    del store

    env = os.environ.copy()
    env["CHUNKSTORE_CROSS_ROOT"] = str(root)
    env["CHUNKSTORE_FILE_ID"] = FILE_ID
    subprocess.run(
        [
            "go",
            "test",
            "-run",
            "^TestCrossLangReadDelete$",
            "-count=1",
            "-v",
        ],
        cwd=GO_PKG_DIR,
        env=env,
        check=True,
    )

    store2 = ChunkStore.open(FilesystemBackend(root))
    stats_after = store2.stats()
    assert stats_after.total_bytes == 0
    assert stats_after.stored_bytes == 0

    with pytest.raises(OSError):
        store2.read(FILE_ID)

    for digest in digests:
        assert not (root / digest).exists()
