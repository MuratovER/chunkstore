#!/usr/bin/env bash
# Go wrapper tests (same as CI go job; S3 tests need MinIO + -tags s3).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"

CARGO_TARGET_DIR="$root/target" cargo build --release -p chunkstore-core -q
cd "$root/go/chunkstore"
go test -v
