---
title: Assets Storage
desc: "Logos storage subsystem: local FS, S3-compatible, and database backends."
---

# Assets Storage

Logos stores user-uploaded assets (images, fonts, thumbnails, file exports) through a
pluggable storage backend. The backend is configured via the `STORAGE_BACKEND` environment
variable.

---

## Storage Backends

| Backend | `STORAGE_BACKEND` value | Description |
|---|---|---|
| **Local filesystem** | `local` | Writes files under `STORAGE_LOCAL_DIR` (default: `./data`) |
| **S3-compatible** | `s3` | AWS S3, MinIO, Cloudflare R2, Backblaze B2, etc. |

### Local filesystem

```bash
export STORAGE_BACKEND=local
export STORAGE_LOCAL_DIR=/opt/logos/data
```

Files are stored at `<STORAGE_LOCAL_DIR>/<bucket>/<object-id>`.

> In production, mount `STORAGE_LOCAL_DIR` on a persistent volume and back it up regularly.

### S3-compatible

```bash
export STORAGE_BACKEND=s3
export S3_BUCKET=logos-assets
export S3_REGION=eu-west-1
export S3_ENDPOINT=https://s3.amazonaws.com      # omit for AWS; set for MinIO/R2
export AWS_ACCESS_KEY_ID=<key>
export AWS_SECRET_ACCESS_KEY=<secret>
```

For MinIO in development:

```bash
export S3_ENDPOINT=http://localhost:9000
```

---

## Storage Object Metadata

Every stored object has a row in the `storage_object` PostgreSQL table:

| Column | Type | Description |
|---|---|---|
| `id` | UUID | Object identifier — used to reference the object |
| `backend` | text | `local` or `s3` — where the bytes live |
| `created_at` | timestamptz | Creation time |
| `deleted_at` | timestamptz | Soft-delete marker (GC picks up these) |
| `expired_at` | timestamptz | Auto-expiry for temporary objects |
| `touched_at` | timestamptz | GC reference counting marker |
| `metadata` | jsonb | Content-type, bucket, size, custom attributes |

---

## Asset Types and Their Tables

### File media objects

User-uploaded images, SVGs, and video used in file designs. Stored in `file_media_object`:

| Column | Description |
|---|---|
| `id` | UUID |
| `file_id` | Owning file |
| `storage_id` | Reference to `storage_object.id` for the full-resolution asset |
| `thumb_id` | Reference to `storage_object.id` for the thumbnail (nullable) |
| `is_local` | `true` = local to file, `false` = shared library asset |
| `name` | User-visible asset name |
| `media_type` | MIME type |
| `width`, `height` | Dimensions in pixels |

### Fonts

Custom font variants uploaded per team. Stored in `team_font_variant`:

| Column | Description |
|---|---|
| `id` | UUID |
| `team_id` | Owning team |
| `font_id` | Logical font group ID |
| `font_family` | Family name |
| `font_weight` | Numeric weight (400, 700…) |
| `font_style` | `normal` or `italic` |
| `storage_id` | Storage object for the font file |

### Thumbnails

File thumbnails used in the dashboard file cards. Stored in `file_thumbnail`:

| Column | Description |
|---|---|
| `file_id` | Owning file |
| `revn` | File revision this thumbnail was generated for |
| `media_id` | Storage object for the thumbnail image |
| `deleted_at` | Soft-delete |

---

## Go Storage Interface

The storage backend is abstracted behind the `Backend` interface in
`backend-go/internal/storage/storage.go`:

```go
type Backend interface {
    Put(ctx context.Context, bucket, key string, r io.Reader, opts PutOptions) error
    Get(ctx context.Context, bucket, key string) (io.ReadCloser, error)
    Delete(ctx context.Context, bucket, key string) error
}
```

Handlers receive a `*storage.Storage` (which wraps the active backend) via dependency injection.
To serve an asset to the client, handlers call `Get` and stream the result directly to the
HTTP response:

```go
rc, err := deps.Storage.Get(ctx, "media", mediaID)
if err != nil {
    writeError(w, http.StatusNotFound, "asset not found")
    return
}
defer rc.Close()
w.Header().Set("Content-Type", contentType)
io.Copy(w, rc)
```

---

## Garbage Collection

A background goroutine periodically cleans up:

1. **Expired objects** (`expired_at < now`) — temporary upload slots
2. **Deleted objects** (`deleted_at < now - grace_period`) — soft-deleted assets
3. **Unreferenced objects** — storage objects not pointed to by any `file_media_object`, `team_font_variant`, or `file_thumbnail` row

The GC touches both the `storage_object` table (deletes the row) and the underlying storage backend (deletes the bytes).
