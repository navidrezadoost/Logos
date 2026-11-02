---
title: 1. Self-hosting Guide
desc: Self-host Logos Community Edition — Docker, Kubernetes, or build from source.
---

# Self-hosting Logos Community Edition

Logos Community Edition is fully open-source and designed for self-hosting.
You get every feature at no cost — GPU-accelerated canvas, real-time collaboration,
AI assistant, design tokens, Dev Mode, plugins, and `.logos` file import/export.

---

## What You Are Installing

| Component | Technology | Size |
|---|---|---|
| **Backend** | Single Go binary (static, no JVM) | ~20 MB |
| **Frontend** | Static React SPA (served by Nginx) | ~8 MB gzipped |
| **Database** | PostgreSQL 14+ | — |
| **Cache / Pub/Sub** | Redis or Valkey 7+ | — |
| **Storage** | Local FS or S3-compatible (MinIO, etc.) | — |

The total deployment is **two application processes** (Go binary + Nginx) plus the
PostgreSQL and Redis dependencies you already run.

---

## Deployment Options

### Option 1 — Docker Compose (coming soon)

> Official Docker images for the community edition are being prepared and will be published
> under the `logos/` Docker Hub namespace at the first stable release.
>
> Once available:
> ```bash
> wget https://raw.githubusercontent.com/navidrezadoost/Logos/main/docker-compose.yaml
> docker compose -p logos up -d
> # Application at http://localhost:9001
> ```
>
> Subscribe to [releases](https://github.com/navidrezadoost/Logos/releases) or the
> community forum to be notified when images are available.

### Option 2 — Build from Source

Building directly from source is the supported path until official images are released.
See the [Getting Started](../../getting-started/) guide for full instructions.

**Summary:**

```bash
git clone https://github.com/navidrezadoost/Logos.git
cd Logos

# Backend
export DATABASE_URL="postgres://logos:logos@localhost:5432/logos"
export REDIS_URL="redis://localhost:6379"
export LOGOS_SECRET_KEY="<your-secret-key>"
cd backend-go && go run ./cmd/server -migrate && go run ./cmd/server &

# Frontend (development server, or use `npm run build` for production)
cd ../logos-app && npm ci && npm run build
# Serve logos-app/dist/ with any static server or Nginx
```

### Option 3 — Kubernetes / Helm (coming soon)

A Helm chart will be published alongside the Docker images at the first stable release.

### Option 4 — Community options

Community-maintained deployment guides (Elestio, TrueNAS, bare-metal scripts) will be
linked here as they become available.

---

## Required Environment Variables

| Variable | Description |
|---|---|
| `DATABASE_URL` | PostgreSQL DSN, e.g. `postgres://logos:logos@localhost:5432/logos` |
| `REDIS_URL` | Redis/Valkey DSN, e.g. `redis://localhost:6379` |
| `LOGOS_SECRET_KEY` | Master secret for token derivation (32+ bytes, keep private) |

See [Configuration](../configuration/) for the full list of optional settings.

---

## Production Checklist

- [ ] `LOGOS_SECRET_KEY` is at least 32 bytes and kept in a secret manager
- [ ] PostgreSQL runs with daily backups
- [ ] TLS is terminated by a reverse proxy (Nginx, Caddy, Traefik)
- [ ] `STORAGE_BACKEND=s3` with a managed bucket (or local FS with a backup plan)
- [ ] SMTP configured for transactional email (registration, recovery, invitations)
- [ ] Redis/Valkey persistence enabled if collaboration sessions must survive restarts

---

## Upgrading

1. Read the `CHANGELOG.md` for the new version
2. Back up PostgreSQL: `pg_dump logos > backup-$(date +%Y%m%d).sql`
3. Stop the running backend
4. Pull the new code (or image when available): `git pull && git checkout vX.Y.Z`
5. Apply new migrations: `go run ./cmd/server -migrate`
6. Start the new backend
7. Rebuild the frontend: `cd logos-app && npm ci && npm run build`

Logos migrations are always forward-only and backward-safe — you can run a new migration
against a database that was previously used by an older version.

---

## Reverse Proxy Configuration

### Nginx example

```nginx
server {
    listen 443 ssl http2;
    server_name design.example.com;

    ssl_certificate     /etc/letsencrypt/live/design.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/design.example.com/privkey.pem;

    # Frontend static assets
    root /opt/logos/frontend;
    index index.html;
    try_files $uri $uri/ /index.html;

    # API + WebSocket proxied to the Go backend
    location /api/ {
        proxy_pass         http://127.0.0.1:6060;
        proxy_http_version 1.1;
        proxy_set_header   Upgrade $http_upgrade;
        proxy_set_header   Connection "upgrade";
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        proxy_read_timeout 3600s;
    }
}
```

### Caddy example

```caddyfile
design.example.com {
    handle /api/* {
        reverse_proxy localhost:6060
    }
    handle {
        root * /opt/logos/frontend
        try_files {path} /index.html
        file_server
    }
}
```

---

## Troubleshooting

See [Troubleshooting](../troubleshooting/) for a guide to reading logs and diagnosing
common issues.
