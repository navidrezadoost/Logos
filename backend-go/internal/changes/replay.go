package changes

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/rebase"
)

// ReplayStoredChanges applies all JSON-encoded file_change rows in revn order.
func ReplayStoredChanges(ctx context.Context, pool *db.Pool, fileID string, data map[string]any) (map[string]any, error) {
	rows, err := pool.Query(ctx, `
		SELECT changes
		  FROM file_change
		 WHERE file_id = $1
		   AND changes IS NOT NULL
		 ORDER BY revn ASC`, fileID)
	if err != nil {
		return data, err
	}
	defer rows.Close()

	for rows.Next() {
		var raw []byte
		if err := rows.Scan(&raw); err != nil {
			return data, err
		}
		var batch []rebase.Change
		if err := json.Unmarshal(raw, &batch); err != nil {
			continue
		}
		data, err = ProcessChanges(data, batch)
		if err != nil {
			return data, fmt.Errorf("replay file %s: %w", fileID, err)
		}
	}
	return data, rows.Err()
}
