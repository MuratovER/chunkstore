"""Tests for backup packaging helpers."""

from __future__ import annotations

import gzip
import io
import tarfile
from pathlib import Path

import pytest

from backup import (
    decompress_archive,
    directory_to_gzip_tar,
    is_gzip_archive,
    resolve_scan_path,
    upload_to_gzip,
)


def test_is_gzip_archive() -> None:
    assert is_gzip_archive("dump.tar.gz")
    assert is_gzip_archive("dump.tgz")
    assert not is_gzip_archive("notes.txt")


def test_upload_to_gzip_wraps_plain_file() -> None:
    compressed, fmt, uncompressed, count = upload_to_gzip("doc.txt", b"hello")
    assert fmt == "tar+gzip"
    assert uncompressed == 5
    assert count == 1

    tar_bytes = gzip.decompress(compressed)
    with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as archive:
        member = archive.getmember("doc.txt")
        extracted = archive.extractfile(member)
        assert extracted is not None
        assert extracted.read() == b"hello"


def test_upload_to_gzip_passes_through_archive() -> None:
    payload = gzip.compress(b"already compressed")
    compressed, fmt, uncompressed, count = upload_to_gzip("dump.tar.gz", payload)
    assert fmt == "gzip"
    assert compressed == payload
    assert uncompressed == len(payload)
    assert count == 1


def test_directory_to_gzip_tar(tmp_path: Path) -> None:
    docs = tmp_path / "docs"
    docs.mkdir()
    (docs / "a.txt").write_text("alpha")
    sub = docs / "sub"
    sub.mkdir()
    (sub / "b.txt").write_text("beta")

    compressed, uncompressed, count = directory_to_gzip_tar(docs)
    assert count == 2
    assert uncompressed == 9

    tar_bytes = gzip.decompress(compressed)
    with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as archive:
        names = {member.name for member in archive.getmembers()}
    assert names == {"a.txt", "sub/b.txt"}


def test_decompress_archive_tar_gzip() -> None:
    compressed, fmt, _, _ = upload_to_gzip("x.bin", b"payload")
    payload = decompress_archive(compressed, fmt)
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:") as archive:
        assert archive.getnames() == ["x.bin"]


def test_resolve_scan_path_rejects_escape(tmp_path: Path) -> None:
    root = tmp_path / "sources"
    root.mkdir()
    (root / "ok").mkdir()

    target = resolve_scan_path(root, "ok")
    assert target.name == "ok"

    with pytest.raises(ValueError, match="escapes"):
        resolve_scan_path(root, "../outside")

    with pytest.raises(ValueError, match="relative"):
        resolve_scan_path(root, "/abs/path")
