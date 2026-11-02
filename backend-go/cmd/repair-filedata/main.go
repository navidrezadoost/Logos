// One-off dev tool: repair legacy Go-seeded file.data root frames and replay
// stored file_change rows into file.data.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/logos-design/logos/backend-go/internal/changes"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/filedata"
)

func main() {
	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		dbURL = "postgres://logos:logos@localhost:5432/logos"
	}
	ctx := context.Background()
	pgxPool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		log.Fatal(err)
	}
	defer pgxPool.Close()
	pool := &db.Pool{Pool: pgxPool}

	rows, err := pool.Query(ctx, `
		SELECT id::text, data, revn
		  FROM file
		 WHERE deleted_at IS NULL
		   AND data IS NOT NULL`)
	if err != nil {
		log.Fatal(err)
	}
	defer rows.Close()

	repaired := 0
	for rows.Next() {
		var id string
		var raw []byte
		var revn int64
		if err := rows.Scan(&id, &raw, &revn); err != nil {
			log.Fatal(err)
		}
		if len(raw) == 0 || raw[0] != '{' {
			continue
		}
		var data map[string]any
		if err := json.Unmarshal(raw, &data); err != nil {
			continue
		}
		changed := filedata.NormalizeFileData(data)
		if changes.NeedsReplay(data, revn) {
			data, err = changes.ReplayStoredChanges(ctx, pool, id, data)
			if err != nil {
				log.Fatal(err)
			}
			changed = true
		}
		if !changed {
			continue
		}
		encoded, err := filedata.EncodeJSON(data)
		if err != nil {
			log.Fatal(err)
		}
		if _, err := pool.Exec(ctx, `UPDATE file SET data = $1 WHERE id = $2`, encoded, id); err != nil {
			log.Fatal(err)
		}
		page := filedata.FirstPage(data)
		count := 0
		if page != nil {
			if objects, ok := page["objects"].(map[string]any); ok {
				count = len(objects)
			}
		}
		fmt.Printf("repaired file=%s objects=%d revn=%d\n", id, count, revn)
		repaired++
	}
	fmt.Printf("done, repaired %d file(s)\n", repaired)
}
