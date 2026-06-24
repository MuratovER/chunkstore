package chunkstore

import (
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func openTempStore(t *testing.T) *Store {
	t.Helper()
	store, err := OpenFilesystem(filepath.Join(t.TempDir(), "chunks"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(store.Close)
	return store
}

func byteAt(i int) byte {
	x := uint64(i) * 0x9E3779B9
	return byte((x>>24)^(x>>16)^(x>>8)) & 0xFF
}

func make20MBWithPrefixInsert() ([]byte, []byte) {
	size := 20 * 1024 * 1024
	base := make([]byte, size)
	for i := range base {
		base[i] = byteAt(i)
	}
	edited := append([]byte{0xAB}, base...)
	return base, edited
}

func makeSharedBlockFiles(blockSize int) ([]byte, []byte) {
	shared := make([]byte, blockSize)
	for i := range shared {
		shared[i] = 0xBB
	}
	a := append(append([]byte{}, shared...), []byte("tail-a")...)
	b := append(append([]byte{}, shared...), []byte("tail-b-longer")...)
	return a, b
}

func TestScenario01UniqueFileDownload(t *testing.T) {
	store := openTempStore(t)
	digests, err := store.Ingest("doc", []byte("unique-payload"))
	if err != nil {
		t.Fatalf("ingest: %v", err)
	}
	if len(digests) != 1 {
		t.Fatalf("digests: %v", digests)
	}
	got, err := store.Read("doc")
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != "unique-payload" {
		t.Fatalf("read mismatch: %q", got)
	}
}

func TestScenario02DuplicateFileDedups(t *testing.T) {
	store := openTempStore(t)
	payload := []byte("same-payload")
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
	if stats.StoredBytes*2 != stats.TotalBytes {
		t.Fatalf("stored*2 != total: %+v", stats)
	}
}

func TestScenario03PartialOverlapReusesPrefix(t *testing.T) {
	store := openTempStore(t)
	chunkSize := 64
	prefix := strings.Repeat("\x01", chunkSize*2)
	fileA := append(append([]byte{}, prefix...), []byte("AAA")...)
	fileB := append(append([]byte{}, prefix...), []byte("BBBBB")...)

	da, err := store.IngestFixed("a", fileA, chunkSize)
	if err != nil {
		t.Fatalf("ingest a: %v", err)
	}
	db, err := store.IngestFixed("b", fileB, chunkSize)
	if err != nil {
		t.Fatalf("ingest b: %v", err)
	}
	if len(da) < 3 || len(db) < 3 {
		t.Fatalf("expected 3 chunks, got %d and %d", len(da), len(db))
	}
	if da[0] != db[0] || da[1] != db[1] {
		t.Fatalf("shared prefix chunks differ: %v vs %v", da[:2], db[:2])
	}
	if da[2] == db[2] {
		t.Fatalf("tail chunks should differ")
	}
}

func TestScenario04DeleteOneOfTwoKeepsShared(t *testing.T) {
	root := filepath.Join(t.TempDir(), "chunks")
	store, err := OpenFilesystem(root)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	t.Cleanup(store.Close)

	if _, err := store.Ingest("a", []byte("shared-prefix")); err != nil {
		t.Fatalf("ingest a: %v", err)
	}
	digests, err := store.Ingest("b", []byte("shared-prefix"))
	if err != nil {
		t.Fatalf("ingest b: %v", err)
	}
	shared := digests[0]

	if err := store.Delete("a"); err != nil {
		t.Fatalf("delete a: %v", err)
	}
	got, err := store.Read("b")
	if err != nil {
		t.Fatalf("read b: %v", err)
	}
	if string(got) != "shared-prefix" {
		t.Fatalf("read b: %q", got)
	}
	if _, err := os.Stat(filepath.Join(root, shared)); err != nil {
		t.Fatalf("shared chunk should remain on disk: %v", err)
	}
}

func TestScenario05DeleteLastFileGCsChunk(t *testing.T) {
	root := filepath.Join(t.TempDir(), "chunks")
	store, err := OpenFilesystem(root)
	if err != nil {
		t.Fatalf("open: %v", err)
	}

	digests, err := store.Ingest("only", []byte("gc-me"))
	if err != nil {
		t.Fatalf("ingest: %v", err)
	}
	digest := digests[0]

	if err := store.Delete("only"); err != nil {
		t.Fatalf("delete: %v", err)
	}
	store.Close()

	if _, err := os.Stat(filepath.Join(root, digest)); !os.IsNotExist(err) {
		t.Fatalf("chunk file should be GC'd, stat err=%v", err)
	}
}

func TestScenario08CDCBeatsFixedOnPrefixInsert(t *testing.T) {
	base, edited := make20MBWithPrefixInsert()

	fixedStore := openTempStore(t)
	if _, err := fixedStore.Ingest("base", base); err != nil {
		t.Fatalf("fixed base: %v", err)
	}
	if _, err := fixedStore.Ingest("edited", edited); err != nil {
		t.Fatalf("fixed edited: %v", err)
	}
	fixedStats, err := fixedStore.Stats()
	if err != nil {
		t.Fatalf("fixed stats: %v", err)
	}

	cdcStore := openTempStore(t)
	if _, err := cdcStore.IngestCDC("base", base); err != nil {
		t.Fatalf("cdc base: %v", err)
	}
	if _, err := cdcStore.IngestCDC("edited", edited); err != nil {
		t.Fatalf("cdc edited: %v", err)
	}
	cdcStats, err := cdcStore.Stats()
	if err != nil {
		t.Fatalf("cdc stats: %v", err)
	}

	if fixedStats.SavingsPct >= 5.0 {
		t.Fatalf("fixed savings should be <5%%, got %f", fixedStats.SavingsPct)
	}
	if cdcStats.SavingsPct <= 30.0 {
		t.Fatalf("cdc savings should be >30%%, got %f", cdcStats.SavingsPct)
	}
}

func TestScenario09SharedBinaryBlockSavings(t *testing.T) {
	store := openTempStore(t)
	a, b := makeSharedBlockFiles(4 * 1024 * 1024)
	if _, err := store.Ingest("a", a); err != nil {
		t.Fatalf("ingest a: %v", err)
	}
	if _, err := store.Ingest("b", b); err != nil {
		t.Fatalf("ingest b: %v", err)
	}
	stats, err := store.Stats()
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if stats.SavingsPct < 40.0 {
		t.Fatalf("expected savings >= 40%%, got %f", stats.SavingsPct)
	}
}

func TestIngestCDCRoundtrip(t *testing.T) {
	store := openTempStore(t)
	payload := make([]byte, 600*1024)
	for i := range payload {
		payload[i] = 0xCD
	}
	if _, err := store.IngestCDC("cdc-doc", payload); err != nil {
		t.Fatalf("ingest cdc: %v", err)
	}
	got, err := store.Read("cdc-doc")
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if len(got) != len(payload) {
		t.Fatalf("length mismatch: %d vs %d", len(got), len(payload))
	}
}

func TestIngestReaderAndFile(t *testing.T) {
	store := openTempStore(t)
	data := bytesRepeat(5, 256*1024+17)
	path := filepath.Join(t.TempDir(), "upload.bin")
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("write file: %v", err)
	}
	if _, err := store.IngestFile("from-path", path); err != nil {
		t.Fatalf("ingest file: %v", err)
	}
	got, err := store.Read("from-path")
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(got) != string(data) {
		t.Fatalf("file ingest mismatch")
	}

	reader := strings.NewReader(string(data))
	if _, err := store.IngestReader("from-reader", reader, 64*1024); err != nil {
		t.Fatalf("ingest reader: %v", err)
	}
	got, err = store.Read("from-reader")
	if err != nil {
		t.Fatalf("read reader: %v", err)
	}
	if string(got) != string(data) {
		t.Fatalf("reader ingest mismatch")
	}
}

func TestConcurrentIngestDedup(t *testing.T) {
	store := openTempStore(t)
	payload := []byte("concurrent-same")
	var wg sync.WaitGroup
	results := make([][]string, 2)
	var errs [2]error

	for i := 0; i < 2; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			results[idx], errs[idx] = store.Ingest(fmt.Sprintf("doc-%d", idx), payload)
		}(i)
	}
	wg.Wait()

	for i, err := range errs {
		if err != nil {
			t.Fatalf("ingest %d: %v", i, err)
		}
	}
	if results[0][0] != results[1][0] {
		t.Fatalf("expected same digest, got %v and %v", results[0], results[1])
	}
	stats, err := store.Stats()
	if err != nil {
		t.Fatalf("stats: %v", err)
	}
	if stats.SavingsPct <= 0 || math.IsNaN(stats.SavingsPct) {
		t.Fatalf("expected dedup savings, got %f", stats.SavingsPct)
	}
}

func bytesRepeat(b byte, n int) []byte {
	out := make([]byte, n)
	for i := range out {
		out[i] = b
	}
	return out
}
