// Minimal HTTP service demonstrating the Go chunkstore wrapper with a filesystem backend.
package main

import (
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"github.com/chunkstore/chunkstore/go/chunkstore"
)

type statsJSON struct {
	TotalBytes  uint64  `json:"total_bytes"`
	StoredBytes uint64  `json:"stored_bytes"`
	SavingsPct  float64 `json:"savings_pct"`
}

type uploadJSON struct {
	FileID string    `json:"file_id"`
	Stats  statsJSON `json:"stats"`
}

type deleteJSON struct {
	Deleted string `json:"deleted"`
}

type server struct {
	store *chunkstore.Store
}

func (s *server) routes() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /files/{file_id}", s.upload)
	mux.HandleFunc("GET /files/{file_id}", s.download)
	mux.HandleFunc("DELETE /files/{file_id}", s.delete)
	mux.HandleFunc("GET /stats", s.stats)
	return mux
}

func (s *server) upload(w http.ResponseWriter, r *http.Request) {
	fileID := r.PathValue("file_id")
	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	if err := s.store.Ingest(fileID, body); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	stats, err := s.store.Stats()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, uploadJSON{
		FileID: fileID,
		Stats:  toStatsJSON(stats),
	})
}

func (s *server) download(w http.ResponseWriter, r *http.Request) {
	fileID := r.PathValue("file_id")
	data, err := s.store.Read(fileID)
	if err != nil {
		if isNotFound(err) {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/octet-stream")
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(data)
}

func (s *server) delete(w http.ResponseWriter, r *http.Request) {
	fileID := r.PathValue("file_id")
	if err := s.store.Delete(fileID); err != nil {
		if isNotFound(err) {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, deleteJSON{Deleted: fileID})
}

func (s *server) stats(w http.ResponseWriter, r *http.Request) {
	stats, err := s.store.Stats()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, toStatsJSON(stats))
}

func toStatsJSON(stats chunkstore.Stats) statsJSON {
	return statsJSON{
		TotalBytes:  stats.TotalBytes,
		StoredBytes: stats.StoredBytes,
		SavingsPct:  stats.SavingsPct,
	}
}

func writeJSON(w http.ResponseWriter, status int, payload any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(payload); err != nil {
		log.Printf("encode json: %v", err)
	}
}

func isNotFound(err error) bool {
	return strings.Contains(strings.ToLower(err.Error()), "not found")
}

func main() {
	dataRoot := os.Getenv("CHUNKSTORE_DATA_DIR")
	if dataRoot == "" {
		dataRoot = filepath.Join(os.TempDir(), "chunkstore-go-http-data")
	}
	chunksDir := filepath.Join(dataRoot, "chunks")

	store, err := chunkstore.OpenFilesystem(chunksDir)
	if err != nil {
		log.Fatalf("open store: %v", err)
	}
	defer store.Close()

	addr := ":8080"
	if port := os.Getenv("PORT"); port != "" {
		addr = ":" + port
	}

	srv := &server{store: store}
	log.Printf("listening on %s (data: %s)", addr, chunksDir)
	log.Fatal(http.ListenAndServe(addr, srv.routes()))
}
