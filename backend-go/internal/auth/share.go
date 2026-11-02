// Package auth — share-link validation helpers.
//
// Share links use a plain UUID (the share_link.id) as a "token".
// The viewer endpoint validates this token by looking it up in share_link
// and checking it belongs to the requested file.
package auth

import (
	"context"
	"net/http"

	pgx "github.com/jackc/pgx/v5"

	"github.com/logos-design/logos/backend-go/internal/db"
)

// ctxKeyShareID is the context key used to propagate the resolved share-link ID.
type ctxKeyShareID struct{}

// WithShareID stores a resolved share-link ID in the context.
func WithShareID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, ctxKeyShareID{}, id)
}

// ShareID retrieves the share-link ID from the context, or "" if not present.
func ShareID(ctx context.Context) string {
	v, _ := ctx.Value(ctxKeyShareID{}).(string)
	return v
}

// ShareLink holds metadata resolved from a share_link row.
type ShareLink struct {
	ID         string
	FileID     string
	OwnerID    string
	WhoComment string
	WhoInspect string
}

// ValidateShareLink looks up a share_link row by id and checks that it belongs
// to the given fileID. Returns (link, true) on success, (nil, false) if the
// link does not exist or does not match the file.
func ValidateShareLink(ctx context.Context, pool *db.Pool, shareID, fileID string) (*ShareLink, bool) {
	if shareID == "" {
		return nil, false
	}

	var sl ShareLink
	err := pool.QueryRow(ctx,
		`SELECT id, file_id, COALESCE(owner_id::text,''), who_comment, who_inspect
		   FROM share_link WHERE id = $1`, shareID).
		Scan(&sl.ID, &sl.FileID, &sl.OwnerID, &sl.WhoComment, &sl.WhoInspect)
	if err != nil {
		if err == pgx.ErrNoRows {
			return nil, false
		}
		return nil, false
	}

	if fileID != "" && sl.FileID != fileID {
		return nil, false
	}
	return &sl, true
}

// ShareLinkMiddleware is an optional middleware that reads ?share-id from the
// query string, validates it against ?file-id, and stores the result in the
// context. It does NOT reject the request on failure — the handler decides
// whether authenticated access or share-link access is required.
func ShareLinkMiddleware(pool *db.Pool) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			shareID := r.URL.Query().Get("share-id")
			fileID := r.URL.Query().Get("file-id")
			if shareID != "" {
				if _, ok := ValidateShareLink(r.Context(), pool, shareID, fileID); ok {
					r = r.WithContext(WithShareID(r.Context(), shareID))
				}
			}
			next.ServeHTTP(w, r)
		})
	}
}
