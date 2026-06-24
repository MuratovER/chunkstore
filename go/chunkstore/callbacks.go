package chunkstore

/*
#cgo CFLAGS: -I${SRCDIR}/../../core/include
#cgo LDFLAGS: ${SRCDIR}/../../target/release/libchunkstore.a -ldl -lm -lpthread

#include "chunkstore.h"

// Go //export uses non-const char*; match that here (not const char* from chunkstore.h).
int chunkstore_go_backend_get(char *key, uint8_t **out_data, size_t *out_len, void *userdata);
int chunkstore_go_backend_put(char *key, uint8_t *data, size_t len, void *userdata);
int chunkstore_go_backend_exists(char *key, void *userdata);
int chunkstore_go_backend_delete(char *key, void *userdata);

static ChunkBackendCallbacks chunkstore_go_callbacks(void *userdata) {
    ChunkBackendCallbacks cb;
    cb.get = (int (*)(const char *, uint8_t **, size_t *, void *))chunkstore_go_backend_get;
    cb.put = (int (*)(const char *, const uint8_t *, size_t, void *))chunkstore_go_backend_put;
    cb.exists = (int (*)(const char *, void *))chunkstore_go_backend_exists;
    cb.delete = (int (*)(const char *, void *))chunkstore_go_backend_delete;
    cb.userdata = userdata;
    return cb;
}
*/
import "C"

import (
	"errors"
	"unsafe"
)

// Backend is the key-value interface expected by the Rust core via C callbacks.
type Backend interface {
	Get(key string) ([]byte, bool, error)
	Put(key string, data []byte) error
	Exists(key string) (bool, error)
	Delete(key string) error
}

type backendBridge struct {
	backend Backend
}

//export chunkstore_go_backend_get
func chunkstore_go_backend_get(
	key *C.char,
	outData **C.uint8_t,
	outLen *C.size_t,
	userdata unsafe.Pointer,
) C.int {
	bridge := (*backendBridge)(userdata)
	data, ok, err := bridge.backend.Get(C.GoString(key))
	if err != nil {
		return C.CHUNKSTORE_ERR
	}
	if !ok {
		*outData = nil
		*outLen = 0
		return C.CHUNKSTORE_OK
	}
	if len(data) == 0 {
		*outData = nil
		*outLen = 0
		return C.CHUNKSTORE_OK
	}
	buf := C.chunkstore_bytes_alloc(C.size_t(len(data)))
	if buf == nil {
		return C.CHUNKSTORE_ERR
	}
	dest := unsafe.Slice((*byte)(unsafe.Pointer(buf)), len(data))
	copy(dest, data)
	*outData = (*C.uint8_t)(buf)
	*outLen = C.size_t(len(data))
	return C.CHUNKSTORE_OK
}

//export chunkstore_go_backend_put
func chunkstore_go_backend_put(
	key *C.char,
	data *C.uint8_t,
	length C.size_t,
	userdata unsafe.Pointer,
) C.int {
	bridge := (*backendBridge)(userdata)
	var payload []byte
	if data != nil && length > 0 {
		payload = C.GoBytes(unsafe.Pointer(data), C.int(length))
	}
	if err := bridge.backend.Put(C.GoString(key), payload); err != nil {
		return C.CHUNKSTORE_ERR
	}
	return C.CHUNKSTORE_OK
}

//export chunkstore_go_backend_exists
func chunkstore_go_backend_exists(key *C.char, userdata unsafe.Pointer) C.int {
	bridge := (*backendBridge)(userdata)
	ok, err := bridge.backend.Exists(C.GoString(key))
	if err != nil {
		return C.CHUNKSTORE_ERR
	}
	if ok {
		return 1
	}
	return 0
}

//export chunkstore_go_backend_delete
func chunkstore_go_backend_delete(key *C.char, userdata unsafe.Pointer) C.int {
	bridge := (*backendBridge)(userdata)
	if err := bridge.backend.Delete(C.GoString(key)); err != nil {
		return C.CHUNKSTORE_ERR
	}
	return C.CHUNKSTORE_OK
}

// Open creates a store backed by a Go Backend implementation (filesystem, S3, etc.).
func Open(backend Backend) (*Store, error) {
	if backend == nil {
		return nil, errors.New("backend is nil")
	}
	bridge := &backendBridge{backend: backend}
	cb := C.chunkstore_go_callbacks(unsafe.Pointer(bridge))
	handle := C.chunkstore_create(&cb)
	if handle == nil {
		return nil, errors.New("chunkstore_create failed")
	}
	return &Store{handle: handle, keep: bridge}, nil
}
