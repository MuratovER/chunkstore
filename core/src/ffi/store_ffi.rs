use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::Arc;

use crate::error::ChunkStoreError;
use crate::ffi::types::{
    ChunkBackendCallbacks, ChunkStoreHandle, ChunkstoreStats, CHUNKSTORE_ERR, CHUNKSTORE_OK,
};
use crate::store::{ChunkBackend, ChunkStore, FsBackend};

struct CallbackBackend {
    callbacks: ChunkBackendCallbacks,
}

unsafe impl Send for CallbackBackend {}
unsafe impl Sync for CallbackBackend {}

impl CallbackBackend {
    fn cstr(key: &str) -> Result<std::ffi::CString, ChunkStoreError> {
        std::ffi::CString::new(key).map_err(|e| ChunkStoreError::invalid_argument(e.to_string()))
    }
}

impl ChunkBackend for CallbackBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, ChunkStoreError> {
        let get = self
            .callbacks
            .get
            .ok_or_else(|| ChunkStoreError::backend("backend get callback missing"))?;
        let c_key = Self::cstr(key)?;
        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;

        let rc = unsafe {
            get(
                c_key.as_ptr(),
                &mut out_data,
                &mut out_len,
                self.callbacks.userdata,
            )
        };
        if rc != CHUNKSTORE_OK {
            return Err(ChunkStoreError::backend(format!(
                "get failed for key {key}"
            )));
        }
        if out_data.is_null() || out_len == 0 {
            return Ok(None);
        }
        let slice = unsafe { std::slice::from_raw_parts(out_data, out_len) };
        let data = slice.to_vec();
        unsafe { chunkstore_bytes_free(out_data, out_len) };
        Ok(Some(data))
    }

    fn put(&self, key: &str, data: &[u8]) -> Result<(), ChunkStoreError> {
        let put = self
            .callbacks
            .put
            .ok_or_else(|| ChunkStoreError::backend("backend put callback missing"))?;
        let c_key = Self::cstr(key)?;
        let rc = unsafe {
            put(
                c_key.as_ptr(),
                data.as_ptr(),
                data.len(),
                self.callbacks.userdata,
            )
        };
        if rc != CHUNKSTORE_OK {
            return Err(ChunkStoreError::backend(format!(
                "put failed for key {key}"
            )));
        }
        Ok(())
    }

    fn exists(&self, key: &str) -> Result<bool, ChunkStoreError> {
        let exists = self
            .callbacks
            .exists
            .ok_or_else(|| ChunkStoreError::backend("backend exists callback missing"))?;
        let c_key = Self::cstr(key)?;
        let rc = unsafe { exists(c_key.as_ptr(), self.callbacks.userdata) };
        match rc {
            1 => Ok(true),
            0 => Ok(false),
            _ => Err(ChunkStoreError::backend(format!(
                "exists failed for key {key}"
            ))),
        }
    }

    fn delete(&self, key: &str) -> Result<(), ChunkStoreError> {
        let delete = self
            .callbacks
            .delete
            .ok_or_else(|| ChunkStoreError::backend("backend delete callback missing"))?;
        let c_key = Self::cstr(key)?;
        let rc = unsafe { delete(c_key.as_ptr(), self.callbacks.userdata) };
        if rc != CHUNKSTORE_OK {
            return Err(ChunkStoreError::backend(format!(
                "delete failed for key {key}"
            )));
        }
        Ok(())
    }
}

struct FfiStore {
    inner: FfiStoreInner,
}

enum FfiStoreInner {
    Callback(ChunkStore<Arc<CallbackBackend>>),
    Fs(ChunkStore<FsBackend>),
}

impl FfiStore {
    fn ingest(&self, file_id: &str, data: &[u8]) -> Result<Vec<String>, ChunkStoreError> {
        match &self.inner {
            FfiStoreInner::Callback(store) => store.ingest(file_id, data),
            FfiStoreInner::Fs(store) => store.ingest(file_id, data),
        }
    }

    fn ingest_cdc(&self, file_id: &str, data: &[u8]) -> Result<Vec<String>, ChunkStoreError> {
        match &self.inner {
            FfiStoreInner::Callback(store) => store.ingest_cdc(file_id, data),
            FfiStoreInner::Fs(store) => store.ingest_cdc(file_id, data),
        }
    }

    fn read(&self, file_id: &str) -> Result<Vec<u8>, ChunkStoreError> {
        match &self.inner {
            FfiStoreInner::Callback(store) => store.read(file_id),
            FfiStoreInner::Fs(store) => store.read(file_id),
        }
    }

    fn delete(&self, file_id: &str) -> Result<(), ChunkStoreError> {
        match &self.inner {
            FfiStoreInner::Callback(store) => store.delete(file_id),
            FfiStoreInner::Fs(store) => store.delete(file_id),
        }
    }

    fn stats(&self) -> Result<crate::store::Stats, ChunkStoreError> {
        match &self.inner {
            FfiStoreInner::Callback(store) => store.stats(),
            FfiStoreInner::Fs(store) => store.stats(),
        }
    }
}

/// Create a new store backed by C callbacks. Returns null on invalid input.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_create(
    callbacks: *const ChunkBackendCallbacks,
) -> *mut ChunkStoreHandle {
    if callbacks.is_null() {
        return ptr::null_mut();
    }
    let callbacks = *callbacks;
    let backend = Arc::new(CallbackBackend { callbacks });
    match ChunkStore::open(backend) {
        Ok(store) => Box::into_raw(Box::new(FfiStore {
            inner: FfiStoreInner::Callback(store),
        })) as *mut ChunkStoreHandle,
        Err(_) => ptr::null_mut(),
    }
}

/// Open a store with the built-in filesystem backend.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_open_fs(root: *const c_char) -> *mut ChunkStoreHandle {
    if root.is_null() {
        return ptr::null_mut();
    }
    let root = match CStr::from_ptr(root).to_str() {
        Ok(path) => path,
        Err(_) => return ptr::null_mut(),
    };
    let backend = match FsBackend::new(root) {
        Ok(backend) => backend,
        Err(_) => return ptr::null_mut(),
    };
    match ChunkStore::open(backend) {
        Ok(store) => Box::into_raw(Box::new(FfiStore {
            inner: FfiStoreInner::Fs(store),
        })) as *mut ChunkStoreHandle,
        Err(_) => ptr::null_mut(),
    }
}

/// Destroy a store created by `chunkstore_create`.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_destroy(store: *mut ChunkStoreHandle) {
    if store.is_null() {
        return;
    }
    drop(Box::from_raw(store as *mut FfiStore));
}

/// Ingest raw file bytes with fixed-size chunking.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_ingest(
    store: *mut ChunkStoreHandle,
    file_id: *const c_char,
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
) -> c_int {
    ingest_impl(
        store,
        file_id,
        data,
        len,
        out_err,
        |store, file_id, data| store.ingest(file_id, data),
    )
}

/// Ingest raw file bytes with CDC chunking.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_ingest_cdc(
    store: *mut ChunkStoreHandle,
    file_id: *const c_char,
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
) -> c_int {
    ingest_impl(
        store,
        file_id,
        data,
        len,
        out_err,
        |store, file_id, data| store.ingest_cdc(file_id, data),
    )
}

/// Read assembled file bytes. Caller must free with `chunkstore_bytes_free`.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_read(
    store: *mut ChunkStoreHandle,
    file_id: *const c_char,
    out_data: *mut *mut u8,
    out_len: *mut usize,
    out_err: *mut *mut c_char,
) -> c_int {
    if store.is_null() || file_id.is_null() || out_data.is_null() || out_len.is_null() {
        return CHUNKSTORE_ERR;
    }

    let store = &mut *(store as *mut FfiStore);
    let file_id = match CStr::from_ptr(file_id).to_str() {
        Ok(s) => s,
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            return CHUNKSTORE_ERR;
        }
    };

    match store.read(file_id) {
        Ok(bytes) => {
            let len = bytes.len();
            let ptr = bytes.as_ptr();
            std::mem::forget(bytes);
            *out_data = ptr as *mut u8;
            *out_len = len;
            CHUNKSTORE_OK
        }
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            CHUNKSTORE_ERR
        }
    }
}

/// Delete a file and GC unreferenced chunks.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_delete(
    store: *mut ChunkStoreHandle,
    file_id: *const c_char,
    out_err: *mut *mut c_char,
) -> c_int {
    if store.is_null() || file_id.is_null() {
        return CHUNKSTORE_ERR;
    }

    let store = &mut *(store as *mut FfiStore);
    let file_id = match CStr::from_ptr(file_id).to_str() {
        Ok(s) => s,
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            return CHUNKSTORE_ERR;
        }
    };

    match store.delete(file_id) {
        Ok(()) => CHUNKSTORE_OK,
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            CHUNKSTORE_ERR
        }
    }
}

/// Return dedup statistics for the store.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_stats(
    store: *mut ChunkStoreHandle,
    out_stats: *mut ChunkstoreStats,
    out_err: *mut *mut c_char,
) -> c_int {
    if store.is_null() || out_stats.is_null() {
        return CHUNKSTORE_ERR;
    }

    let store = &mut *(store as *mut FfiStore);
    match store.stats() {
        Ok(stats) => {
            (*out_stats).total_bytes = stats.total_bytes;
            (*out_stats).stored_bytes = stats.stored_bytes;
            (*out_stats).savings_pct = stats.savings_pct();
            CHUNKSTORE_OK
        }
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            CHUNKSTORE_ERR
        }
    }
}

/// Allocate a buffer for backend `get` callbacks. Free with `chunkstore_bytes_free`.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_bytes_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return ptr::null_mut();
    }
    let mut bytes = vec![0; len];
    let ptr = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    ptr
}

/// Free buffers allocated by the Rust FFI layer.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_bytes_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(ptr, len, len));
}

/// Free error strings allocated by the Rust FFI layer.
#[no_mangle]
pub unsafe extern "C" fn chunkstore_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    drop(std::ffi::CString::from_raw(ptr));
}

unsafe fn ingest_impl<F>(
    store: *mut ChunkStoreHandle,
    file_id: *const c_char,
    data: *const u8,
    len: usize,
    out_err: *mut *mut c_char,
    f: F,
) -> c_int
where
    F: FnOnce(&mut FfiStore, &str, &[u8]) -> Result<Vec<String>, ChunkStoreError>,
{
    if store.is_null() || file_id.is_null() || data.is_null() {
        return CHUNKSTORE_ERR;
    }

    let store = &mut *(store as *mut FfiStore);
    let file_id = match CStr::from_ptr(file_id).to_str() {
        Ok(s) => s,
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            return CHUNKSTORE_ERR;
        }
    };
    let data = std::slice::from_raw_parts(data, len);

    match f(store, file_id, data) {
        Ok(_) => CHUNKSTORE_OK,
        Err(e) => {
            write_ffi_error(out_err, e.to_string());
            CHUNKSTORE_ERR
        }
    }
}

unsafe fn write_ffi_error(out_err: *mut *mut c_char, message: String) {
    if out_err.is_null() {
        return;
    }
    if let Ok(c) = std::ffi::CString::new(message) {
        *out_err = c.into_raw();
    }
}
