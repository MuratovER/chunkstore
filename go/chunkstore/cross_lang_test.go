package chunkstore

import (
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"testing"
)

type crossLangStats struct {
	TotalBytes  uint64  `json:"total_bytes"`
	StoredBytes uint64  `json:"stored_bytes"`
	SavingsPct  float64 `json:"savings_pct"`
}

// TestCrossLangReadDelete is invoked by the Python cross-language integration test.
// Skipped unless CHUNKSTORE_CROSS_ROOT and CHUNKSTORE_FILE_ID are set.
func TestCrossLangReadDelete(t *testing.T) {
	root := os.Getenv("CHUNKSTORE_CROSS_ROOT")
	fileID := os.Getenv("CHUNKSTORE_FILE_ID")
	if root == "" || fileID == "" {
		t.Skip("not a cross-language subprocess (missing CHUNKSTORE_CROSS_ROOT or CHUNKSTORE_FILE_ID)")
	}

	sidecar := filepath.Join(root, ".cross_lang")
	expectedPayload, err := os.ReadFile(filepath.Join(sidecar, "expected.bin"))
	if err != nil {
		t.Fatalf("read expected.bin: %v", err)
	}

	var wantStats crossLangStats
	statsRaw, err := os.ReadFile(filepath.Join(sidecar, "stats.json"))
	if err != nil {
		t.Fatalf("read stats.json: %v", err)
	}
	if err := json.Unmarshal(statsRaw, &wantStats); err != nil {
		t.Fatalf("parse stats.json: %v", err)
	}

	store, err := OpenFilesystem(root)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(store.Close)

	got, err := store.Read(fileID)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != string(expectedPayload) {
		t.Fatalf("payload mismatch: got %d bytes, want %d bytes", len(got), len(expectedPayload))
	}

	stats, err := store.Stats()
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if stats.TotalBytes != wantStats.TotalBytes {
		t.Fatalf("total_bytes: got %d, want %d", stats.TotalBytes, wantStats.TotalBytes)
	}
	if stats.StoredBytes != wantStats.StoredBytes {
		t.Fatalf("stored_bytes: got %d, want %d", stats.StoredBytes, wantStats.StoredBytes)
	}
	if math.Abs(stats.SavingsPct-wantStats.SavingsPct) > 1e-6 {
		t.Fatalf("savings_pct: got %f, want %f", stats.SavingsPct, wantStats.SavingsPct)
	}

	if err := store.Delete(fileID); err != nil {
		t.Fatalf("delete: %v", err)
	}
}
