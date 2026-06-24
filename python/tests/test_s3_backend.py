"""S3 backend integration tests (MinIO / LocalStack via S3_ENDPOINT_URL)."""

from __future__ import annotations

import os
import uuid

import pytest

pytest.importorskip("boto3")

import boto3
from botocore.exceptions import ClientError

from chunkstore import ChunkStore, S3Backend

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
def s3_env() -> tuple[S3Backend, ChunkStore]:
    endpoint = os.environ.get("S3_ENDPOINT_URL")
    if not endpoint:
        pytest.skip("S3_ENDPOINT_URL not set (MinIO or LocalStack required)")

    bucket = os.environ.get("S3_BUCKET", "chunkstore-test")
    client = _s3_client()
    _ensure_bucket(client, bucket)

    prefix = f"pytest-{uuid.uuid4().hex}"
    backend = S3Backend(bucket, prefix=prefix, endpoint_url=endpoint)
    store = ChunkStore.open(backend)
    return backend, store


def test_s3_ingest_read_roundtrip(s3_env: tuple[S3Backend, ChunkStore]) -> None:
    store = s3_env[1]
    digests = store.ingest("doc", b"s3-payload")
    assert len(digests) == 1
    assert store.read("doc") == b"s3-payload"


def test_s3_duplicate_file_dedups(s3_env: tuple[S3Backend, ChunkStore]) -> None:
    store = s3_env[1]
    store.ingest("a", b"same-bytes")
    store.ingest("b", b"same-bytes")
    stats = store.stats()
    assert stats.savings_pct > 0.0
    assert stats.stored_bytes * 2 == stats.total_bytes


def test_s3_delete_last_file_gc(s3_env: tuple[S3Backend, ChunkStore]) -> None:
    backend, store = s3_env
    digests = store.ingest("only", b"gc-on-s3")
    digest = digests[0]
    assert backend.exists(digest)
    store.delete("only")
    assert not backend.exists(digest)
