# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.2.1] - 2026-06-24

### Fixed

- Core: serialize concurrent chunk puts on the filesystem backend (unique temp files + lock around exists/put) so parallel ingests of the same digest no longer race
- CI: chain PyPI and crates.io publish after the Release workflow (`workflow_run` gate); GITHUB_TOKEN releases do not emit `release:published` to other workflows

## [0.2.0] - 2026-06-24

### Added

- C-API: `chunkstore_ingest_with_digests`, `chunkstore_ingest_fixed`, `chunkstore_ingest_cdc_with_digests` (digest list as JSON)
- Go: `IngestFixed`, `IngestReader`, `IngestFile`, `IngestFileCDC`; ingest methods return chunk digests
- Go: `S3Backend.ListChunkKeys` for paginated chunk key listing
- Python: `S3Backend.list_chunk_keys`; configurable S3 timeouts and retries
- `scripts/build-core.sh` for building `libchunkstore.a`
- Docs: [docs/S3.md](docs/S3.md), [docs/CHUNKING.md](docs/CHUNKING.md), [docs/CRATES.md](docs/CRATES.md)
- CI: `python-quality` job (`ruff` + `mypy --strict`)
- CI: crates.io publish workflow (requires `CARGO_REGISTRY_TOKEN` secret)
- Go scenario tests (1–5, 8, 9); Python scenario #6 (concurrent ingest)
- FFI integration tests in `core/tests/ffi_ingest.rs`

### Changed

- **Breaking (Go):** `Ingest` / `IngestCDC` now return `([]string, error)` instead of `error` only
- **Breaking (Go):** module path `github.com/chunkstore/chunkstore/go` → `github.com/MuratovER/chunkstore/go`
- S3 backends (Python + Go): default retries and per-request timeouts
- CI: Go version aligned to 1.24

### Fixed

- Go cgo build documents fixed `target/` path via `build-core.sh`

## [0.1.x]

### 0.1.1

- Patch release on PyPI (workspace version sync).

### 0.1.0

- Initial public release: Rust core, Python wrapper (PyPI), Go cgo wrapper, FS + S3 backends, cross-language format, FastAPI and go-http examples.

[0.2.1]: https://github.com/MuratovER/chunkstore/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/MuratovER/chunkstore/compare/v0.1.1...v0.2.0
[0.1.x]: https://github.com/MuratovER/chunkstore/releases/tag/v0.1.0
