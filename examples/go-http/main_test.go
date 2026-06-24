package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"

	"github.com/MuratovER/chunkstore/go/chunkstore"
)

func newTestServer(t *testing.T) *httptest.Server {
	t.Helper()

	store, err := chunkstore.OpenFilesystem(filepath.Join(t.TempDir(), "chunks"))
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(store.Close)

	return httptest.NewServer((&server{store: store}).routes())
}

func TestUploadDownloadDeleteStats(t *testing.T) {
	ts := newTestServer(t)
	defer ts.Close()

	payload := []byte("hello-go-http")
	uploadResp, err := http.Post(
		ts.URL+"/files/doc_v1",
		"application/octet-stream",
		bytes.NewReader(payload),
	)
	if err != nil {
		t.Fatalf("upload: %v", err)
	}
	defer uploadResp.Body.Close()

	if uploadResp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(uploadResp.Body)
		t.Fatalf("upload status %d: %s", uploadResp.StatusCode, body)
	}

	var uploaded uploadJSON
	if err := json.NewDecoder(uploadResp.Body).Decode(&uploaded); err != nil {
		t.Fatalf("decode upload: %v", err)
	}
	if uploaded.FileID != "doc_v1" {
		t.Fatalf("file_id: %q", uploaded.FileID)
	}
	if uploaded.Stats.TotalBytes == 0 {
		t.Fatalf("expected non-zero total_bytes")
	}

	downloadResp, err := http.Get(ts.URL + "/files/doc_v1")
	if err != nil {
		t.Fatalf("download: %v", err)
	}
	defer downloadResp.Body.Close()

	got, err := io.ReadAll(downloadResp.Body)
	if err != nil {
		t.Fatalf("read body: %v", err)
	}
	if !bytes.Equal(got, payload) {
		t.Fatalf("download mismatch: %q", got)
	}

	statsResp, err := http.Get(ts.URL + "/stats")
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	defer statsResp.Body.Close()

	var stats statsJSON
	if err := json.NewDecoder(statsResp.Body).Decode(&stats); err != nil {
		t.Fatalf("decode stats: %v", err)
	}
	if stats.TotalBytes == 0 {
		t.Fatalf("expected stats total_bytes > 0")
	}

	deleteReq, err := http.NewRequest(http.MethodDelete, ts.URL+"/files/doc_v1", nil)
	if err != nil {
		t.Fatalf("delete request: %v", err)
	}
	deleteResp, err := http.DefaultClient.Do(deleteReq)
	if err != nil {
		t.Fatalf("delete: %v", err)
	}
	defer deleteResp.Body.Close()

	if deleteResp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(deleteResp.Body)
		t.Fatalf("delete status %d: %s", deleteResp.StatusCode, body)
	}

	missingResp, err := http.Get(ts.URL + "/files/doc_v1")
	if err != nil {
		t.Fatalf("get missing: %v", err)
	}
	defer missingResp.Body.Close()
	if missingResp.StatusCode != http.StatusNotFound {
		t.Fatalf("expected 404 after delete, got %d", missingResp.StatusCode)
	}

	deleteMissing, err := http.NewRequest(http.MethodDelete, ts.URL+"/files/missing", nil)
	if err != nil {
		t.Fatalf("delete missing request: %v", err)
	}
	deleteMissingResp, err := http.DefaultClient.Do(deleteMissing)
	if err != nil {
		t.Fatalf("delete missing: %v", err)
	}
	defer deleteMissingResp.Body.Close()
	if deleteMissingResp.StatusCode != http.StatusNotFound {
		body, _ := io.ReadAll(deleteMissingResp.Body)
		t.Fatalf("delete missing status %d: %s", deleteMissingResp.StatusCode, body)
	}
}
