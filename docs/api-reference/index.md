---
title: API Reference
desc: "Logos API Reference: backend RPC commands, WebSocket events, and storage API."
eleventyNavigation:
  key: API Reference
  order: 3
---

# API Reference

Logos exposes a single HTTP API served by the Go backend at port 6060.
All RPC commands are `POST /api/rpc/command/<name>` with `Content-Type: application/json`.

---

## Authentication

### Session tokens

The backend sets an HTTP-only cookie (`logos-auth`) containing a JWE session token.
All protected endpoints read this cookie automatically — no `Authorization` header needed
for browser clients.

### API tokens

Long-lived tokens can be created via `create-access-token` and passed as:

```
Authorization: Token <token>
```

---

## RPC Commands

### Profile

| Command | Auth | Description |
|---|---|---|
| `get-profile` | required | Returns the current user's profile |
| `update-profile` | required | Update name, email, locale |
| `update-profile-props` | required | Merge arbitrary JSONB props |
| `update-profile-photo` | required | Upload a new profile photo (multipart) |
| `delete-profile` | required | Delete the account |

### Authentication

| Command | Auth | Description |
|---|---|---|
| `login-with-password` | none | Login with email + password; sets session cookie |
| `login-with-ldap` | none | Login via LDAP |
| `login-with-token` | none | Login via magic-link token |
| `logout` | required | Delete the current session |
| `register-profile` | none | Create a new account |
| `prepare-register-profile` | none | Validate registration data (pre-register check) |
| `send-email-verification` | required | Re-send the verification email |
| `request-email-change` | required | Request an email change (sends verification) |
| `send-recovery-email` | none | Send a password recovery link |
| `recover-profile` | none | Reset password using a recovery token |

### Access Tokens

| Command | Auth | Description |
|---|---|---|
| `get-access-tokens` | required | List all API tokens for the current user |
| `create-access-token` | required | Create a new API token |
| `delete-access-token` | required | Delete an API token by ID |

### Token Verification

| Command | Auth | Description |
|---|---|---|
| `verify-token` | none | Verify an email/magic-link token and return its data |

### Teams

| Command | Auth | Description |
|---|---|---|
| `get-teams` | required | List all teams the user belongs to |
| `get-team` | required | Get a team by ID |
| `create-team` | required | Create a new team |
| `update-team` | required | Update team name / metadata |
| `delete-team` | required | Delete a team (owner only) |
| `leave-team` | required | Leave a team |
| `get-team-members` | required | List team members |
| `update-team-member-role` | required | Change a member's role (admin only) |
| `delete-team-member` | required | Remove a member from the team |
| `get-team-invitations` | required | List pending invitations |
| `create-team-invitation` | required | Invite a user by email |
| `resend-team-invitation` | required | Resend an invitation email |
| `delete-team-invitation` | required | Cancel an invitation |
| `accept-team-invitation` | none | Accept an invitation via token |

### Projects

| Command | Auth | Description |
|---|---|---|
| `get-projects` | required | List projects in a team |
| `get-project` | required | Get a project by ID |
| `create-project` | required | Create a new project |
| `update-project` | required | Update project name |
| `delete-project` | required | Delete a project |
| `toggle-project-is-pinned` | required | Pin/unpin a project |
| `duplicate-project` | required | Duplicate a project and all its files |
| `move-project` | required | Move a project to another team |

### Files

| Command | Auth | Description |
|---|---|---|
| `get-file` | required | Get file metadata + first page |
| `get-project-files` | required | List files in a project |
| `create-file` | required | Create a new file |
| `update-file` | required | Update file name / metadata |
| `delete-file` | required | Delete a file |
| `get-file-viewers` | required | List users currently viewing a file |
| `get-shared-files` | required | List shared library files in a team |
| `link-file-to-library` | required | Link a file to a shared library |
| `unlink-file-from-library` | required | Remove a library link |
| `move-files` | required | Move files to another project |

### File Collaboration

| Command | Auth | Description |
|---|---|---|
| `update-file` | required | Submit change-sets (OT rebase, Redis broadcast) |
| `get-file-snapshot` | required | Get a labeled snapshot of a file version |
| `create-file-snapshot` | required | Create a labeled snapshot |
| `update-file-snapshot` | required | Rename a snapshot |
| `delete-file-snapshot` | required | Delete a snapshot |
| `restore-file-snapshot` | required | Restore a file to a snapshot |

### Thumbnails

| Command | Auth | Description |
|---|---|---|
| `get-file-thumbnail` | required | Get the latest thumbnail for a file |
| `upsert-file-thumbnail` | required | Create or replace a file thumbnail |
| `delete-file-thumbnail` | required | Delete a thumbnail by revn |

### Binary File Format

| Command | Auth | Description |
|---|---|---|
| `export-binfile` | required | Export file as `.logos` ZIP (streaming download) |
| `import-binfile` | required | Import a `.logos` or `.penpot` file (multipart upload) |

### Comments

| Command | Auth | Description |
|---|---|---|
| `get-file-comments` | required | List all comment threads in a file |
| `create-comment-thread` | required | Start a new comment thread |
| `update-comment-thread` | required | Mark thread as resolved / unresolved |
| `delete-comment-thread` | required | Delete a thread |
| `create-comment` | required | Add a reply to a thread |
| `update-comment` | required | Edit a comment |
| `delete-comment` | required | Delete a comment |

### Media

| Command | Auth | Description |
|---|---|---|
| `get-team-shared-files-library-media` | required | List media in a team library |
| `upload-file-media-object` | required | Upload media (multipart) |
| `create-file-media-object-from-url` | required | Import media from a URL |
| `delete-file-media-object` | required | Delete a media object |

### Fonts

| Command | Auth | Description |
|---|---|---|
| `get-fonts` | required | List custom fonts for a team |
| `upload-team-font-variant` | required | Upload a font variant (multipart) |
| `update-team-font-variant` | required | Rename a font variant |
| `delete-team-font-variant` | required | Delete a font variant |

### Search

| Command | Auth | Description |
|---|---|---|
| `search-files` | required | Full-text file search within a team |

### Webhooks

| Command | Auth | Description |
|---|---|---|
| `get-webhooks` | required | List webhooks for a team |
| `create-webhook` | required | Create a new webhook |
| `update-webhook` | required | Update webhook URL / events |
| `delete-webhook` | required | Delete a webhook |

### Audit

| Command | Auth | Description |
|---|---|---|
| `push-audit-events` | required | Batch insert audit log events (requires `LOGOS_ENABLE_AUDIT_LOG=true`) |

### Demo

| Command | Auth | Description |
|---|---|---|
| `create-demo-profile` | none | Provision a demo account (requires `LOGOS_ENABLE_DEMO_USERS=true`) |

### Feedback

| Command | Auth | Description |
|---|---|---|
| `send-user-feedback` | required | Submit feedback (requires `LOGOS_ENABLE_USER_FEEDBACK=true`) |

### Management

| Command | Auth | Description |
|---|---|---|
| `get-builtin-templates` | none | Returns `[]` — templates are frontend-driven |

---

## WebSocket Events

The backend broadcasts file change events via Redis Pub/Sub. Clients subscribe over
WebSocket at `ws://<host>/ws/file/<file-id>`:

| Event | Payload | Description |
|---|---|---|
| `file-change` | `{revn, changes[]}` | A new change-set was committed to the file |
| `who-is-here` | `{profileId, sessionId}` | A user joined the file view |
| `who-left-here` | `{profileId, sessionId}` | A user left the file view |
| `presence` | `{profileId, pointer}` | Cursor position update |

---

## Health

```http
GET /api/_health
→ 200 {"status": "ok", "version": "dev"}
```

---

## Error Response Format

All errors return a JSON body:

```json
{
  "type": "error",
  "code": "not-found",
  "hint": "file not found"
}
```

Common error codes:

| Code | HTTP Status | Meaning |
|---|---|---|
| `not-authenticated` | 401 | Missing or invalid session |
| `not-authorized` | 403 | Authenticated but insufficient permissions |
| `not-found` | 404 | Resource does not exist |
| `validation` | 422 | Invalid request body |
| `internal-error` | 500 | Unexpected server error |
