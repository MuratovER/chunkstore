#!/usr/bin/env bash
# Fast Python tests (same markers as CI python job).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root/python"

if [[ ! -d .venv ]]; then
  python -m venv .venv
  .venv/bin/pip install -q maturin pytest
fi

.venv/bin/maturin develop --release -q
.venv/bin/pytest -q -m "not cross_lang and not s3"
