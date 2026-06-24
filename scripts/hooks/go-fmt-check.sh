#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

unformatted="$(gofmt -l go/chunkstore examples/go-http)"
if [[ -n "$unformatted" ]]; then
  echo "gofmt needed on:" >&2
  echo "$unformatted" >&2
  exit 1
fi
