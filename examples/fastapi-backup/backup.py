"""Backup packaging helpers: tar.gz creation and restore.

chunkstore receives the **compressed** bytes. Identical dumps share chunks
(reference-count dedup); unique gzip blobs rarely deduplicate across backups.

Two on-disk formats (see ``BackupFormat`` in catalog.py):

- ``tar+gzip`` — directory scan or plain file upload wrapped in tar.gz.
- ``gzip``     — client uploaded an already-compressed ``.tar.gz`` / ``.tgz``;
  stored as-is to avoid double compression.
"""

from __future__ import annotations

import gzip
import io
import tarfile
from pathlib import Path

from catalog import BackupFormat

GZIP_ARCHIVE_SUFFIXES = (".tar.gz", ".tgz", ".gz")


def is_gzip_archive(filename: str) -> bool:
    """True when the upload already looks like a gzip-compressed archive."""
    lowered = filename.lower()
    return any(lowered.endswith(suffix) for suffix in GZIP_ARCHIVE_SUFFIXES)


def directory_to_gzip_tar(path: Path) -> tuple[bytes, int, int]:
    """Pack a directory tree into an in-memory tar.gz archive.

    Returns ``(compressed_bytes, uncompressed_total, file_count)``.
    Paths inside the tar are relative to ``path`` (e.g. ``sub/file.txt``).
    """
    buffer = io.BytesIO()
    file_count = 0
    uncompressed_bytes = 0

    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        for file_path in sorted(path.rglob("*")):
            if not file_path.is_file():
                continue
            arcname = file_path.relative_to(path).as_posix()
            data = file_path.read_bytes()
            info = tarfile.TarInfo(name=arcname)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
            file_count += 1
            uncompressed_bytes += len(data)

    return buffer.getvalue(), uncompressed_bytes, file_count


def upload_to_gzip(filename: str, data: bytes) -> tuple[bytes, BackupFormat, int, int]:
    """Prepare HTTP upload bytes for chunkstore ingest.

    Plain files are wrapped in a single-entry tar.gz (``format=tar+gzip``).
    Pre-compressed uploads are passed through unchanged (``format=gzip``).

    Returns ``(compressed, format, uncompressed_size, file_count)``.
    """
    if is_gzip_archive(filename):
        # ``uncompressed_bytes`` here is the compressed size — metadata only.
        return data, "gzip", len(data), 1

    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
        info = tarfile.TarInfo(name=Path(filename).name)
        info.size = len(data)
        archive.addfile(info, io.BytesIO(data))

    compressed = buffer.getvalue()
    return compressed, "tar+gzip", len(data), 1


def decompress_archive(compressed: bytes, fmt: BackupFormat) -> bytes:
    """Undo one gzip layer for download.

    - ``tar+gzip``: returns raw tar bytes (``application/x-tar``).
    - ``gzip``: returns the inner payload (often itself a ``.tar.gz`` file).

    Both formats store a single gzip wrapper in chunkstore; ``fmt`` only
    affects the response ``Content-Type`` on download.
    """
    return gzip.decompress(compressed)


def resolve_scan_path(source_root: Path, relative_path: str) -> Path:
    """Resolve a client-relative path safely under ``source_root``.

    Rejects absolute paths and ``..`` traversal after ``resolve()``.
    Used by ``POST /backups/scan`` so callers cannot read arbitrary host paths.
    """
    if not relative_path or relative_path.startswith("/"):
        raise ValueError("path must be a non-empty relative path")

    root = source_root.resolve()
    target = (root / relative_path).resolve()
    if not target.is_relative_to(root):
        raise ValueError(f"path escapes backup source root: {relative_path!r}")
    if not target.is_dir():
        raise ValueError(f"not a directory: {relative_path!r}")
    return target
