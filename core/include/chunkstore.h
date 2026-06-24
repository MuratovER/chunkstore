#ifndef CHUNKSTORE_H
#define CHUNKSTORE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    int (*get)(const char *key, uint8_t **out_data, size_t *out_len, void *userdata);
    int (*put)(const char *key, const uint8_t *data, size_t len, void *userdata);
    int (*exists)(const char *key, void *userdata);
    int (*delete)(const char *key, void *userdata);
    void *userdata;
} ChunkBackendCallbacks;

typedef struct ChunkStoreHandle ChunkStoreHandle;

typedef struct {
    uint64_t total_bytes;
    uint64_t stored_bytes;
    double savings_pct;
} ChunkstoreStats;

#define CHUNKSTORE_OK 0
#define CHUNKSTORE_ERR -1

ChunkStoreHandle *chunkstore_create(const ChunkBackendCallbacks *callbacks);
ChunkStoreHandle *chunkstore_open_fs(const char *root);
void chunkstore_destroy(ChunkStoreHandle *store);

int chunkstore_ingest(
    ChunkStoreHandle *store,
    const char *file_id,
    const uint8_t *data,
    size_t len,
    char **out_err);

int chunkstore_ingest_cdc(
    ChunkStoreHandle *store,
    const char *file_id,
    const uint8_t *data,
    size_t len,
    char **out_err);

int chunkstore_ingest_with_digests(
    ChunkStoreHandle *store,
    const char *file_id,
    const uint8_t *data,
    size_t len,
    char **out_digests_json,
    char **out_err);

int chunkstore_ingest_fixed(
    ChunkStoreHandle *store,
    const char *file_id,
    const uint8_t *data,
    size_t len,
    size_t chunk_size,
    char **out_digests_json,
    char **out_err);

int chunkstore_ingest_cdc_with_digests(
    ChunkStoreHandle *store,
    const char *file_id,
    const uint8_t *data,
    size_t len,
    char **out_digests_json,
    char **out_err);

int chunkstore_read(
    ChunkStoreHandle *store,
    const char *file_id,
    uint8_t **out_data,
    size_t *out_len,
    char **out_err);

int chunkstore_delete(ChunkStoreHandle *store, const char *file_id, char **out_err);

int chunkstore_stats(ChunkStoreHandle *store, ChunkstoreStats *out_stats, char **out_err);

void chunkstore_bytes_free(uint8_t *ptr, size_t len);
uint8_t *chunkstore_bytes_alloc(size_t len);
void chunkstore_string_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif /* CHUNKSTORE_H */
