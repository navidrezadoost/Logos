package handler

import (
	"context"
	"encoding/json"
	"log"

	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/filedata"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/transit"
)

// FilePermissions is returned with get-file for workspace bootstrap.
type FilePermissions struct {
	Type     transit.Keyword `json:"type"`
	IsOwner  bool            `json:"is-owner"`
	IsAdmin  bool            `json:"is-admin"`
	CanEdit  bool            `json:"can-edit"`
	CanRead  bool            `json:"can-read"`
	IsLogged bool            `json:"is-logged"`
}

// FileDetail is the full file payload returned by get-file.
type FileDetail struct {
	ID                string          `json:"id"`
	ProjectID         string          `json:"project-id"`
	Name              string          `json:"name"`
	IsShared          bool            `json:"is-shared"`
	Revn              int             `json:"revn"`
	Vern              int             `json:"vern"`
	CommentThreadSeqn int             `json:"comment-thread-seqn"`
	CreatedAt         transit.Instant `json:"created-at"`
	ModifiedAt        transit.Instant `json:"modified-at"`
	Features          []string        `json:"features"`
	HasMediaTrimmed   bool            `json:"has-media-trimmed"`
	Version           int             `json:"version"`
	Data              map[string]any  `json:"data"`
	Permissions       FilePermissions `json:"permissions"`
}

func membershipPermissions(p *perms.ProjectPerms) FilePermissions {
	if p == nil {
		return FilePermissions{Type: transit.Keyword("membership"), CanRead: true, IsLogged: true}
	}
	return FilePermissions{
		Type:     transit.Keyword("membership"),
		IsOwner:  p.IsOwner,
		IsAdmin:  p.IsAdmin,
		CanEdit:  p.CanEdit,
		CanRead:  p.CanRead,
		IsLogged: true,
	}
}

func loadOrInitFileData(ctx context.Context, pool *db.Pool, fileID string, features []string) (map[string]any, error) {
	var raw []byte
	err := pool.QueryRow(ctx, `SELECT data FROM file WHERE id = $1`, fileID).Scan(&raw)
	if err != nil {
		return nil, err
	}
	if len(raw) > 0 && raw[0] == '{' {
		var data map[string]any
		if err := json.Unmarshal(raw, &data); err == nil && len(data) > 0 {
			if filedata.NormalizeFileData(data) {
				if encoded, err := filedata.EncodeJSON(data); err == nil {
					if _, err := pool.Exec(ctx, `UPDATE file SET data = $1 WHERE id = $2`, encoded, fileID); err != nil {
						log.Printf("[get-file] persist normalized data failed file=%s: %v", fileID, err)
					}
				}
			}
			logShapeHitTestFromData("get-file", fileID, data)
			return data, nil
		}
	}

	data := filedata.BuildEmptyData(fileID, "")
	encoded, err := filedata.EncodeJSON(data)
	if err != nil {
		return nil, err
	}
	if len(features) == 0 {
		features = filedata.DefaultFeatures
	}
	_, err = pool.Exec(ctx, `
		UPDATE file
		   SET data = $1,
		       version = COALESCE(version, $2),
		       features = CASE
		         WHEN features IS NULL OR cardinality(features) = 0 THEN $3
		         ELSE features
		       END
		 WHERE id = $4`, encoded, filedata.FileVersion, features, fileID)
	if err != nil {
		return data, err
	}
	return data, nil
}

func logShapeHitTestFromData(event, fileID string, data map[string]any) {
	page := filedata.FirstPage(data)
	if page == nil {
		return
	}
	objects, _ := page["objects"].(map[string]any)
	for id, raw := range objects {
		if id == filedata.RootShapeID {
			continue
		}
		shape, ok := raw.(map[string]any)
		if !ok {
			continue
		}
		filedata.LogShapeHitTest(event, fileID, id, shape)
	}
}
