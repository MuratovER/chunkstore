# Contributing to chunkstore

Thanks for your interest in contributing. This document is for **developers** who want to change the library — not for end users integrating chunkstore into an app (see [README.md](README.md)).

## What we're building

An **embeddable** content-addressed chunk store with deduplication and refcount GC:

- **Rust core** — chunking, hashing, manifests, refcount, C-API
- **Python / Go wrappers** — backends (FS, S3) and high-level API
- **Shared on-disk format** — Python and Go must read what the other writes

We are **not** building a backup CLI, S3 gateway, or distributed database. See [README.md](README.md#what-this-is--is-not).

---

## Getting started

### Prerequisites

| Tool | Version | Used for |
|------|---------|----------|
| Rust | 1.75+ (`rust-version` in workspace) | `core/`, Python extension |
| Python | 3.10+ | `python/` wrapper, tests |
| Go | 1.22+ | `go/` wrapper (optional) |
| maturin | 1.4+ | Python build |
| pytest | 8+ | Python tests |

### Clone and build

```bash
git clone https://github.com/MuratovER/chunkstore.git
cd chunkstore

# Rust core
cargo build --release -p chunkstore-core
cargo test -p chunkstore-core

# Python (recommended: venv)
cd python
python -m venv .venv && source .venv/bin/activate
pip install maturin pytest
maturin develop --release
pytest -q

# Go
cd ../..
CARGO_TARGET_DIR=target cargo build --release -p chunkstore-core
cd go/chunkstore && go test -v
go test -v -tags s3    # needs MinIO (see go/README.md)
```

### Pre-commit hooks

Install once — runs on `git commit` when Rust files change:

```bash
pip install pre-commit
pre-commit install
```

Hooks: `cargo fmt --all`, `cargo test -p chunkstore-core`, `cargo deny check`, `cargo-check`, `clippy`.

Run manually on all files: `pre-commit run --all-files`

### Full local CI (before opening a PR)

```bash
# From repository root
cargo fmt --all -- --check
cargo clippy -p chunkstore-core --all-targets -- -D warnings
cargo test -p chunkstore-core
cargo run -p chunkstore-core --example workload_analysis --release

cd python
maturin develop --release
pytest -q
pytest -m cross_lang -q    # needs Go + built libchunkstore.a
pytest -m s3 -q            # needs MinIO (see CI python-s3 job env vars)

cd ../go/chunkstore
go test -v

cd ../..
cargo deny check
cargo audit
```

Optional Python lint (not in CI yet, but encouraged):

```bash
cd python
pip install ".[dev]"
ruff check python_src tests
mypy python_src
```

---

## Repository layout

```
chunkstore/
├── core/                 # Rust library + C-API (source of truth)
│   ├── src/chunker/      # Fixed + CDC chunkers
│   ├── src/store/        # Manifests, refcount, persistence, FsBackend
│   ├── src/ffi/          # C-API for Go (and external bindings)
│   ├── include/          # Public C headers
│   └── tests/            # Integration + 9 scenario tests
├── python/               # PyO3/maturin wrapper
│   ├── src/lib.rs        # Native module
│   └── python_src/       # Pure Python API
├── go/chunkstore/        # cgo wrapper (links libchunkstore.a)
├── examples/             # FastAPI, FastAPI backup, Go HTTP
└── docs/images/          # README diagrams (SVG source + PNG for GitHub)
```

**Rule:** business logic belongs in `core/`. Wrappers should be thin — I/O, FFI, ergonomics — not duplicate chunking or refcount rules.

---

## How to contribute

### 1. Open an issue (recommended)

For non-trivial changes, open an issue first:

- **Bug** — steps to reproduce, expected vs actual
- **Feature** — use case, why it fits embeddable CAS (not backup)
- **Format change** — affects cross-language compatibility; needs explicit discussion

### 2. Fork, branch, change

```bash
git checkout -b fix/my-bug
# or
git checkout -b feat/streaming-read
```

Use clear commit messages. One logical change per PR when possible.

### 3. Add or update tests

| Area | Where | What to cover |
|------|-------|----------------|
| Rust core | `core/tests/`, `#[cfg(test)]` in modules | Chunking, refcount GC, persistence, errors |
| Python | `python/tests/` | Scenario tests, API roundtrips |
| Python S3 | `pytest -m s3` | S3 backend against MinIO (`S3_ENDPOINT_URL`) |
| Go | `go/chunkstore/*_test.go` | FS + S3 backends (`-tags s3` for MinIO) |
| Cross-language | `pytest -m cross_lang` | Any change to on-disk format or metadata keys |

**Required scenarios** (must keep passing):

1. Unique file + download  
2. Duplicate file dedups  
3. Partial overlap reuses prefix  
4. Delete one of two keeps shared chunk  
5. Delete last file GCs chunk  
6. Concurrent ingest (same chunk, refcount=2) — Rust  
7. Streaming upload from disk — Python  
8. CDC beats fixed on prefix insert  
9. Shared binary block savings  

### 4. Open a pull request

- Describe **what** and **why**
- Note if on-disk format or C-API changed
- Confirm you ran the relevant CI commands locally
- Link the issue if applicable

PRs need green CI on `main` / `master` (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

---

## Code guidelines

### Rust (`core/`)

- `cargo fmt` before commit; `clippy` with `-D warnings`
- No `unwrap()` / `expect()` in library code — use `Result` and `ChunkStoreError`
- `unsafe` only in `ffi/` with documented invariants
- Chunk keys = **full 64-char SHA-256 hex** — never truncate
- Verify `sha256(chunk) == digest` on every read path
- Public Rust API: doc comments (`///`) on types and functions
- Line length ≤ 100 characters

Key modules:

- `chunker/` — streaming chunkers (`Read` / feed API)
- `store/persistence.rs` — `_manifest/`, `_refcount/` keys (changing this breaks Python ↔ Go)
- `ffi/` — C-API surface; memory ownership must match header comments

### Python (`python/`)

- Type hints on public API; `mypy --strict` on `python_src/`
- `ruff` line length 100
- `FilesystemBackend` with `.root` uses Rust `FsBackend` directly — prefer this in cross-lang tests
- Backend protocol: `get`, `put`, `exists`, `delete` (see `python_src/chunkstore/backend.py`)

### Go (`go/`)

- `go fmt` / `go vet`
- `FilesystemBackend`, `S3Backend` (`OpenS3`), generic `Open(Backend)` via C callbacks
- cgo links `target/release/libchunkstore.a` — build core before `go test`
- S3 tests: `go test -tags s3` with MinIO (see [go/README.md](go/README.md))
- Do not reimplement persistence layout in Go

### C-API (`core/include/chunkstore.h`)

Treat as **stable-ish** for v0.1:

- Adding functions is OK
- `chunkstore_bytes_alloc` / `chunkstore_bytes_free` pair buffers returned from backend `get` callbacks
- Changing signatures or memory contract requires updating Go bindings and documenting in the PR

---

## On-disk format (do not break silently)

Metadata on the backend:

| Key | Format |
|-----|--------|
| `_manifest/{file_id}` | JSON: `{ "digests": [...], "file_bytes": N }` |
| `_refcount/{digest}` | JSON: unsigned integer |
| `_manifest/__index__` | JSON: array of file IDs |
| `_refcount/__index__` | JSON: array of digests |
| `{digest}` (64 hex) | Raw chunk bytes at repo root |

Any change to keys, JSON shape, or digest rules **must**:

1. Update tests in Rust, Python, and Go  
2. Run `pytest -m cross_lang`  
3. Be documented in the PR (version bump / migration note if needed)

---

## What we usually accept

- Bug fixes in core, wrappers, or backends
- Performance improvements with benchmarks
- Better errors, docs, examples
- FS/S3 backend hardening
- Streaming read/write (roadmap-friendly)
- Tests and CI improvements

## What we usually decline

- Backup CLI or `restic`-style UX
- S3 gateway / reverse proxy
- Perceptual dedup
- Distributed refcount without a clear design
- Large unrelated refactors
- Breaking on-disk format without migration plan

---

## Architecture decisions

When in doubt:

1. **Correctness** over speed (verify hashes, refcount GC)
2. **Embeddable** over standalone service
3. **Shared format** over per-language shortcuts
4. **Minimal dependencies** in `core/` (`sha2`, `serde`, `thiserror`, `fastcdc`)

---

## Release / versioning (maintainers)

- Workspace version: [`Cargo.toml`](../Cargo.toml) + [`python/pyproject.toml`](../python/pyproject.toml) + [`python_src/chunkstore/__init__.py`](../python/python_src/chunkstore/__init__.py) — keep in sync
- **Release:** bump version, push to `main` → [`release.yml`](../.github/workflows/release.yml) tags + GitHub Release → [`pypi.yml`](../.github/workflows/pypi.yml) publishes to PyPI
- **PyPI:** see [docs/PYPI.md](../docs/PYPI.md)
- **Roadmap:** [docs/ROADMAP.md](../docs/ROADMAP.md)
- Semantic versioning intended after v1.0
- crates.io publish — not automated yet

---

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).

---

## Questions

Open a [GitHub issue](https://github.com/MuratovER/chunkstore/issues) with the `question` label, or describe your use case in a feature request so we can check fit before you invest in a large PR.
