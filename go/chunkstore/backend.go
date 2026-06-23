package chunkstore

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// FilesystemBackend stores chunk blobs and metadata on disk.
type FilesystemBackend struct {
	root string
}

// NewFilesystemBackend creates a filesystem backend rooted at `root`.
func NewFilesystemBackend(root string) (*FilesystemBackend, error) {
	if err := os.MkdirAll(root, 0o755); err != nil {
		return nil, err
	}
	return &FilesystemBackend{root: root}, nil
}

func (b *FilesystemBackend) keyPath(key string) (string, error) {
	if key == "" || strings.Contains(key, `\`) {
		return "", fmt.Errorf("invalid key: %q", key)
	}
	rel := filepath.Clean(key)
	if rel == ".." || strings.HasPrefix(rel, ".."+string(os.PathSeparator)) {
		return "", fmt.Errorf("invalid key path: %q", key)
	}
	path := filepath.Join(b.root, rel)
	absRoot, err := filepath.Abs(b.root)
	if err != nil {
		return "", err
	}
	absPath, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	if absPath != absRoot && !strings.HasPrefix(absPath, absRoot+string(os.PathSeparator)) {
		return "", fmt.Errorf("key escapes backend root: %q", key)
	}
	return absPath, nil
}

// Get returns chunk bytes for `key`, or ok=false when missing.
func (b *FilesystemBackend) Get(key string) ([]byte, bool, error) {
	path, err := b.keyPath(key)
	if err != nil {
		return nil, false, err
	}
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return nil, false, nil
	}
	if err != nil {
		return nil, false, err
	}
	return data, true, nil
}

// Put writes bytes atomically for `key`.
func (b *FilesystemBackend) Put(key string, data []byte) error {
	path, err := b.keyPath(key)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// Exists reports whether `key` is present.
func (b *FilesystemBackend) Exists(key string) (bool, error) {
	path, err := b.keyPath(key)
	if err != nil {
		return false, err
	}
	_, err = os.Stat(path)
	if os.IsNotExist(err) {
		return false, nil
	}
	return err == nil, err
}

// Delete removes `key` when present.
func (b *FilesystemBackend) Delete(key string) error {
	path, err := b.keyPath(key)
	if err != nil {
		return err
	}
	err = os.Remove(path)
	if os.IsNotExist(err) {
		return nil
	}
	return err
}
