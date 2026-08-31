//! Postgres-backed manifests and refcounts for multi-process deployments.
//!
//! Enable with `--features postgres`. See [docs/CONSISTENCY.md](../../../docs/CONSISTENCY.md).

use std::sync::Mutex;

use postgres::{Client, NoTls, Transaction};
use serde_json::Value;

use crate::error::ChunkStoreError;
use crate::store::{Manifest, RefCount, Stats};

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS chunkstore_manifests (
    file_id TEXT PRIMARY KEY,
    digests JSONB NOT NULL,
    file_bytes BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS chunkstore_refcounts (
    digest TEXT PRIMARY KEY,
    count BIGINT NOT NULL CHECK (count > 0),
    size BIGINT NOT NULL CHECK (size >= 0)
);
";

/// Shared metadata store. All mutating methods run inside a SQL transaction.
pub struct PostgresMetadata {
    client: Mutex<Client>,
}

impl std::fmt::Debug for PostgresMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresMetadata").finish_non_exhaustive()
    }
}

impl PostgresMetadata {
    /// Connect with `NoTls` and ensure schema exists.
    ///
    /// Use a local/private network or a TLS terminator in front of Postgres.
    pub fn connect(dsn: &str) -> Result<Self, ChunkStoreError> {
        let mut client = Client::connect(dsn, NoTls)
            .map_err(|e| ChunkStoreError::backend(format!("postgres connect: {e}")))?;
        client
            .batch_execute(SCHEMA)
            .map_err(|e| ChunkStoreError::backend(format!("postgres schema: {e}")))?;
        Ok(Self {
            client: Mutex::new(client),
        })
    }

    pub fn load_manifests(&self) -> Result<Manifest, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let rows = client
            .query(
                "SELECT file_id, digests, file_bytes FROM chunkstore_manifests",
                &[],
            )
            .map_err(|e| ChunkStoreError::backend(format!("postgres load manifests: {e}")))?;
        let mut manifest = Manifest::new();
        for row in rows {
            let file_id: String = row.get(0);
            let digests = json_digests(row.get(1))?;
            let file_bytes: i64 = row.get(2);
            manifest.insert(&file_id, digests, file_bytes as u64)?;
        }
        Ok(manifest)
    }

    pub fn load_refcounts(&self) -> Result<RefCount, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let rows = client
            .query("SELECT digest, count, size FROM chunkstore_refcounts", &[])
            .map_err(|e| ChunkStoreError::backend(format!("postgres load refcounts: {e}")))?;
        let mut refcount = RefCount::new();
        for row in rows {
            let digest: String = row.get(0);
            let count: i64 = row.get(1);
            let size: i64 = row.get(2);
            if count > 0 {
                refcount.set(digest.trim(), count as u64, size as u64);
            }
        }
        Ok(refcount)
    }

    pub fn manifest_digests(&self, file_id: &str) -> Result<Option<Vec<String>>, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let row = client
            .query_opt(
                "SELECT digests FROM chunkstore_manifests WHERE file_id = $1",
                &[&file_id],
            )
            .map_err(|e| ChunkStoreError::backend(format!("postgres get manifest: {e}")))?;
        match row {
            Some(row) => Ok(Some(json_digests(row.get(0))?)),
            None => Ok(None),
        }
    }

    pub fn compute_stats(&self) -> Result<Stats, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let row = client
            .query_one(
                "SELECT COALESCE(SUM(size * count), 0)::BIGINT, \
                        COALESCE(SUM(size), 0)::BIGINT \
                 FROM chunkstore_refcounts",
                &[],
            )
            .map_err(|e| ChunkStoreError::backend(format!("postgres stats: {e}")))?;
        let total: i64 = row.get(0);
        let stored: i64 = row.get(1);
        Ok(Stats {
            total_bytes: total as u64,
            stored_bytes: stored as u64,
            gc_chunks: 0,
        })
    }

    /// Increment new chunks, replace manifest, decrement previous file's chunks.
    /// Returns digests whose refcount hit zero (caller should delete blobs).
    pub fn commit_ingest(
        &self,
        file_id: &str,
        new_digests: &[String],
        file_bytes: u64,
        sizes: &[u64],
        old_digests: Option<&[String]>,
    ) -> Result<Vec<String>, ChunkStoreError> {
        if new_digests.len() != sizes.len() {
            return Err(ChunkStoreError::invalid_argument(
                "digest/size length mismatch",
            ));
        }
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let mut txn = client
            .transaction()
            .map_err(|e| ChunkStoreError::backend(format!("postgres begin: {e}")))?;

        for (digest, size) in new_digests.iter().zip(sizes.iter()) {
            increment_refcount(&mut txn, digest, *size)?;
        }

        upsert_manifest(&mut txn, file_id, new_digests, file_bytes)?;

        let mut gc = Vec::new();
        if let Some(old) = old_digests {
            for digest in old {
                if decrement_refcount(&mut txn, digest)? == 0 {
                    gc.push(digest.clone());
                }
            }
        }

        txn.commit()
            .map_err(|e| ChunkStoreError::backend(format!("postgres commit ingest: {e}")))?;
        Ok(gc)
    }

    /// Remove manifest and decrement its chunks. Returns digests to GC.
    pub fn commit_delete(
        &self,
        file_id: &str,
        digests: &[String],
    ) -> Result<Vec<String>, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let mut txn = client
            .transaction()
            .map_err(|e| ChunkStoreError::backend(format!("postgres begin: {e}")))?;

        txn.execute(
            "DELETE FROM chunkstore_manifests WHERE file_id = $1",
            &[&file_id],
        )
        .map_err(|e| ChunkStoreError::backend(format!("postgres delete manifest: {e}")))?;

        let mut gc = Vec::new();
        for digest in digests {
            if decrement_refcount(&mut txn, digest)? == 0 {
                gc.push(digest.clone());
            }
        }

        txn.commit()
            .map_err(|e| ChunkStoreError::backend(format!("postgres commit delete: {e}")))?;
        Ok(gc)
    }

    pub fn chunk_size(&self, digest: &str) -> Result<Option<u64>, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let row = client
            .query_opt(
                "SELECT size FROM chunkstore_refcounts WHERE digest = $1",
                &[&digest],
            )
            .map_err(|e| ChunkStoreError::backend(format!("postgres chunk size: {e}")))?;
        Ok(row.map(|r| r.get::<_, i64>(0) as u64))
    }

    pub fn list_refcounts(&self) -> Result<Vec<(String, u64, u64)>, ChunkStoreError> {
        let mut client = self.client.lock().map_err(|_| ChunkStoreError::LockError)?;
        let rows = client
            .query("SELECT digest, count, size FROM chunkstore_refcounts", &[])
            .map_err(|e| ChunkStoreError::backend(format!("postgres list refcounts: {e}")))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let digest: String = row.get(0);
                let count: i64 = row.get(1);
                let size: i64 = row.get(2);
                (digest.trim().to_string(), count as u64, size as u64)
            })
            .collect())
    }
}

fn json_digests(value: Value) -> Result<Vec<String>, ChunkStoreError> {
    serde_json::from_value(value)
        .map_err(|e| ChunkStoreError::backend(format!("postgres digest json: {e}")))
}

fn upsert_manifest(
    txn: &mut Transaction<'_>,
    file_id: &str,
    digests: &[String],
    file_bytes: u64,
) -> Result<(), ChunkStoreError> {
    let payload = serde_json::to_value(digests)
        .map_err(|e| ChunkStoreError::backend(format!("postgres digest encode: {e}")))?;
    let bytes = file_bytes as i64;
    txn.execute(
        "INSERT INTO chunkstore_manifests (file_id, digests, file_bytes) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (file_id) DO UPDATE SET digests = EXCLUDED.digests, \
             file_bytes = EXCLUDED.file_bytes",
        &[&file_id, &payload, &bytes],
    )
    .map_err(|e| ChunkStoreError::backend(format!("postgres upsert manifest: {e}")))?;
    Ok(())
}

fn increment_refcount(
    txn: &mut Transaction<'_>,
    digest: &str,
    size: u64,
) -> Result<u64, ChunkStoreError> {
    let size = size as i64;
    let row = txn
        .query_one(
            "INSERT INTO chunkstore_refcounts (digest, count, size) VALUES ($1, 1, $2) \
             ON CONFLICT (digest) DO UPDATE SET count = chunkstore_refcounts.count + 1 \
             RETURNING count",
            &[&digest, &size],
        )
        .map_err(|e| ChunkStoreError::backend(format!("postgres increment: {e}")))?;
    let count: i64 = row.get(0);
    Ok(count as u64)
}

fn decrement_refcount(txn: &mut Transaction<'_>, digest: &str) -> Result<u64, ChunkStoreError> {
    let row = txn
        .query_opt(
            "UPDATE chunkstore_refcounts SET count = count - 1 \
             WHERE digest = $1 RETURNING count",
            &[&digest],
        )
        .map_err(|e| ChunkStoreError::backend(format!("postgres decrement: {e}")))?;
    let Some(row) = row else {
        return Err(ChunkStoreError::not_found(format!("refcount {digest}")));
    };
    let remaining: i64 = row.get(0);
    if remaining <= 0 {
        txn.execute(
            "DELETE FROM chunkstore_refcounts WHERE digest = $1",
            &[&digest],
        )
        .map_err(|e| ChunkStoreError::backend(format!("postgres delete refcount: {e}")))?;
        return Ok(0);
    }
    Ok(remaining as u64)
}
