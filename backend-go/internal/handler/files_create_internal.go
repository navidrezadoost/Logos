package handler

import (
	"context"
	"fmt"

	"github.com/jackc/pgx/v5/pgconn"

	"github.com/logos-design/logos/backend-go/internal/filedata"
)

// CreatedFile is the create-file RPC response. The frontend reads :pages to
// navigate straight into the workspace after creating a file.
type CreatedFile struct {
	File
	Pages []string `json:"pages"`
}

type fileExec interface {
	Exec(ctx context.Context, sql string, arguments ...any) (pgconn.CommandTag, error)
}

// insertFileInTx creates a file row with empty page data and owner permissions.
func insertFileInTx(
	ctx context.Context,
	tx fileExec,
	profileID, projectID, name, fileID string,
	isShared bool,
	features []string,
) (CreatedFile, error) {
	if fileID == "" {
		fileID = newUUID()
	}
	pageID := filedata.NewPageID()
	if len(features) == 0 {
		features = filedata.DefaultFeatures
	}
	fileData := filedata.BuildEmptyData(fileID, pageID)
	dataJSON, err := filedata.EncodeJSON(fileData)
	if err != nil {
		return CreatedFile{}, fmt.Errorf("encode file data: %w", err)
	}

	if _, err := tx.Exec(ctx, `
		INSERT INTO file (id, project_id, name, is_shared, features, data, version)
		VALUES ($1, $2, $3, $4, $5, $6, $7)`,
		fileID, projectID, name, isShared, features, dataJSON, filedata.FileVersion); err != nil {
		return CreatedFile{}, fmt.Errorf("insert file: %w", err)
	}

	if _, err := tx.Exec(ctx, `
		INSERT INTO file_profile_rel (file_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, true, true, true)`,
		fileID, profileID); err != nil {
		return CreatedFile{}, fmt.Errorf("insert file role: %w", err)
	}

	if _, err := tx.Exec(ctx,
		`UPDATE project SET modified_at = now() WHERE id = $1`, projectID); err != nil {
		return CreatedFile{}, fmt.Errorf("touch project: %w", err)
	}

	return CreatedFile{
		File: File{
			ID:        fileID,
			ProjectID: projectID,
			Name:      name,
			IsShared:  isShared,
		},
		Pages: []string{pageID},
	}, nil
}
