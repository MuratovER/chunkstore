package chunkstore

// Stats reports deduplication metrics from the store.
type Stats struct {
	TotalBytes  uint64
	StoredBytes uint64
	SavingsPct  float64
}
