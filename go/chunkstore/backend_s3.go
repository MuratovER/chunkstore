package chunkstore

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/aws/retry"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	"github.com/aws/smithy-go"
)

var digestKeyPattern = regexp.MustCompile(`^[0-9a-f]{64}$`)

// S3Options configures an S3-compatible backend (AWS S3, MinIO, LocalStack).
type S3Options struct {
	Bucket         string
	Prefix         string        // default "chunks"
	Region         string        // default AWS_DEFAULT_REGION or us-east-1
	EndpointURL    string        // set for MinIO / LocalStack
	RequestTimeout time.Duration // per-request timeout; default 60s
	MaxRetries     int           // SDK retry attempts; default 3
}

// S3Backend stores chunk blobs and metadata in an S3 bucket.
type S3Backend struct {
	client         *s3.Client
	bucket         string
	prefix         string
	requestTimeout time.Duration
}

// NewS3Backend creates an S3 backend using the AWS SDK default credential chain.
func NewS3Backend(opts S3Options) (*S3Backend, error) {
	if opts.Bucket == "" {
		return nil, fmt.Errorf("s3 bucket is required")
	}

	prefix := strings.Trim(opts.Prefix, "/")
	if prefix == "" && opts.Prefix == "" {
		prefix = "chunks"
	}

	region := opts.Region
	if region == "" {
		region = os.Getenv("AWS_DEFAULT_REGION")
	}
	if region == "" {
		region = "us-east-1"
	}

	requestTimeout := opts.RequestTimeout
	if requestTimeout == 0 {
		requestTimeout = 60 * time.Second
	}
	maxRetries := opts.MaxRetries
	if maxRetries == 0 {
		maxRetries = 3
	}

	ctx := context.Background()
	cfg, err := config.LoadDefaultConfig(
		ctx,
		config.WithRegion(region),
		config.WithRetryer(func() aws.Retryer {
			return retry.AddWithMaxAttempts(retry.NewStandard(), maxRetries)
		}),
	)
	if err != nil {
		return nil, err
	}

	client := s3.NewFromConfig(cfg, func(o *s3.Options) {
		if opts.EndpointURL != "" {
			o.BaseEndpoint = aws.String(opts.EndpointURL)
			o.UsePathStyle = true
		}
	})

	return &S3Backend{
		client:         client,
		bucket:         opts.Bucket,
		prefix:         prefix,
		requestTimeout: requestTimeout,
	}, nil
}

func (b *S3Backend) objectKey(key string) string {
	if b.prefix == "" {
		return key
	}
	return b.prefix + "/" + key
}

func (b *S3Backend) requestContext(parent context.Context) (context.Context, context.CancelFunc) {
	if parent == nil {
		parent = context.Background()
	}
	return context.WithTimeout(parent, b.requestTimeout)
}

func s3MissingKey(err error) bool {
	var apiErr smithy.APIError
	if errors.As(err, &apiErr) {
		switch apiErr.ErrorCode() {
		case "NoSuchKey", "NotFound", "404":
			return true
		}
	}
	return false
}

// Get returns object bytes for `key`, or ok=false when missing.
func (b *S3Backend) Get(key string) ([]byte, bool, error) {
	ctx, cancel := b.requestContext(context.Background())
	defer cancel()

	out, err := b.client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(b.bucket),
		Key:    aws.String(b.objectKey(key)),
	})
	if err != nil {
		if s3MissingKey(err) {
			return nil, false, nil
		}
		return nil, false, err
	}
	defer out.Body.Close()

	data, err := io.ReadAll(out.Body)
	if err != nil {
		return nil, false, err
	}
	return data, true, nil
}

// Put writes bytes for `key`.
func (b *S3Backend) Put(key string, data []byte) error {
	ctx, cancel := b.requestContext(context.Background())
	defer cancel()

	_, err := b.client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(b.bucket),
		Key:    aws.String(b.objectKey(key)),
		Body:   bytes.NewReader(data),
	})
	return err
}

// Exists reports whether `key` is present.
func (b *S3Backend) Exists(key string) (bool, error) {
	ctx, cancel := b.requestContext(context.Background())
	defer cancel()

	_, err := b.client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(b.bucket),
		Key:    aws.String(b.objectKey(key)),
	})
	if err != nil {
		if s3MissingKey(err) {
			return false, nil
		}
		return false, err
	}
	return true, nil
}

// Delete removes `key` (no error when already absent).
func (b *S3Backend) Delete(key string) error {
	ctx, cancel := b.requestContext(context.Background())
	defer cancel()

	_, err := b.client.DeleteObject(ctx, &s3.DeleteObjectInput{
		Bucket: aws.String(b.bucket),
		Key:    aws.String(b.objectKey(key)),
	})
	return err
}

// ListChunkKeys lists raw chunk digest keys (64-char hex) under the backend prefix.
func (b *S3Backend) ListChunkKeys(ctx context.Context) ([]string, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	ctx, cancel := b.requestContext(ctx)
	defer cancel()

	prefix := b.objectKey("")
	if prefix != "" && !strings.HasSuffix(prefix, "/") {
		prefix += "/"
	}

	var keys []string
	var continuation *string
	for {
		out, err := b.client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
			Bucket:            aws.String(b.bucket),
			Prefix:            aws.String(prefix),
			ContinuationToken: continuation,
		})
		if err != nil {
			return nil, err
		}
		for _, obj := range out.Contents {
			if obj.Key == nil {
				continue
			}
			rel := strings.TrimPrefix(*obj.Key, prefix)
			if rel == "" || strings.Contains(rel, "/") {
				continue
			}
			if digestKeyPattern.MatchString(rel) {
				keys = append(keys, rel)
			}
		}
		if !aws.ToBool(out.IsTruncated) {
			break
		}
		continuation = out.NextContinuationToken
	}
	return keys, nil
}
