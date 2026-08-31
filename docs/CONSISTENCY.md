# Consistency model

How chunkstore treats **chunk blobs** vs **metadata** (manifests and refcounts), and what is safe to run concurrently.

This document is required reading before using multi-process or multi-node deployments.

## Two stores, one file

| Layer | What it holds | Default location |
|-------|---------------|------------------|
| Blob store | Chunk payloads keyed by SHA-256 hex | Filesystem or S3 (`ChunkBackend`) |
| Metadata | `file_id → [digest…]`, `digest → (refcount, size)` | Same blob backend under `_manifest/` and `_refcount/` |

Ingest writes blobs first, then commits metadata. Delete commits metadata (refcount--) first, then unlinks blobs that reached zero. If a crash happens in between, [fsck](#crash-and-fsck) can drop orphan blobs.

Content addressing is always SHA-256 of **plaintext**. Optional [encryption hooks](#encryption-hooks) wrap only the stored blob bytes.

## Deployment modes

### 1. Single process (default, v0.1–v0.3)

One `ChunkStore` instance, in-memory mutexes around metadata, blobs on FS or S3.

- Concurrent `ingest` / `delete` on **that instance** is safe (one lock for metadata updates; blob exists/put is serialized per digest).
- `stats()` is exact for that process.
- Safe for embeddable use: FastAPI one worker, a CLI, a library inside one VM.

**Not safe:** two processes (or two Gunicorn workers) opening the same backend. Each keeps its own in-memory refcount cache and last-write-wins JSON on the blob store. Parallel ingest of the same digest can lose a refcount update.

### 2. Single writer, many readers

One writer process, readers that only call `read` / `read_to_writer`.

Readers still load metadata at `open()` and **do not** see later ingest/delete until they reopen. Treat this as a snapshot, not live shared state.

### 3. Multi-process / multi-node (v0.4, Postgres)

`ChunkStore::open_postgres(backend, dsn)` keeps **blobs** on FS/S3 and **metadata** in Postgres.

- Manifest replace + refcount increment/decrement run in **one SQL transaction**.
- Two nodes can ingest/delete the same `file_id` or shared chunks without lost updates.
- `stats()` is computed from Postgres (`SUM(size)`, `SUM(size * count)`), so it is cluster-wide.
- Blob `put` remains idempotent (content-addressed). A race that writes the same digest twice is fine.

Connect with `NoTls`. Terminate TLS in front of Postgres (cloud proxy, stunnel) or stay on a private network.

Python and Go wrappers still use blob-backed metadata (mode 1). Shared Postgres metadata is on the Rust crate: `ChunkStore::open_postgres` with `--features postgres`.

Redis is **not** implemented: multi-key ingest is not atomic with simple `INCR` without a Lua/MULTI design. Postgres is the supported shared-metadata backend.

## Ingest commit order

1. Stream-chunk the input (fixed or CDC). Each completed chunk is hashed and stored immediately — payloads are not held as a full-file `Vec`.
2. Peek the previous manifest for `file_id` (if any).
3. Commit metadata: increment new digests, upsert the new manifest, decrement the old digests.
4. Unlink blobs whose refcount hit zero.

Replacing a file therefore cannot drop the old bytes before the new ones are durable. A crash during step 1 may leave **orphan blobs** (no refcount); fsck removes them. A crash during step 3 rolls back the SQL transaction (Postgres) or may leave a partial JSON write on the blob backend (single-process mode).

## Crash and fsck

`ChunkStore::fsck(repair)`:

- **Missing blobs:** digest has a refcount but `backend.exists(digest)` is false — data loss; repair cannot invent bytes.
- **Orphan blobs:** 64-char keys on the backend with no refcount — safe to delete when `repair` is true.
- Backends that cannot list keys (`Ok(None)` from `list_keys`) skip orphan detection.

Run fsck after a crash or as periodic compaction. It is not a substitute for Postgres transactions.

## Encryption hooks

`ChunkCodec` encodes/decodes blob bytes. Digests stay SHA-256 of plaintext, so identical files still dedup. The library ships `IdentityCodec` (prefer `open()` with no codec to avoid copies). Bring your own AEAD; this is not a KMS product.

## What we will not do

- Multi-node refcount on S3/JSON alone (lost-update).
- Distributed locking in the blob backend.
- Treating `stats().gc_chunks` as a cluster metric (it counts GC in the local process).
