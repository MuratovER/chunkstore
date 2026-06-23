use std::os::raw::{c_char, c_int, c_void};

/// Backend callback table passed from language wrappers (Python, Go).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct ChunkBackendCallbacks {
    pub get: Option<
        unsafe extern "C" fn(
            key: *const c_char,
            out_data: *mut *mut u8,
            out_len: *mut usize,
            userdata: *mut c_void,
        ) -> c_int,
    >,
    pub put: Option<
        unsafe extern "C" fn(
            key: *const c_char,
            data: *const u8,
            len: usize,
            userdata: *mut c_void,
        ) -> c_int,
    >,
    pub exists: Option<unsafe extern "C" fn(key: *const c_char, userdata: *mut c_void) -> c_int>,
    pub delete: Option<unsafe extern "C" fn(key: *const c_char, userdata: *mut c_void) -> c_int>,
    pub userdata: *mut c_void,
}

/// Dedup statistics returned through the C API.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct ChunkstoreStats {
    pub total_bytes: u64,
    pub stored_bytes: u64,
    pub savings_pct: f64,
}

/// Opaque store handle for FFI consumers.
pub type ChunkStoreHandle = c_void;

/// FFI success / error codes.
pub const CHUNKSTORE_OK: c_int = 0;
pub const CHUNKSTORE_ERR: c_int = -1;

impl std::fmt::Debug for ChunkBackendCallbacks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkBackendCallbacks")
            .field("userdata", &self.userdata)
            .finish_non_exhaustive()
    }
}
