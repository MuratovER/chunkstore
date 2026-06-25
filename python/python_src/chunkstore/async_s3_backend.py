from __future__ import annotations

import re
from contextlib import AsyncExitStack
from typing import Any, cast

from chunkstore._s3_deps import require_aiobotocore


class AsyncS3Backend:
    """Async S3 backend using aiobotocore (requires chunkstore[s3]).

    Credentials and region follow the usual AWS chain (env vars, shared config).
    Pass ``endpoint_url`` for S3-compatible stores (MinIO, LocalStack).

    Call ``await backend.start()`` before use (or open via :class:`AsyncChunkStore`).
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
        deps = require_aiobotocore()
        self._session = deps.aio_session.AioSession()
        self._client_error = deps.ClientError
        self._config = deps.Config(
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
            retries={"max_attempts": max_attempts, "mode": "adaptive"},
        )
        self._client_kwargs: dict[str, Any] = {"config": self._config}
        if endpoint_url is not None:
            self._client_kwargs["endpoint_url"] = endpoint_url
        if region_name is not None:
            self._client_kwargs["region_name"] = region_name
        self.bucket = bucket
        self.prefix = prefix.strip("/")
        self._exit_stack: AsyncExitStack | None = None
        self._client: Any = None

    async def start(self) -> None:
        if self._client is not None:
            return
        self._exit_stack = AsyncExitStack()
        self._client = await self._exit_stack.enter_async_context(
            self._session.create_client("s3", **self._client_kwargs)
        )

    async def aclose(self) -> None:
        if self._exit_stack is not None:
            await self._exit_stack.aclose()
        self._exit_stack = None
        self._client = None

    def _require_client(self) -> Any:
        if self._client is None:
            raise RuntimeError("AsyncS3Backend not started; call await backend.start() first")
        return self._client

    def _key(self, key: str) -> str:
        return f"{self.prefix}/{key}" if self.prefix else key

    def _missing_key(self, exc: BaseException) -> bool:
        if not isinstance(exc, self._client_error):
            return False
        code = exc.response.get("Error", {}).get("Code", "")
        return code in {"404", "NoSuchKey", "NotFound"}

    async def aget(self, key: str) -> bytes | None:
        client = self._require_client()
        try:
            response = await client.get_object(Bucket=self.bucket, Key=self._key(key))
        except self._client_error as exc:
            if self._missing_key(exc):
                return None
            raise
        async with response["Body"] as stream:
            body = await stream.read()
        return cast(bytes, body)

    async def aput(self, key: str, data: bytes) -> None:
        client = self._require_client()
        await client.put_object(Bucket=self.bucket, Key=self._key(key), Body=data)

    async def aexists(self, key: str) -> bool:
        client = self._require_client()
        try:
            await client.head_object(Bucket=self.bucket, Key=self._key(key))
        except self._client_error as exc:
            if self._missing_key(exc):
                return False
            raise
        return True

    async def adelete(self, key: str) -> None:
        client = self._require_client()
        await client.delete_object(Bucket=self.bucket, Key=self._key(key))

    async def alist_chunk_keys(self) -> list[str]:
        """List raw chunk digest keys (64-char hex) under the backend prefix."""
        client = self._require_client()
        digest_re = re.compile(r"^[0-9a-f]{64}$")
        prefix = f"{self.prefix}/" if self.prefix else ""
        keys: list[str] = []
        paginator = client.get_paginator("list_objects_v2")
        async for page in paginator.paginate(Bucket=self.bucket, Prefix=prefix):
            for obj in page.get("Contents", []):
                rel = obj["Key"][len(prefix) :]
                if "/" in rel:
                    continue
                if digest_re.fullmatch(rel):
                    keys.append(rel)
        return keys
