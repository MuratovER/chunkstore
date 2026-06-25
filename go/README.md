# Go wrapper

Module: `github.com/MuratovER/chunkstore/go`
Import: `github.com/MuratovER/chunkstore/go/chunkstore`

```bash
# From repository root — build Rust static lib, then test Go
./scripts/build-core.sh
cd go/chunkstore && go test -v
```

```bash
# As a dependency (after tagging, e.g. v0.3.0)
go get github.com/MuratovER/chunkstore/go@v0.3.0
./scripts/build-core.sh   # required: cgo links target/release/libchunkstore.a
```

The Go package links against `target/release/libchunkstore.a`. See [docs/CRATES.md](../docs/CRATES.md).

## Backends

**Filesystem** (built-in C helper):

```go
import "github.com/MuratovER/chunkstore/go/chunkstore"

store, err := chunkstore.OpenFilesystem("/data/chunks")
defer store.Close()
digests, err := store.Ingest("doc", []byte("hello"))
// Stream to an io.Writer without loading the full file:
var buf bytes.Buffer
err = store.ReadTo(&buf, "doc")
```

**S3** (AWS or MinIO via aws-sdk-go-v2):

```go
store, err := chunkstore.OpenS3(chunkstore.S3Options{
    Bucket:      "my-bucket",
    Prefix:      "chunks",
    EndpointURL: "http://localhost:9000", // MinIO / LocalStack
})
defer store.Close()
```

Credentials and region follow the standard AWS SDK chain (`AWS_ACCESS_KEY_ID`, `AWS_DEFAULT_REGION`, etc.).

See [docs/S3.md](../docs/S3.md) for bucket layout and IAM.

### S3 integration tests (MinIO)

```bash
docker run -d -p 9000:9000 \
  -e MINIO_ROOT_USER=minioadmin -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data

export AWS_ACCESS_KEY_ID=minioadmin AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_DEFAULT_REGION=us-east-1
export S3_ENDPOINT_URL=http://localhost:9000 S3_BUCKET=chunkstore-test

go test -v -tags s3
```

## Cross-language integration test

Python writes to a shared FS backend, Go reads and deletes, Python verifies stats and GC:

```bash
# from repository root
./scripts/build-core.sh
cd python && maturin develop --release && pytest -m cross_lang -v
```
