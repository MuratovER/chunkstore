# Rust crate: chunkstore-core

The core library is published on [crates.io](https://crates.io/crates/chunkstore-core) as **`chunkstore-core`**.

## Add as a Rust dependency

```toml
[dependencies]
chunkstore-core = "0.2"
```

```rust
use chunkstore::{ChunkStore, FsBackend};

let store = ChunkStore::open(FsBackend::new("/data/chunks")?)?;
store.ingest("doc", b"hello")?;
```

## Build static library for Go cgo

The Go wrapper links against a pre-built static archive:

```bash
cargo build --release -p chunkstore-core
# → target/release/libchunkstore.a
# → core/include/chunkstore.h
```

Or use the helper script from the repo root:

```bash
./scripts/build-core.sh
```

Then build/test Go:

```bash
cd go/chunkstore && go test -v
```

The cgo LDFLAGS in `go/chunkstore/store.go` expect `../../target/release/libchunkstore.a` relative to the package directory.

## C API

Headers: [`core/include/chunkstore.h`](../core/include/chunkstore.h)

v0.2 adds digest-returning ingest functions:

- `chunkstore_ingest_with_digests`
- `chunkstore_ingest_fixed`
- `chunkstore_ingest_cdc_with_digests`

Digest JSON is freed with `chunkstore_string_free`.
