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
	"sync"
	"sync/atomic"
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

var (
	bridgeSeq      atomic.Uint64
	bridgeRegistry sync.Map // uint64 -> *backendBridge
)

func registerBridge(bridge *backendBridge) uint64 {
	id := bridgeSeq.Add(1)
	bridgeRegistry.Store(id, bridge)
	return id
}

func unregisterBridge(id uint64) {
	if id != 0 {
		bridgeRegistry.Delete(id)
	}
}

func bridgeFromUserdata(userdata unsafe.Pointer) (*backendBridge, bool) {
	id := uint64(uintptr(userdata))
	value, ok := bridgeRegistry.Load(id)
	if !ok {
		return nil, false
	}
	bridge, ok := value.(*backendBridge)
	return bridge, ok
}

//export chunkstore_go_backend_get
func chunkstore_go_backend_get(
	key *C.char,
	outData **C.uint8_t,
	outLen *C.size_t,
	userdata unsafe.Pointer,
) C.int {
	bridge, ok := bridgeFromUserdata(userdata)
	if !ok {
		return C.CHUNKSTORE_ERR
	}
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
	bridge, ok := bridgeFromUserdata(userdata)
	if !ok {
		return C.CHUNKSTORE_ERR
	}
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
	bridge, ok := bridgeFromUserdata(userdata)
	if !ok {
		return C.CHUNKSTORE_ERR
	}
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
	bridge, ok := bridgeFromUserdata(userdata)
	if !ok {
		return C.CHUNKSTORE_ERR
	}
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
	id := registerBridge(bridge)
	// Pass numeric id as void* — avoids cgo "Go pointer to Go pointer" (Go 1.21+).
	cb := C.chunkstore_go_callbacks(unsafe.Pointer(uintptr(id)))
	handle := C.chunkstore_create(&cb)
	if handle == nil {
		unregisterBridge(id)
		return nil, errors.New("chunkstore_create failed")
	}
	return &Store{handle: handle, bridgeID: id, keep: bridge}, nil
}
