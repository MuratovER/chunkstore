#!/usr/bin/env bash
# Python tests — mirrors CI python + python-s3 jobs.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root/python"

if [[ ! -d .venv ]]; then
  python -m venv .venv
  .venv/bin/pip install -q maturin pytest
fi

.venv/bin/maturin develop --release -q
.venv/bin/pytest -q -m "not cross_lang and not s3"

run_python_s3_tests() {
  if ! command -v docker &>/dev/null; then
    echo "note: skip python S3 tests (docker not installed; CI job python-s3 still runs them)"
    return 0
  fi
  if ! docker info &>/dev/null 2>&1; then
    echo "note: skip python S3 tests (docker daemon not running; CI job python-s3 still runs them)"
    return 0
  fi

  local port=19001
  local name="chunkstore-precommit-minio-py-$$"
  trap "docker rm -f '${name}' &>/dev/null || true" RETURN

  docker run -d --name "$name" \
    -p "127.0.0.1:${port}:9000" \
    -e MINIO_ROOT_USER=minioadmin \
    -e MINIO_ROOT_PASSWORD=minioadmin \
    minio/minio:RELEASE.2024-12-18T13-15-44Z server /data

  for _ in $(seq 1 60); do
    if curl -sf "http://127.0.0.1:${port}/minio/health/live"; then
      echo "MinIO ready on port ${port}"
      break
    fi
    sleep 1
  done
  if ! curl -sf "http://127.0.0.1:${port}/minio/health/live"; then
    echo "MinIO failed to start" >&2
    docker logs "$name" >&2 || true
    return 1
  fi

  .venv/bin/pip install -q "boto3>=1.34"
  export AWS_ACCESS_KEY_ID=minioadmin
  export AWS_SECRET_ACCESS_KEY=minioadmin
  export AWS_DEFAULT_REGION=us-east-1
  export S3_ENDPOINT_URL="http://127.0.0.1:${port}"
  export S3_BUCKET=chunkstore-precommit-py

  .venv/bin/pytest -q -m s3
}

run_python_s3_tests
