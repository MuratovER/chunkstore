package chunkstore

/*
#cgo CFLAGS: -I${SRCDIR}/../../core/include
#cgo LDFLAGS: ${SRCDIR}/../../target/release/libchunkstore.a -ldl -lm -lpthread

#include "chunkstore.h"
#include <stdlib.h>

int chunkstore_go_write_cb(uint8_t *data, size_t len, void *userdata);
*/
import "C"

import (
	"encoding/json"
	"errors"
	"io"
	"os"
	"unsafe"
)

// Store wraps the Rust chunkstore core via C-API.
type Store struct {
	handle   *C.ChunkStoreHandle
	bridgeID uint64 // callback registry id; 0 for OpenFilesystem
	keep     any    // keeps backend alive for Open()-backed stores
}

// OpenFilesystem creates a store backed by on-disk chunk files.
func OpenFilesystem(root string) (*Store, error) {
	croot := C.CString(root)
	defer C.free(unsafe.Pointer(croot))

	handle := C.chunkstore_open_fs(croot)
	if handle == nil {
		return nil, errors.New("chunkstore_open_fs failed")
	}

	return &Store{handle: handle}, nil
}

// OpenS3 creates a store backed by S3 (or S3-compatible object storage).
func OpenS3(opts S3Options) (*Store, error) {
	backend, err := NewS3Backend(opts)
	if err != nil {
		return nil, err
	}
	return Open(backend)
}

// Close releases native store resources.
func (s *Store) Close() {
	if s.handle != nil {
		C.chunkstore_destroy(s.handle)
		s.handle = nil
	}
	if s.bridgeID != 0 {
		unregisterBridge(s.bridgeID)
		s.bridgeID = 0
	}
}

// Ingest stores a file using fixed-size chunking and returns chunk digests.
func (s *Store) Ingest(fileID string, data []byte) ([]string, error) {
	return s.ingestWithDigests(func(cid *C.char, ptr *C.uint8_t, length C.size_t, digests **C.char, errOut **C.char) C.int {
		return C.chunkstore_ingest_with_digests(s.handle, cid, ptr, length, digests, errOut)
	}, fileID, data)
}

// IngestFixed stores a file with a custom fixed chunk size.
func (s *Store) IngestFixed(fileID string, data []byte, chunkSize int) ([]string, error) {
	if chunkSize <= 0 {
		return nil, errors.New("chunk size must be positive")
	}
	return s.ingestWithDigests(func(cid *C.char, ptr *C.uint8_t, length C.size_t, digests **C.char, errOut **C.char) C.int {
		return C.chunkstore_ingest_fixed(s.handle, cid, ptr, length, C.size_t(chunkSize), digests, errOut)
	}, fileID, data)
}

// IngestCDC stores a file using content-defined chunking.
func (s *Store) IngestCDC(fileID string, data []byte) ([]string, error) {
	return s.ingestWithDigests(func(cid *C.char, ptr *C.uint8_t, length C.size_t, digests **C.char, errOut **C.char) C.int {
		return C.chunkstore_ingest_cdc_with_digests(s.handle, cid, ptr, length, digests, errOut)
	}, fileID, data)
}

// IngestReader reads all bytes from r and ingests with fixed-size chunking.
func (s *Store) IngestReader(fileID string, r io.Reader, chunkSize int) ([]string, error) {
	data, err := io.ReadAll(r)
	if err != nil {
		return nil, err
	}
	return s.IngestFixed(fileID, data, chunkSize)
}

// IngestFile reads a file from disk and ingests with default fixed chunking.
func (s *Store) IngestFile(fileID, path string) ([]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return s.Ingest(fileID, data)
}

// IngestFileCDC reads a file from disk and ingests with CDC chunking.
func (s *Store) IngestFileCDC(fileID, path string) ([]string, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	return s.IngestCDC(fileID, data)
}

// Read reconstructs file bytes for `fileID`.
func (s *Store) Read(fileID string) ([]byte, error) {
	var out []byte
	err := s.withErr(func(errOut **C.char) C.int {
		cid := C.CString(fileID)
		defer C.free(unsafe.Pointer(cid))
		var data *C.uint8_t
		var length C.size_t
		rc := C.chunkstore_read(s.handle, cid, &data, &length, errOut)
		if rc != C.CHUNKSTORE_OK {
			return rc
		}
		if data != nil && length > 0 {
			out = C.GoBytes(unsafe.Pointer(data), C.int(length))
			C.chunkstore_bytes_free(data, length)
		}
		return rc
	})
	return out, err
}

// ReadTo streams verified chunk payloads to w without assembling the full file in memory.
func (s *Store) ReadTo(w io.Writer, fileID string) error {
	id := registerWriter(w)
	defer unregisterWriter(id)
	return s.withErr(func(errOut **C.char) C.int {
		cid := C.CString(fileID)
		defer C.free(unsafe.Pointer(cid))
		return C.chunkstore_read_to_writer(
			s.handle,
			cid,
			(C.ChunkstoreWriteCallback)(C.chunkstore_go_write_cb),
			unsafe.Pointer(uintptr(id)),
			errOut,
		)
	})
}

// Delete removes a file and garbage-collects unreferenced chunks.
func (s *Store) Delete(fileID string) error {
	return s.withErr(func(errOut **C.char) C.int {
		cid := C.CString(fileID)
		defer C.free(unsafe.Pointer(cid))
		return C.chunkstore_delete(s.handle, cid, errOut)
	})
}

// Stats reports deduplication metrics.
func (s *Store) Stats() (Stats, error) {
	var stats Stats
	err := s.withErr(func(errOut **C.char) C.int {
		var out C.ChunkstoreStats
		rc := C.chunkstore_stats(s.handle, &out, errOut)
		if rc == C.CHUNKSTORE_OK {
			stats = Stats{
				TotalBytes:  uint64(out.total_bytes),
				StoredBytes: uint64(out.stored_bytes),
				SavingsPct:  float64(out.savings_pct),
			}
		}
		return rc
	})
	return stats, err
}

type ingestDigestsFn func(cid *C.char, ptr *C.uint8_t, length C.size_t, digests **C.char, errOut **C.char) C.int

func (s *Store) ingestWithDigests(fn ingestDigestsFn, fileID string, data []byte) ([]string, error) {
	var digestsJSON *C.char
	err := s.withErr(func(errOut **C.char) C.int {
		cid := C.CString(fileID)
		defer C.free(unsafe.Pointer(cid))
		var ptr *C.uint8_t
		if len(data) > 0 {
			ptr = (*C.uint8_t)(unsafe.Pointer(&data[0]))
		}
		return fn(cid, ptr, C.size_t(len(data)), &digestsJSON, errOut)
	})
	if err != nil {
		return nil, err
	}
	if digestsJSON == nil {
		return nil, nil
	}
	defer C.chunkstore_string_free(digestsJSON)
	var digests []string
	if err := json.Unmarshal([]byte(C.GoString(digestsJSON)), &digests); err != nil {
		return nil, err
	}
	return digests, nil
}

func (s *Store) withErr(fn func(**C.char) C.int) error {
	var errMsg *C.char
	rc := fn(&errMsg)
	if rc != C.CHUNKSTORE_OK {
		msg := "chunkstore operation failed"
		if errMsg != nil {
			msg = C.GoString(errMsg)
			C.chunkstore_string_free(errMsg)
		}
		return errors.New(msg)
	}
	return nil
}
