//go:build s3

package chunkstore

import (
	"context"
	"errors"
	"fmt"
	"os"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	"github.com/aws/smithy-go"
)

func s3TestEnv(t *testing.T) (endpoint, bucket, prefix string) {
	t.Helper()
	endpoint = os.Getenv("S3_ENDPOINT_URL")
	if endpoint == "" {
		t.Skip("S3_ENDPOINT_URL not set (MinIO or LocalStack required)")
	}
	bucket = os.Getenv("S3_BUCKET")
	if bucket == "" {
		bucket = "chunkstore-test"
	}
	prefix = fmt.Sprintf("go-pytest-%d", os.Getpid())
	return endpoint, bucket, prefix
}

func ensureS3Bucket(t *testing.T, endpoint, bucket string) {
	t.Helper()
	ctx := context.Background()
	cfg, err := config.LoadDefaultConfig(ctx, config.WithRegion(os.Getenv("AWS_DEFAULT_REGION")))
	if err != nil {
		t.Fatalf("load aws config: %v", err)
	}
	if cfg.Region == "" {
		cfg.Region = "us-east-1"
	}
	client := s3.NewFromConfig(cfg, func(o *s3.Options) {
		o.BaseEndpoint = aws.String(endpoint)
		o.UsePathStyle = true
	})
	_, err = client.CreateBucket(ctx, &s3.CreateBucketInput{Bucket: aws.String(bucket)})
	if err != nil {
		var apiErr smithy.APIError
		if errors.As(err, &apiErr) {
			switch apiErr.ErrorCode() {
			case "BucketAlreadyOwnedByYou", "BucketAlreadyExists":
				return
			}
		}
		t.Fatalf("create bucket: %v", err)
	}
}

func openS3TestStore(t *testing.T) (*Store, *S3Backend) {
	t.Helper()
	endpoint, bucket, prefix := s3TestEnv(t)
	ensureS3Bucket(t, endpoint, bucket)

	backend, err := NewS3Backend(S3Options{
		Bucket:      bucket,
		Prefix:      prefix,
		EndpointURL: endpoint,
	})
	if err != nil {
		t.Fatalf("new s3 backend: %v", err)
	}
	store, err := Open(backend)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(store.Close)
	return store, backend
}

func TestS3IngestReadRoundtrip(t *testing.T) {
	store, _ := openS3TestStore(t)

	payload := []byte("go-s3-payload")
	if err := store.Ingest("doc", payload); err != nil {
		t.Fatalf("ingest: %v", err)
	}
	got, err := store.Read("doc")
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("read mismatch: %q", got)
	}
}

func TestS3DuplicateDedup(t *testing.T) {
	store, _ := openS3TestStore(t)

	payload := []byte("shared-go-s3")
	if err := store.Ingest("a", payload); err != nil {
		t.Fatalf("ingest a: %v", err)
	}
	if err := store.Ingest("b", payload); err != nil {
		t.Fatalf("ingest b: %v", err)
	}
	stats, err := store.Stats()
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if stats.SavingsPct <= 0 {
		t.Fatalf("expected dedup savings, got %f", stats.SavingsPct)
	}
}

func TestS3DeleteGC(t *testing.T) {
	store, backend := openS3TestStore(t)

	payload := []byte("gc-on-go-s3")
	if err := store.Ingest("only", payload); err != nil {
		t.Fatalf("ingest: %v", err)
	}

	stats, err := store.Stats()
	if err != nil {
		t.Fatalf("stats before delete: %v", err)
	}
	if stats.StoredBytes == 0 {
		t.Fatalf("expected stored bytes before delete")
	}

	if err := store.Delete("only"); err != nil {
		t.Fatalf("delete: %v", err)
	}

	// After deleting the only file, stored bytes should drop to zero.
	stats, err = store.Stats()
	if err != nil {
		t.Fatalf("stats after delete: %v", err)
	}
	if stats.StoredBytes != 0 {
		t.Fatalf("expected stored_bytes=0 after GC, got %d", stats.StoredBytes)
	}

	// Sanity: backend still works (no leaked broken state).
	ok, err := backend.Exists("_manifest/__index__")
	if err != nil {
		t.Fatalf("exists manifest index: %v", err)
	}
	if ok {
		t.Fatalf("expected empty manifest index after delete")
	}
}
