# Security Policy

## Supported Versions

Logos Community Edition follows a rolling release model. Security fixes are applied
to the latest version on `main`. We do not backport fixes to older versions.

| Version | Supported |
|---|---|
| Latest (`main`) | ✓ |
| Older tags | Not supported — upgrade to latest |

---

## Reporting a Vulnerability

**Please do not report security vulnerabilities as public GitHub Issues.**

Security issues must be reported privately so we can prepare a fix before any
public disclosure.

### Preferred method — GitHub Security Advisories

Use the **Report a vulnerability** button on the
[Security Advisories](https://github.com/navidrezadoost/Logos/security/advisories/new)
page. This creates a private draft advisory visible only to maintainers.

### Alternative — Email

Send a report to **`security@logos.app`** with:

- A clear description of the vulnerability
- Affected component(s): backend (`backend-go/`), frontend (`logos-app/`), Rust engine, plugin runtime, file format
- Steps to reproduce or a proof-of-concept (even partial is helpful)
- Potential impact assessment (confidentiality, integrity, availability)
- Your GitHub username or contact information for follow-up

We use GPG key ID `TBD` for encrypted communication if needed.

---

## What to Expect

| Step | Timeline |
|---|---|
| Acknowledgment | Within 48 hours |
| Initial triage | Within 5 business days |
| Fix ETA provided | Within 10 business days |
| Fix released + public advisory | Coordinated with reporter |

We follow [responsible disclosure](https://en.wikipedia.org/wiki/Coordinated_vulnerability_disclosure):
we ask reporters to give us a reasonable window (typically 90 days) to prepare and ship a fix
before any public disclosure.

---

## Scope

### In scope

- Authentication bypass or privilege escalation (backend)
- Cross-site scripting (XSS) in the frontend or plugin sandbox
- Cross-site request forgery (CSRF)
- SQL injection or unauthorized database access
- Path traversal or unauthorized file access in storage backends
- Token forgery or session hijacking
- Insecure deserialization in `.logos` / `.penpot` file import
- Plugin sandbox escape (TypeScript sandbox or WASM runtime)
- Sensitive data exposure in API responses
- Denial of service via crafted requests or files

### Out of scope

- Issues in dependencies that are already publicly known and tracked upstream
- Vulnerabilities requiring physical access to the server
- Rate limiting or brute-force protections (informational reports welcome)
- Social engineering
- Self-XSS (attacker must trick the victim into running code themselves)

---

## Disclosure Policy

Once a fix is released we will publish a GitHub Security Advisory with:

- CVE identifier (if applicable)
- Description of the vulnerability and its impact
- Affected versions
- Fix location (commit / release)
- Credit to the reporter (unless anonymity is requested)

---

## Security Architecture Notes

These notes are for researchers auditing the codebase.

### Authentication

- Passwords stored as Argon2id PHC strings (memory=32768, time=3, parallelism=2)
- Sessions are JWE tokens (A256KW + A256GCM) in HTTP-only cookies
- API tokens are JWE tokens with `iss: "token"` and no expiry (revocable via the API)
- Token keys derived via HKDF-Blake2b-512 from `LOGOS_SECRET_KEY`

### Plugin Sandbox

- Plugins run inside a TypeScript sandbox with a restricted global scope
- The `logos` global exposes only the documented plugin API — no `window`, no `document`, no `fetch`
- Plugins cannot access the filesystem, network, or other plugins' data

### File Format

- `.logos` / `.penpot` import validates the manifest type before processing
- Media blobs are size-limited before storage
- File IDs are UUIDs — not user-controlled strings

### Database

- All queries use parameterized statements via `pgx/v5` — no string interpolation
- Row-level locking (`SELECT ... FOR UPDATE`) prevents concurrent write races in `files_update`
- Permissions are checked at the handler level before any data access
