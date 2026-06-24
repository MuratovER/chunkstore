package chunkstore

/*
#cgo CFLAGS: -I${SRCDIR}/../../core/include
#cgo LDFLAGS: ${SRCDIR}/../../target/release/libchunkstore.a -ldl -lm -lpthread

#include "chunkstore.h"
#include <stdlib.h>
*/
import "C"

import (
	"errors"
	"unsafe"
)

// Store wraps the Rust chunkstore core via C-API.
type Store struct {
	handle *C.ChunkStoreHandle
	keep   any // keeps callback userdata alive for Open()-backed stores
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
}

// Ingest stores a file using fixed-size chunking.
func (s *Store) Ingest(fileID string, data []byte) error {
	return s.withErr(func(errOut **C.char) C.int {
		cid := C.CString(fileID)
		defer C.free(unsafe.Pointer(cid))
		var ptr *C.uint8_t
		if len(data) > 0 {
			ptr = (*C.uint8_t)(unsafe.Pointer(&data[0]))
		}
		return C.chunkstore_ingest(s.handle, cid, ptr, C.size_t(len(data)), errOut)
	})
}

// IngestCDC stores a file using content-defined chunking.
func (s *Store) IngestCDC(fileID string, data []byte) error {
	return s.withErr(func(errOut **C.char) C.int {
		cid := C.CString(fileID)
		defer C.free(unsafe.Pointer(cid))
		var ptr *C.uint8_t
		if len(data) > 0 {
			ptr = (*C.uint8_t)(unsafe.Pointer(&data[0]))
		}
		return C.chunkstore_ingest_cdc(s.handle, cid, ptr, C.size_t(len(data)), errOut)
	})
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
