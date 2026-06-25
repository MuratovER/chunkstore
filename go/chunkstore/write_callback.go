package chunkstore

/*
#cgo CFLAGS: -I${SRCDIR}/../../core/include
#cgo LDFLAGS: ${SRCDIR}/../../target/release/libchunkstore.a -ldl -lm -lpthread

#include "chunkstore.h"

int chunkstore_go_write_cb(uint8_t *data, size_t len, void *userdata);
*/
import "C"

import (
	"io"
	"sync"
	"sync/atomic"
	"unsafe"
)

var (
	writerSeq      atomic.Uint64
	writerRegistry sync.Map // uint64 -> io.Writer
)

func registerWriter(w io.Writer) uint64 {
	id := writerSeq.Add(1)
	writerRegistry.Store(id, w)
	return id
}

func unregisterWriter(id uint64) {
	if id != 0 {
		writerRegistry.Delete(id)
	}
}

func writerFromUserdata(userdata unsafe.Pointer) (io.Writer, bool) {
	id := uint64(uintptr(userdata))
	value, ok := writerRegistry.Load(id)
	if !ok {
		return nil, false
	}
	w, ok := value.(io.Writer)
	return w, ok
}

//export chunkstore_go_write_cb
func chunkstore_go_write_cb(data *C.uint8_t, length C.size_t, userdata unsafe.Pointer) C.int {
	w, ok := writerFromUserdata(userdata)
	if !ok {
		return C.CHUNKSTORE_ERR
	}
	var payload []byte
	if data != nil && length > 0 {
		payload = unsafe.Slice((*byte)(unsafe.Pointer(data)), int(length))
	}
	if _, err := w.Write(payload); err != nil {
		return C.CHUNKSTORE_ERR
	}
	return C.CHUNKSTORE_OK
}
