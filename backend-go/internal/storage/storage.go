// Package storage provides a simple file storage abstraction used by the
// profile-photo upload (and potentially other media uploads).
//
// Two backends are supported:
//   - "local"  — files written to a directory on disk (default, for dev).
//   - "s3"     — files written to an S3-compatible bucket.
//
// The Backend interface is intentionally narrow.  Photo resizing is handled
// by the caller before calling Put; this layer only deals with raw bytes.
package storage

import (
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/logos-design/logos/backend-go/internal/config"
)

// Backend is the storage interface all implementations must satisfy.
type Backend interface {
	// Put writes content into <bucket>/<id>.  contentType is stored in
	// metadata but is not enforced by the local backend.
	Put(ctx context.Context, bucket, id string, content io.Reader, size int64, contentType string) error

	// Get returns a reader for the stored object.  The caller must close it.
	Get(ctx context.Context, bucket, id string) (io.ReadCloser, error)

	// Delete removes the stored object.  Implementations should treat
	// "not found" as a no-op and return nil.
	Delete(ctx context.Context, bucket, id string) error
}

// New returns the Backend configured by cfg.
func New(cfg config.Config) (Backend, error) {
	switch cfg.StorageBackend {
	case "local", "":
		dir := cfg.StorageLocalDir
		if dir == "" {
			dir = "/tmp/logos-storage"
		}
		return &localBackend{root: dir}, nil
	case "s3":
		return nil, fmt.Errorf("storage: S3 backend not yet implemented")
	default:
		return nil, fmt.Errorf("storage: unknown backend %q", cfg.StorageBackend)
	}
}

// ─── Local FS backend ────────────────────────────────────────────────────────

type localBackend struct {
	root string // e.g. /tmp/logos-storage
}

func (b *localBackend) path(bucket, id string) string {
	return filepath.Join(b.root, bucket, id)
}

func (b *localBackend) Put(_ context.Context, bucket, id string, content io.Reader, _ int64, _ string) error {
	p := b.path(bucket, id)
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		return fmt.Errorf("storage/local: mkdir %s: %w", filepath.Dir(p), err)
	}
	f, err := os.Create(p)
	if err != nil {
		return fmt.Errorf("storage/local: create %s: %w", p, err)
	}
	defer f.Close()
	if _, err := io.Copy(f, content); err != nil {
		return fmt.Errorf("storage/local: write %s: %w", p, err)
	}
	return nil
}

func (b *localBackend) Get(_ context.Context, bucket, id string) (io.ReadCloser, error) {
	f, err := os.Open(b.path(bucket, id))
	if err != nil {
		return nil, fmt.Errorf("storage/local: open %s/%s: %w", bucket, id, err)
	}
	return f, nil
}

func (b *localBackend) Delete(_ context.Context, bucket, id string) error {
	err := os.Remove(b.path(bucket, id))
	if err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("storage/local: delete %s/%s: %w", bucket, id, err)
	}
	return nil
}
