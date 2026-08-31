package chunkstore

/*
#cgo CFLAGS: -I${SRCDIR}/../../core/include
#cgo LDFLAGS: ${SRCDIR}/../../target/release/libchunkstore.a -ldl -lm -lpthread

#include "chunkstore.h"

int chunkstore_go_read_cb(uint8_t *buf, size_t cap, size_t *out_len, void *userdata);
*/
import "C"

import (
	"io"
	"sync"
	"sync/atomic"
	"unsafe"
)

var (
	readerSeq      atomic.Uint64
	readerRegistry sync.Map // uint64 -> io.Reader
)

func registerReader(r io.Reader) uint64 {
	id := readerSeq.Add(1)
	readerRegistry.Store(id, r)
	return id
}

func unregisterReader(id uint64) {
	if id != 0 {
		readerRegistry.Delete(id)
	}
}

func readerFromUserdata(userdata unsafe.Pointer) (io.Reader, bool) {
	id := uint64(uintptr(userdata))
	value, ok := readerRegistry.Load(id)
	if !ok {
		return nil, false
	}
	r, ok := value.(io.Reader)
	return r, ok
}

//export chunkstore_go_read_cb
func chunkstore_go_read_cb(buf *C.uint8_t, cap C.size_t, outLen *C.size_t, userdata unsafe.Pointer) C.int {
	r, ok := readerFromUserdata(userdata)
	if !ok || outLen == nil {
		return C.CHUNKSTORE_ERR
	}
	if cap == 0 {
		*outLen = 0
		return C.CHUNKSTORE_OK
	}
	tmp := make([]byte, int(cap))
	n, err := r.Read(tmp)
	if err != nil && err != io.EOF {
		return C.CHUNKSTORE_ERR
	}
	if n > 0 && buf != nil {
		dst := unsafe.Slice((*byte)(unsafe.Pointer(buf)), int(cap))
		copy(dst, tmp[:n])
	}
	*outLen = C.size_t(n)
	return C.CHUNKSTORE_OK
}
