package chunkstore

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	"github.com/aws/smithy-go"
)

// S3Options configures an S3-compatible backend (AWS S3, MinIO, LocalStack).
type S3Options struct {
	Bucket      string
	Prefix      string // default "chunks"
	Region      string // default AWS_DEFAULT_REGION or us-east-1
	EndpointURL string // set for MinIO / LocalStack
}

// S3Backend stores chunk blobs and metadata in an S3 bucket.
type S3Backend struct {
	client *s3.Client
	bucket string
	prefix string
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

	ctx := context.Background()
	cfg, err := config.LoadDefaultConfig(ctx, config.WithRegion(region))
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
		client: client,
		bucket: opts.Bucket,
		prefix: prefix,
	}, nil
}

func (b *S3Backend) objectKey(key string) string {
	if b.prefix == "" {
		return key
	}
	return b.prefix + "/" + key
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
	out, err := b.client.GetObject(context.Background(), &s3.GetObjectInput{
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
	_, err := b.client.PutObject(context.Background(), &s3.PutObjectInput{
		Bucket: aws.String(b.bucket),
		Key:    aws.String(b.objectKey(key)),
		Body:   bytes.NewReader(data),
	})
	return err
}

// Exists reports whether `key` is present.
func (b *S3Backend) Exists(key string) (bool, error) {
	_, err := b.client.HeadObject(context.Background(), &s3.HeadObjectInput{
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
	_, err := b.client.DeleteObject(context.Background(), &s3.DeleteObjectInput{
		Bucket: aws.String(b.bucket),
		Key:    aws.String(b.objectKey(key)),
	})
	return err
}
