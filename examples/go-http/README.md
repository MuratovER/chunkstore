# Go HTTP example

Minimal HTTP service demonstrating the Go `chunkstore` wrapper with a filesystem backend.

## Run

```bash
# from repository root
CARGO_TARGET_DIR=target cargo build --release -p chunkstore-core
cd examples/go-http
go run .
```

Server listens on `:8080` (override with `PORT`). Data directory defaults to
`$TMPDIR/chunkstore-go-http-data` (override with `CHUNKSTORE_DATA_DIR`).

## Test

```bash
cd examples/go-http
go test -v
```

## Endpoints

- `POST /files/{file_id}` — upload bytes, returns `file_id` + savings stats
- `GET /files/{file_id}` — download assembled file
- `DELETE /files/{file_id}` — delete + GC
- `GET /stats` — dedup ratio

```bash
curl -X POST --data-binary @file.pdf http://localhost:8080/files/doc_v1
curl http://localhost:8080/files/doc_v1 -o out.pdf
curl http://localhost:8080/stats
curl -X DELETE http://localhost:8080/files/doc_v1
```
