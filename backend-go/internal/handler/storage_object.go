package handler

import (
	"context"
	"fmt"

	"github.com/logos-design/logos/backend-go/internal/db"
)

// insertStorageObject registers a row in storage_object before any table references
// media_id/thumbnail_id. Matches the Clojure backend's storage-object lifecycle.
func insertStorageObject(ctx context.Context, pool *db.Pool, id, bucket, contentType string, size int64) error {
	metaJSON := fmt.Sprintf(`{"bucket":%q,"content-type":%q}`, bucket, contentType)
	_, err := pool.Exec(ctx,
		`INSERT INTO storage_object (id, backend, size, metadata) VALUES ($1, $2, $3, $4)`,
		id, "local", size, metaJSON)
	return err
}
