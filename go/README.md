# Go wrapper

```bash
# Build the Rust static library first
cd ..
CARGO_TARGET_DIR=target cargo build --release -p chunkstore-core

cd go/chunkstore
go test -v
```

The Go package links against `target/release/libchunkstore.a` and uses `chunkstore_open_fs` for the filesystem backend.

## Cross-language integration test

Python writes to a shared FS backend, Go reads and deletes, Python verifies stats and GC:

```bash
# from repository root
CARGO_TARGET_DIR=target cargo build --release -p chunkstore-core
cd python && maturin develop --release && pytest -m cross_lang -v
```
