# Phase 5 Issue Status Summary

Close-ready summary for the currently open Phase 5 GitHub issues.

## Already implemented in `main`

- `#24` Team libraries — CRDT-based shared components
  - Evidence: `common/src/app/common/logic/libraries.cljc`, `frontend/src/app/main/ui/workspace/libraries.cljs`, `docs/ADR-001-CRDT-Selection.md`
- `#20` SAML/SSO authentication — OAuth2 + OIDC
  - Evidence: `logos-agent-sso/src/saml.rs`, `logos-agent-sso/src/oidc.rs`, `logos-identity/src/oauth.rs`
- `#11` Verified publisher program — Ed25519 attestation
  - Evidence: `logos-marketplace-auth/src/attestation.rs`, `logos-marketplace-auth/src/identity.rs`
- `#10` Enhanced SVG/PDF native engine
  - Evidence: `exporter/src/app/renderer/svg.cljs`, `exporter/src/app/renderer/pdf.cljs`
- `#8` Adobe XD import
  - Evidence: `logos-import-xd/src/lib.rs`, `logos-import-xd/src/convert.rs`
- `#5` On-device asset generation — Stable Diffusion
  - Evidence: `logos-ai/src/inference/asset_gen.rs`
- `#3` Intelligent layout generation — Transformer model
  - Evidence: `logos-ai/src/inference/layout_gen.rs`

## Partially implemented / scaffolded

- `#22` Audit logging — immutable event stream
  - Existing pieces: `backend/src/app/loggers/audit.clj`, `logos-identity/src/audit.rs`, `logos-replay/src/oplog.rs`
- `#19` On-premise deployment — Docker + Kubernetes
  - Existing pieces: `docker/images/docker-compose.yaml`, docs, plus new in-repo Helm chart scaffold in `deploy/helm/logos`
- `#14` Community template gallery — 1000 templates
  - Existing pieces: `logos-marketplace-db/src/templates.rs`, `logos-marketplace-api/src/handlers.rs`
- `#12` Plugin analytics — privacy-preserving telemetry
  - Existing pieces: `logos-marketplace-db/src/analytics.rs`
- `#7` Sketch import/export
  - Import exists: `logos-import-sketch/src/lib.rs`; export not obvious
- `#6` Figma import/export
  - Import exists: `logos-import-figma/src/parser.rs`; export not obvious
- `#4` Style transfer engine — real-time preview
  - Engine exists: `logos-ai/src/inference/style_transfer.rs`; editor preview integration not obvious

## Not present

- `#13` Revenue sharing — Stripe Connect integration
  - No clear Stripe integration found in the current repo

## This change set

This commit advances `#19` by adding an in-repo Helm deployment scaffold under `deploy/helm/logos`.
