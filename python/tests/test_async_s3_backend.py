"""Async S3 backend integration tests (MinIO / LocalStack via S3_ENDPOINT_URL)."""

from __future__ import annotations

import os
import uuid

import pytest

pytest.importorskip("aiobotocore")
pytest.importorskip("boto3")

import boto3
from botocore.exceptions import ClientError

from chunkstore import AsyncChunkStore, AsyncS3Backend

pytestmark = pytest.mark.s3


def _s3_client() -> object:
    endpoint = os.environ.get("S3_ENDPOINT_URL", "http://127.0.0.1:9000")
    return boto3.client(
        "s3",
        endpoint_url=endpoint,
        aws_access_key_id=os.environ.get("AWS_ACCESS_KEY_ID", "minioadmin"),
        aws_secret_access_key=os.environ.get("AWS_SECRET_ACCESS_KEY", "minioadmin"),
        region_name=os.environ.get("AWS_DEFAULT_REGION", "us-east-1"),
    )


def _ensure_bucket(client: object, bucket: str) -> None:
    try:
        client.create_bucket(Bucket=bucket)  # type: ignore[attr-defined]
    except ClientError as exc:
        code = exc.response.get("Error", {}).get("Code", "")
        if code not in {"BucketAlreadyOwnedByYou", "BucketAlreadyExists"}:
            raise


@pytest.fixture
async def async_s3_env() -> tuple[AsyncS3Backend, AsyncChunkStore]:
    endpoint = os.environ.get("S3_ENDPOINT_URL")
    if not endpoint:
        pytest.skip("S3_ENDPOINT_URL not set (MinIO or LocalStack required)")

    bucket = os.environ.get("S3_BUCKET", "chunkstore-test")
    client = _s3_client()
    _ensure_bucket(client, bucket)

    prefix = f"async-pytest-{uuid.uuid4().hex}"
    backend = AsyncS3Backend(bucket, prefix=prefix, endpoint_url=endpoint)
    store = await AsyncChunkStore.open(backend)
    yield backend, store
    await store.aclose()


@pytest.mark.asyncio
async def test_async_s3_ingest_read_roundtrip(async_s3_env: tuple[AsyncS3Backend, AsyncChunkStore]) -> None:
    store = async_s3_env[1]
    digests = await store.ingest("doc", b"async-s3-payload")
    assert len(digests) == 1
    assert await store.read("doc") == b"async-s3-payload"


@pytest.mark.asyncio
async def test_async_s3_duplicate_file_dedups(async_s3_env: tuple[AsyncS3Backend, AsyncChunkStore]) -> None:
    store = async_s3_env[1]
    await store.ingest("a", b"same-bytes")
    await store.ingest("b", b"same-bytes")
    stats = await store.stats()
    assert stats.savings_pct > 0.0
    assert stats.stored_bytes * 2 == stats.total_bytes


@pytest.mark.asyncio
async def test_async_s3_delete_last_file_gc(async_s3_env: tuple[AsyncS3Backend, AsyncChunkStore]) -> None:
    backend, store = async_s3_env
    digests = await store.ingest("only", b"gc-on-async-s3")
    digest = digests[0]
    assert await backend.aexists(digest)
    await store.delete("only")
    assert not await backend.aexists(digest)


@pytest.mark.asyncio
async def test_async_s3_iter_chunks(async_s3_env: tuple[AsyncS3Backend, AsyncChunkStore]) -> None:
    store = async_s3_env[1]
    payload = b"chunks-" + b"x" * 10_000
    await store.ingest_fixed("doc", payload, 4_096)
    parts = [part async for part in store.iter_chunks("doc")]
    assert b"".join(parts) == payload
