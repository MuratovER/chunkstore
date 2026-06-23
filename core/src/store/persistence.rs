use serde::{Deserialize, Serialize};

use crate::error::ChunkStoreError;
use crate::store::{ChunkBackend, Manifest, RefCount};

fn validate_digest(digest: &str) -> Result<(), ChunkStoreError> {
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ChunkStoreError::invalid_argument(format!(
            "invalid digest: {digest}"
        )));
    }
    Ok(())
}

pub(crate) const MANIFEST_PREFIX: &str = "_manifest/";
pub(crate) const REFCOUNT_PREFIX: &str = "_refcount/";
pub(crate) const MANIFEST_INDEX_KEY: &str = "_manifest/__index__";
pub(crate) const REFCOUNT_INDEX_KEY: &str = "_refcount/__index__";

#[derive(Debug, Serialize, Deserialize)]
struct ManifestRecord {
    digests: Vec<String>,
    file_bytes: u64,
}

pub(crate) fn manifest_key(file_id: &str) -> String {
    format!("{MANIFEST_PREFIX}{file_id}")
}

pub(crate) fn refcount_key(digest: &str) -> String {
    format!("{REFCOUNT_PREFIX}{digest}")
}

pub(crate) fn load_manifests<B: ChunkBackend>(backend: &B) -> Result<Manifest, ChunkStoreError> {
    let index = read_string_list(backend, MANIFEST_INDEX_KEY)?;
    let mut manifest = Manifest::new();

    for file_id in index {
        let key = manifest_key(&file_id);
        let raw = backend
            .get(&key)?
            .ok_or_else(|| ChunkStoreError::backend(format!("missing manifest key {key}")))?;
        let record: ManifestRecord = serde_json::from_slice(&raw).map_err(|e| {
            ChunkStoreError::backend(format!("invalid manifest JSON for {file_id}: {e}"))
        })?;
        for digest in &record.digests {
            validate_digest(digest)?;
        }
        manifest.insert(&file_id, record.digests, record.file_bytes)?;
    }

    Ok(manifest)
}

pub(crate) fn load_refcounts<B: ChunkBackend>(backend: &B) -> Result<RefCount, ChunkStoreError> {
    let index = read_string_list(backend, REFCOUNT_INDEX_KEY)?;
    let mut refcount = RefCount::new();

    for digest in index {
        validate_digest(&digest)?;
        let key = refcount_key(&digest);
        let raw = backend
            .get(&key)?
            .ok_or_else(|| ChunkStoreError::backend(format!("missing refcount key {key}")))?;
        let count: u64 = serde_json::from_slice(&raw).map_err(|e| {
            ChunkStoreError::backend(format!("invalid refcount JSON for {digest}: {e}"))
        })?;
        if count > 0 {
            refcount.set(&digest, count);
        }
    }

    Ok(refcount)
}

pub(crate) fn save_manifest<B: ChunkBackend>(
    backend: &B,
    file_id: &str,
    digests: &[String],
    file_bytes: u64,
) -> Result<(), ChunkStoreError> {
    for digest in digests {
        validate_digest(digest)?;
    }
    let record = ManifestRecord {
        digests: digests.to_vec(),
        file_bytes,
    };
    let payload = serde_json::to_vec(&record)
        .map_err(|e| ChunkStoreError::backend(format!("manifest encode failed: {e}")))?;
    backend.put(&manifest_key(file_id), &payload)?;

    let mut index = read_string_list(backend, MANIFEST_INDEX_KEY)?;
    if !index.iter().any(|id| id == file_id) {
        index.push(file_id.to_string());
        write_string_list(backend, MANIFEST_INDEX_KEY, &index)?;
    }
    Ok(())
}

pub(crate) fn remove_manifest<B: ChunkBackend>(
    backend: &B,
    file_id: &str,
) -> Result<(), ChunkStoreError> {
    backend.delete(&manifest_key(file_id))?;
    let mut index = read_string_list(backend, MANIFEST_INDEX_KEY)?;
    index.retain(|id| id != file_id);
    write_string_list(backend, MANIFEST_INDEX_KEY, &index)?;
    Ok(())
}

pub(crate) fn save_refcount<B: ChunkBackend>(
    backend: &B,
    digest: &str,
    count: u64,
) -> Result<(), ChunkStoreError> {
    validate_digest(digest)?;
    let payload = serde_json::to_vec(&count)
        .map_err(|e| ChunkStoreError::backend(format!("refcount encode failed: {e}")))?;
    backend.put(&refcount_key(digest), &payload)?;

    let mut index = read_string_list(backend, REFCOUNT_INDEX_KEY)?;
    if !index.iter().any(|d| d == digest) {
        index.push(digest.to_string());
        write_string_list(backend, REFCOUNT_INDEX_KEY, &index)?;
    }
    Ok(())
}

pub(crate) fn remove_refcount<B: ChunkBackend>(
    backend: &B,
    digest: &str,
) -> Result<(), ChunkStoreError> {
    backend.delete(&refcount_key(digest))?;
    let mut index = read_string_list(backend, REFCOUNT_INDEX_KEY)?;
    index.retain(|d| d != digest);
    write_string_list(backend, REFCOUNT_INDEX_KEY, &index)?;
    Ok(())
}

fn read_string_list<B: ChunkBackend>(
    backend: &B,
    key: &str,
) -> Result<Vec<String>, ChunkStoreError> {
    match backend.get(key)? {
        Some(raw) => serde_json::from_slice(&raw)
            .map_err(|e| ChunkStoreError::backend(format!("invalid index JSON at {key}: {e}"))),
        None => Ok(Vec::new()),
    }
}

fn write_string_list<B: ChunkBackend>(
    backend: &B,
    key: &str,
    values: &[String],
) -> Result<(), ChunkStoreError> {
    let payload = serde_json::to_vec(values)
        .map_err(|e| ChunkStoreError::backend(format!("index encode failed: {e}")))?;
    backend.put(key, &payload)
}
