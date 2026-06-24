# Go wrapper

```bash
# Build the Rust static library first
cd ..
CARGO_TARGET_DIR=target cargo build --release -p chunkstore-core

cd go/chunkstore
go test -v
```

The Go package links against `target/release/libchunkstore.a`.

## Backends

**Filesystem** (built-in C helper):

```go
store, err := chunkstore.OpenFilesystem("/data/chunks")
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
CARGO_TARGET_DIR=target cargo build --release -p chunkstore-core
cd python && maturin develop --release && pytest -m cross_lang -v
```
