#!/usr/bin/env bash
# Go wrapper tests — mirrors CI go + go-s3 jobs.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"

"$root/scripts/build-core.sh"
cd "$root/go/chunkstore"

go test -v

run_go_s3_tests() {
  if ! command -v docker &>/dev/null; then
    echo "note: skip go S3 tests (docker not installed; CI job go-s3 still runs them)"
    return 0
  fi
  if ! docker info &>/dev/null 2>&1; then
    echo "note: skip go S3 tests (docker daemon not running; CI job go-s3 still runs them)"
    return 0
  fi

  local port=19000
  local name="chunkstore-precommit-minio-$$"
  trap "docker rm -f \"${name}\" &>/dev/null || true" RETURN

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

  export AWS_ACCESS_KEY_ID=minioadmin
  export AWS_SECRET_ACCESS_KEY=minioadmin
  export AWS_DEFAULT_REGION=us-east-1
  export S3_ENDPOINT_URL="http://127.0.0.1:${port}"
  export S3_BUCKET=chunkstore-precommit-go

  go test -v -tags s3 -run 'TestS3'
}

run_go_s3_tests
