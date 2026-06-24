#!/usr/bin/env bash
# Build the Rust static library required by the Go cgo wrapper.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export CARGO_TARGET_DIR="$root/target"
cargo build --release -p chunkstore-core -q
echo "built: $CARGO_TARGET_DIR/release/libchunkstore.a"
