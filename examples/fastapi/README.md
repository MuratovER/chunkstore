# FastAPI example

Uses the **async** API (`AsyncChunkStore` / `AsyncChunkClient`) so Rust chunking runs off the event loop.

```bash
cd python
maturin develop --release
pip install ".[fastapi,dev]"
PYTHONPATH=../examples/fastapi uvicorn main:app --reload
```

Endpoints:

- `POST /files/{file_id}` — upload bytes, returns digests + savings stats
- `GET /files/{file_id}` — download assembled file
- `DELETE /files/{file_id}` — delete + GC
- `GET /stats` — dedup ratio
