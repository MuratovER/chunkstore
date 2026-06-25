# Python wrapper

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1  # if using Python 3.14+
maturin develop --release
pytest
```

Optional extras:

```bash
pip install ".[dev,s3,fastapi]"
```

### Async API (FastAPI / asyncio)

```python
from chunkstore import AsyncChunkStore, AsyncChunkClient, FilesystemBackend

async with await AsyncChunkStore.open(FilesystemBackend("/data/chunks")) as store:
    client = AsyncChunkClient(store)
    await client.upload_file("doc_v1", b"hello")
    data = await client.download_file("doc_v1")
```

For S3, use `AsyncS3Backend` (requires `chunkstore[s3]` with `aiobotocore`):

```python
from chunkstore import AsyncChunkStore, AsyncS3Backend

backend = AsyncS3Backend("my-bucket", prefix="chunks", endpoint_url="http://localhost:9000")
async with await AsyncChunkStore.open(backend) as store:
    await store.ingest("doc", b"payload")
```

**PyPI:** `pip install chunkstore` — [chunkstore on PyPI](https://pypi.org/project/chunkstore/). Maintainer release flow: [docs/PYPI.md](../docs/PYPI.md).
