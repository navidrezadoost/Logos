---
title: Authentication
desc: "Logos Authentication Guide: password hashing, JWE sessions, API tokens, LDAP, and magic links."
---

# Authentication

The Logos Go backend supports several authentication methods. The relevant source code
lives entirely in `backend-go/internal/auth/` and `backend-go/internal/handler/auth.go`.

---

## Authentication Methods

| Method | Handler command | Enabled by |
|---|---|---|
| **Email + password** | `login-with-password` | Always enabled |
| **Magic link** | `login-with-token` | `send-email-verification` initiates; token mailed to user |
| **LDAP** | `login-with-ldap` | `LOGOS_LDAP_HOST` configured |
| **Registration** | `register-profile` | Always enabled (or disable via `LOGOS_ENABLE_REGISTRATION=false`) |

---

## Password Hashing

Passwords are hashed with **Argon2id** using the PHC string format:

```
$argon2id$v=19$m=32768,t=3,p=2$<salt>$<hash>
```

Parameters:
- Memory cost: 32768 KiB (32 MB)
- Time cost: 3 iterations
- Parallelism: 2 lanes
- Hash length: 32 bytes
- Salt length: 16 bytes

These parameters match the original Clojure backend (`buddy-hashers`), so existing
password hashes stored in the database continue to work without any re-hashing.

The Go implementation is in `backend-go/internal/auth/password.go`:

```go
// Hash a new password
hash, err := auth.DerivePassword("user-password")

// Verify a stored hash
ok, err := auth.VerifyPassword("user-password", storedHash)
```

---

## Sessions

After successful authentication, the backend creates a session record in the
`http_session_v2` table and issues a **JWE token** (JSON Web Encryption):

| Property | Value |
|---|---|
| Algorithm | A256KW (AES-256 Key Wrap) |
| Encryption | A256GCM (AES-256-GCM) |
| Key derivation | HKDF-Blake2b-512 from `LOGOS_SECRET_KEY` |
| Cookie name | `logos-auth` (configurable via `COOKIE_NAME`) |
| Cookie flags | `HttpOnly`, `SameSite=Lax`, `Max-Age=7d` |

**Token payload:**

```json
{
  "iss": "authentication",
  "aud": "logos",
  "sid": "<session-uuid>",
  "uid": "<profile-uuid>",
  "iat": 1716800000
}
```

The `JWEMiddleware` in `internal/server/server.go` verifies the cookie on every request
and injects `profileID` into the request context via `auth.WithProfileID`.

### Token compatibility

Tokens issued by the original Clojure backend with `"aud": "penpot"` decrypt
correctly — the Go verifier reads but does not assert the audience value.
New tokens use `"aud": "logos"`.

---

## API Tokens

Long-lived tokens for programmatic access (CI, integrations, plugins):

| Property | Value |
|---|---|
| Token type | `iss: "token"` in JWE payload |
| Expiry | None (revocable via API) |
| Header | `Authorization: Token <token>` |

Create a token:

```http
POST /api/rpc/command/create-access-token
{"name": "my-ci-token"}
→ {"token": "<jwe>", "id": "<uuid>", ...}
```

List and revoke:

```http
GET  /api/rpc/command/get-access-tokens
POST /api/rpc/command/delete-access-token  {"id": "<uuid>"}
```

---

## Registration Flow

1. Client calls `prepare-register-profile` (validates email uniqueness, generates a register token)
2. Client calls `register-profile` with the register token + profile data
3. Backend creates the profile row with `is_active = false` and `source = 'logos'`
4. Backend sends a verification email containing a link with a `verify-token` parameter
5. User clicks the link → frontend calls `verify-token` → backend activates the profile and opens a session

If email is already taken, `prepare-register-profile` returns a `validation` error.

---

## Magic Link Flow

1. User enters their email on the "send magic link" form
2. Client calls `send-email-verification` (or `request-email-change` for address changes)
3. Backend generates a short-lived JWE token (`iss: "verify"`) and emails the link
4. User clicks the link → frontend calls `login-with-token` with the token
5. Backend verifies and creates a session

---

## LDAP

LDAP is enabled when `LOGOS_LDAP_HOST` is set. Configuration:

| Variable | Description |
|---|---|
| `LOGOS_LDAP_HOST` | LDAP server hostname |
| `LOGOS_LDAP_PORT` | Port (default: 389) |
| `LOGOS_LDAP_SSL` | `true` for LDAPS (default: `false`) |
| `LOGOS_LDAP_START_TLS` | `true` to use STARTTLS |
| `LOGOS_LDAP_BASE_DN` | Base DN for user search |
| `LOGOS_LDAP_BIND_DN` | Service account DN for directory bind |
| `LOGOS_LDAP_BIND_PASSWORD` | Service account password |
| `LOGOS_LDAP_USER_QUERY` | LDAP query to find the user (default: `(mail=%s)`) |

On successful LDAP bind, a Logos profile is created or matched by email address.
Subsequent logins update the display name from the directory.

---

## Middleware Chain

Every request to a protected endpoint passes through:

```
JWEMiddleware
  → reads logos-auth cookie
  → decrypts JWE
  → looks up session in http_session_v2
  → injects profileID into context
  → calls next handler

RequireAuth (called inside handler)
  → reads profileID from context
  → returns 401 if missing
```

Unauthenticated endpoints (login, register, verify-token) skip `RequireAuth`.

---

## Session Lifecycle

| Event | Action |
|---|---|
| Login / register | INSERT into `http_session_v2`; set cookie |
| Request | Session looked up by `sid` in JWT; `updated_at` bumped |
| Logout | DELETE from `http_session_v2`; clear cookie |
| Session expiry | Sessions expire after 7 days of inactivity; a cleanup job removes expired rows |
