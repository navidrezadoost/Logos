# Logos Helm Chart

This chart provides a first-party in-repo Kubernetes deployment scaffold for Logos.
It covers the core self-hosted stack described by the existing Docker compose file:

- frontend
- backend
- exporter
- mcp
- postgres
- valkey
- mailcatch

## Goals

- unblock issue `#19` with a concrete Helm-chart baseline
- support image pull secrets for private registries
- include readiness/liveness probes
- include HorizontalPodAutoscaler resources for frontend/backend
- keep values simple and close to the existing compose environment

## Usage

```bash
helm install logos ./deploy/helm/logos
```

With custom values:

```bash
helm install logos ./deploy/helm/logos -f my-values.yaml
```

## Notes

- This is a scaffold intended to live in-repo and evolve with the product.
- Persistence is currently modeled only for PostgreSQL.
- Asset storage, S3 wiring, external databases, and full air-gapped packaging can be layered on top in follow-up work.
- The default backend health path is set to `/api/healthz` and may need adjustment if runtime health routes differ.
