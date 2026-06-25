from __future__ import annotations

from pathlib import Path
from typing import Optional, cast

from chunkstore._s3_deps import require_boto3


class FilesystemBackend:
    """Filesystem chunk/metadata backend compatible with the Rust core layout."""

    def __init__(self, root: str | Path) -> None:
        self.root = Path(root)
        self.root.mkdir(parents=True, exist_ok=True)

    def _path(self, key: str) -> Path:
        if not key or "\\" in key:
            raise ValueError(f"invalid key: {key!r}")
        rel = Path(key)
        if any(part in ("..", "") for part in rel.parts):
            raise ValueError(f"invalid key path: {key!r}")
        path = (self.root / rel).resolve()
        root = self.root.resolve()
        if root not in path.parents and path != root:
            raise ValueError(f"key escapes backend root: {key!r}")
        return path

    def get(self, key: str) -> Optional[bytes]:
        path = self._path(key)
        if not path.is_file():
            return None
        return path.read_bytes()

    def put(self, key: str, data: bytes) -> None:
        path = self._path(key)
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(path.suffix + ".tmp")
        tmp.write_bytes(data)
        tmp.replace(path)

    def exists(self, key: str) -> bool:
        return self._path(key).exists()

    def delete(self, key: str) -> None:
        path = self._path(key)
        if path.exists():
            path.unlink()


class S3Backend:
    """Optional S3 backend (requires boto3).

    Credentials and region follow the usual boto3 chain (env vars, shared config).
    Pass ``endpoint_url`` for S3-compatible stores (MinIO, LocalStack).
    """

    def __init__(
        self,
        bucket: str,
        prefix: str = "chunks",
        *,
        endpoint_url: str | None = None,
        region_name: str | None = None,
        connect_timeout: int = 10,
        read_timeout: int = 60,
        max_attempts: int = 3,
    ) -> None:
        deps = require_boto3()
        self._client_error = deps.ClientError
        config = deps.Config(
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
            retries={"max_attempts": max_attempts, "mode": "adaptive"},
        )
        kwargs: dict[str, object] = {"config": config}
        if endpoint_url is not None:
            kwargs["endpoint_url"] = endpoint_url
        if region_name is not None:
            kwargs["region_name"] = region_name
        self._client = deps.boto3.client("s3", **kwargs)
        self.bucket = bucket
        self.prefix = prefix.strip("/")

    def _key(self, key: str) -> str:
        return f"{self.prefix}/{key}" if self.prefix else key

    def _missing_key(self, exc: BaseException) -> bool:
        if not isinstance(exc, self._client_error):
            return False
        code = exc.response.get("Error", {}).get("Code", "")
        return code in {"404", "NoSuchKey", "NotFound"}

    def get(self, key: str) -> bytes | None:
        try:
            response = self._client.get_object(Bucket=self.bucket, Key=self._key(key))
        except self._client_error as exc:
            if self._missing_key(exc):
                return None
            raise
        body = response["Body"].read()
        return cast(bytes, body)

    def put(self, key: str, data: bytes) -> None:
        self._client.put_object(Bucket=self.bucket, Key=self._key(key), Body=data)

    def exists(self, key: str) -> bool:
        try:
            self._client.head_object(Bucket=self.bucket, Key=self._key(key))
        except self._client_error as exc:
            if self._missing_key(exc):
                return False
            raise
        return True

    def delete(self, key: str) -> None:
        self._client.delete_object(Bucket=self.bucket, Key=self._key(key))

    def list_chunk_keys(self) -> list[str]:
        """List raw chunk digest keys (64-char hex) under the backend prefix."""
        import re

        digest_re = re.compile(r"^[0-9a-f]{64}$")
        prefix = f"{self.prefix}/" if self.prefix else ""
        keys: list[str] = []
        paginator = self._client.get_paginator("list_objects_v2")
        for page in paginator.paginate(Bucket=self.bucket, Prefix=prefix):
            for obj in page.get("Contents", []):
                rel = obj["Key"][len(prefix) :]
                if "/" in rel:
                    continue
                if digest_re.fullmatch(rel):
                    keys.append(rel)
        return keys
