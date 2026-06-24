package chunkstore

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestIngestReadDelete(t *testing.T) {
	dir := t.TempDir()
	store, err := OpenFilesystem(filepath.Join(dir, "chunks"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(store.Close)

	payload := []byte("hello-go-wrapper")
	if _, err := store.Ingest("doc", payload); err != nil {
		t.Fatalf("ingest: %v", err)
	}

	got, err := store.Read("doc")
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != string(payload) {
		t.Fatalf("read mismatch: %q", got)
	}

	stats, err := store.Stats()
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if stats.TotalBytes == 0 {
		t.Fatalf("expected non-zero stats")
	}

	if err := store.Delete("doc"); err != nil {
		t.Fatalf("delete: %v", err)
	}
}

func TestDuplicateDedup(t *testing.T) {
	dir := t.TempDir()
	store, err := OpenFilesystem(filepath.Join(dir, "chunks"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(store.Close)

	payload := []byte("shared-payload")
	if _, err := store.Ingest("a", payload); err != nil {
		t.Fatalf("ingest a: %v", err)
	}
	if _, err := store.Ingest("b", payload); err != nil {
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

func TestManifestIndexGet(t *testing.T) {
	backend, err := NewFilesystemBackend(filepath.Join(t.TempDir(), "chunks"))
	if err != nil {
		t.Fatalf("backend: %v", err)
	}
	data, ok, err := backend.Get("_manifest/__index__")
	if err != nil {
		t.Fatalf("get error: %v", err)
	}
	if ok || data != nil {
		t.Fatalf("expected missing key, got ok=%v data=%v", ok, data)
	}
}

func TestFilesystemBackendRoundtrip(t *testing.T) {
	dir := t.TempDir()
	backend, err := NewFilesystemBackend(filepath.Join(dir, "data"))
	if err != nil {
		t.Fatalf("backend: %v", err)
	}

	key := strings.Repeat("a", 64)

	if err := backend.Put(key, []byte("blob")); err != nil {
		t.Fatalf("put: %v", err)
	}
	ok, err := backend.Exists(key)
	if err != nil || !ok {
		t.Fatalf("exists: %v %v", ok, err)
	}
	data, ok, err := backend.Get(key)
	if err != nil || !ok || string(data) != "blob" {
		t.Fatalf("get: %v %v %q", ok, err, data)
	}
	if err := backend.Delete(key); err != nil {
		t.Fatalf("delete: %v", err)
	}
	if _, err := os.Stat(filepath.Join(backend.root, key)); !os.IsNotExist(err) {
		t.Fatalf("expected file removed")
	}
}
