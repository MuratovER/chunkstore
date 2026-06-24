#![allow(unsafe_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

use chunkstore::ffi::{
    chunkstore_destroy, chunkstore_ingest_cdc_with_digests, chunkstore_ingest_fixed,
    chunkstore_ingest_with_digests, chunkstore_open_fs, chunkstore_read, chunkstore_string_free,
    CHUNKSTORE_OK,
};
use tempfile::tempdir;

unsafe fn free_err(err: *mut c_char) {
    if !err.is_null() {
        chunkstore_string_free(err);
    }
}

unsafe fn free_digests_json(json: *mut c_char) {
    if !json.is_null() {
        chunkstore_string_free(json);
    }
}

#[test]
fn ffi_ingest_with_digests_roundtrip() {
    let dir = tempdir().expect("tempdir");
    let root = CString::new(dir.path().to_str().unwrap()).unwrap();
    let payload = b"ffi-digest-payload";

    unsafe {
        let store = chunkstore_open_fs(root.as_ptr());
        assert!(!store.is_null());

        let file_id = CString::new("doc").unwrap();
        let mut digests_json: *mut c_char = ptr::null_mut();
        let mut err: *mut c_char = ptr::null_mut();

        let rc = chunkstore_ingest_with_digests(
            store,
            file_id.as_ptr(),
            payload.as_ptr(),
            payload.len(),
            &mut digests_json,
            &mut err,
        );
        assert_eq!(
            rc,
            CHUNKSTORE_OK,
            "err={}",
            CStr::from_ptr(err).to_str().unwrap_or("")
        );
        free_err(err);

        let json = CStr::from_ptr(digests_json).to_str().unwrap();
        let digests: Vec<String> = serde_json::from_str(json).expect("parse digests json");
        assert_eq!(digests.len(), 1);
        assert_eq!(digests[0].len(), 64);
        free_digests_json(digests_json);

        let mut out_data: *mut u8 = ptr::null_mut();
        let mut out_len = 0usize;
        err = ptr::null_mut();
        let rc = chunkstore_read(
            store,
            file_id.as_ptr(),
            &mut out_data,
            &mut out_len,
            &mut err,
        );
        assert_eq!(
            rc,
            CHUNKSTORE_OK,
            "err={}",
            CStr::from_ptr(err).to_str().unwrap_or("")
        );
        free_err(err);

        let got = std::slice::from_raw_parts(out_data, out_len);
        assert_eq!(got, payload);
        chunkstore::ffi::chunkstore_bytes_free(out_data, out_len);

        chunkstore_destroy(store);
    }
}

#[test]
fn ffi_ingest_fixed_custom_chunk_size() {
    let dir = tempdir().expect("tempdir");
    let root = CString::new(dir.path().to_str().unwrap()).unwrap();
    let chunk_size = 64usize;
    let payload = vec![0xABu8; chunk_size * 2 + 10];

    unsafe {
        let store = chunkstore_open_fs(root.as_ptr());
        assert!(!store.is_null());

        let file_id = CString::new("parts").unwrap();
        let mut digests_json: *mut c_char = ptr::null_mut();
        let mut err: *mut c_char = ptr::null_mut();

        let rc = chunkstore_ingest_fixed(
            store,
            file_id.as_ptr(),
            payload.as_ptr(),
            payload.len(),
            chunk_size,
            &mut digests_json,
            &mut err,
        );
        assert_eq!(
            rc,
            CHUNKSTORE_OK,
            "err={}",
            CStr::from_ptr(err).to_str().unwrap_or("")
        );
        free_err(err);

        let json = CStr::from_ptr(digests_json).to_str().unwrap();
        let digests: Vec<String> = serde_json::from_str(json).expect("parse digests json");
        assert_eq!(digests.len(), 3);
        free_digests_json(digests_json);

        chunkstore_destroy(store);
    }
}

#[test]
fn ffi_ingest_cdc_with_digests() {
    let dir = tempdir().expect("tempdir");
    let root = CString::new(dir.path().to_str().unwrap()).unwrap();
    let payload = vec![0xCDu8; 600 * 1024];

    unsafe {
        let store = chunkstore_open_fs(root.as_ptr());
        assert!(!store.is_null());

        let file_id = CString::new("cdc").unwrap();
        let mut digests_json: *mut c_char = ptr::null_mut();
        let mut err: *mut c_char = ptr::null_mut();

        let rc = chunkstore_ingest_cdc_with_digests(
            store,
            file_id.as_ptr(),
            payload.as_ptr(),
            payload.len(),
            &mut digests_json,
            &mut err,
        );
        assert_eq!(
            rc,
            CHUNKSTORE_OK,
            "err={}",
            CStr::from_ptr(err).to_str().unwrap_or("")
        );
        free_err(err);

        let json = CStr::from_ptr(digests_json).to_str().unwrap();
        let digests: Vec<String> = serde_json::from_str(json).expect("parse digests json");
        assert!(!digests.is_empty());
        free_digests_json(digests_json);

        chunkstore_destroy(store);
    }
}
