# FG-C3.2.3 — Trusted Planner Boundary and Component/Instance Gap

**Status:** Isolated engineering qualification only. No production handler, persistence, or authority change.

## Go/Rust response contract

The coordinator accepts a planner process only after a `capabilities` request returns protocol 2, semantic 1, diagnostic 1, the exact operation sets, the framing limits, and response-correlation contract v1. Component/Instance capability remains explicitly unsupported by this Rust planner. Any startup failure closes the child and leaves it unready.

Every response is checked before it is returned to a caller. The Go boundary checks exact versions and request ID, known response phase, required result/error fields, contradictory fields, typed diagnostic/error shapes, and unknown protocol-owned fields. Evaluate success additionally requires a supported candidate document envelope, matching document ID, and echoed operation kind equal to the request. Document properties remain open data because the Rust Document contract preserves extensions. Invalid responses invalidate the process session.

Successful evaluation requires `phase: evaluated` and an empty diagnostics array. Rejected evaluation requires `input-validation` or `operation-rejected`, at least one structured diagnostic, and no candidate-only fields. A protocol error requires one diagnostic whose code and path match the structured error. The current diagnostic protocol has no warning severity, so successful results cannot carry warning diagnostics.

Identity results require dashed UUID text (hex case-insensitive). `deriveIds` must return one UUID per input address; semantically identical repeated addresses may repeat the same UUID, while distinct addresses must resolve to distinct UUIDs. `checkDetachIds` must return the exact candidate list in order, with no malformed UUIDs, duplicates, or collisions with existing authored IDs. These checks validate response consistency; they do not reproduce the Rust UUIDv5 derivation algorithm in Go.

Capability negotiation verifies protocol compatibility, not trustworthiness of a compromised executable. The child is a local executable configured by its launcher; executable provenance, OS sandboxing, memory/CPU limits, and production supervision are outside this spike.

### Resource and lifecycle limits

- The persistent child is single-flight. Concurrent callers queue; there is no parallel worker pool.
- Request/response frames are bounded to 1 MiB. One-shot stdout capture is also bounded; stderr capture retains at most 4 KiB and discards excess.
- Cancellation and protocol failure kill and reap the child. The next caller starts a fresh process and must repeat the capability handshake.
- No OS-level CPU, memory, file-descriptor, or wall-clock quota is installed on the child. Callers supply request contexts; deployment must add an external supervisor and resource controls before any service adoption.
- Local benchmark observations from FG-C3.2.2 remain fixture-specific. This work does not claim production latency, throughput, or memory behavior.

## TypeScript-to-Rust Component/Instance semantic gap

Evidence reviewed: `logos-app/src/document-core/componentInstanceDomainV1.ts`, Rust `logos-types/src/document.rs` and `shape.rs`, and the Rust planner capability response. Status is source-level only; no behavior parity is claimed.

| Capability | TypeScript v1 contract | Existing Rust document model | Classification |
|---|---|---|---|
| Component definitions | `components` keyed map, definition node graph, ordered roots, properties, variant values | `ShapeType::Component`, flat `ShapeRecord` graph, `component-id` / `component-file` / `component-root` / `main-instance` flags; no definition record or property map | Requires new Rust implementation and canonical ownership decision |
| Component sets and variants | `componentSets`, ordered member IDs, typed variant properties, exact member tuples and unique combinations | No set model, membership, or variant-property model | Requires new Rust implementation |
| Typed property definitions and overrides | Boolean, text, instance-swap, slot; typed values; field-addressed overrides and target validation | No component-property or override-address model | Requires new Rust implementation |
| Slot ownership and authored content | Slot-property-owned ordered content graphs; authored IDs scoped to each slot; guidance separate from hard limits | Ordinary shape parent/child graph exists, but no slot owner or authored slot-content boundary | Requires new Rust implementation; ID scope is an architecture decision |
| Local component references | Explicit local component/set source and contextual set resolution | Basic `component_id` references; document validator checks limited target shape/component references | Adaptable primitives, semantics incomplete |
| External/unresolved references | Explicit external file/component/revision or unresolved JSON locator; preserved with warning | Optional `component_file` UUID; no external revision or unresolved locator union | Requires reference-contract decision and new representation |
| Nested expansion cycle checks | Instance-contextual expansion; swap selection and root slot context; deterministic cycle diagnostic | Graph-cycle validation for document shape membership only; no component expansion graph | Requires new Rust algorithm |
| Diagnostics | Stable component code/path/severity/detail; deterministic sorting; unresolved sources are warnings | Stable validation code/path and optional object IDs; no severity/detail or component diagnostic set | Adaptable diagnostic plumbing; version/shape mapping required |
| Unknown fields | Component v1 rejects unknown fields; preserves explicit unresolved locator JSON | Document/page/shape extensions are open and preserved | Policy decision: strict component schema can coexist inside open document extension, but conversion must be explicit |
| Identity constraints | Component, set, instance, definition-node IDs share component-domain namespace; slot content IDs are scoped | Document graph has a global UUID validation namespace; no separate component/slot namespace | Requires explicit identity mapping and compatibility decision |

The TypeScript validator is currently the only executable owner of the component-domain v1 rules. Rust's document model has useful shape containers and component-link fields, but those are not evidence of equivalent component semantics. The handshake intentionally reports `componentInstanceDomain: not-supported-here`.

## Smallest safe next architecture gate

Recommend a non-production schema and conversion design milestone before a validator port. It should decide where the component-domain value lives relative to the Rust candidate Document, how its identity namespace relates to document IDs, and how external references and warnings survive conversion. Then implement a Rust typed model for the TypeScript v1 JSON without changing persisted files, with TypeScript remaining the differential oracle. Add shared fixtures for valid documents and each diagnostic family before porting expansion behavior. Go remains a process coordinator only.

This next gate must not imply production authority transfer. The remaining work is Component/Instance semantics, executable provenance and deployment supervision, browser/wasm qualification, and—separately—durable revision fencing/idempotency. Durable transaction semantics are not implemented by the framed process protocol.
