# FG-C3.8 — Component/Instance Isolated Capability Exposure Decision

**Status:** Decision package prepared for Chief Architect; no exposure or deployment authorized.  
**Baseline:** `61dc1795553226f42593d901a6071fa4b541338a`  
**Branch:** `core/g1-b9-6-memory-evidence-foundation`  
**Scope:** Evidence and architecture recommendation only.

## Executive recommendation

**NO-GO for advertising or deploying `validateComponentInstanceV1`.** There is no identified production or non-production product consumer in the repository that needs to invoke this operation. The Rust implementation is a useful isolated conformance candidate, but implementation completion alone does not establish product value. Retain it unadvertised for test and architecture research only, or retire it if the Chief Architect decides the comparison no longer has value.

This is a recommendation, not the final capability-exposure decision. TypeScript `componentInstanceDomainV1` remains the v1 semantic reference and current owner. The Rust planner handshake remains `componentInstanceDomain.status = "not-supported-here"`. No conclusion here authorizes runtime integration, canonical ownership, user authorization, persistence, or authority transfer.

## Consolidated evidence matrix

“Verified” below means that the named local code/tests or the cited bounded corpus demonstrated the stated behavior. It does not imply hosted CI, production-document compatibility, all-input correctness, or deployment readiness.

| Area | Status | Evidence | Boundary / outstanding verification |
| --- | --- | --- | --- |
| TypeScript v1 reference and typed Rust representation (FG-C3.3) | **VERIFIED** for generated contract fixtures | Strict Rust DTO parse/serialize/reparse tests preserve supported values and array order, produce deterministic record ordering, and reject unknown or incompatible fields. TypeScript tests exercise the shared positive and negative fixtures. | Fixtures are generated contracts, not a production corpus. No canonical Document placement, saved-file migration, or byte-preserving conversion is qualified. |
| Rust semantic implementation parity (FG-C3.4, C3.4.1, later corpus) | **VERIFIED** for the measured shared JSON corpus | Latest recorded differential run: 339 cases total; 338 shared cases matched exactly, 0 divergences, 1 JavaScript-only `NaN` input unsupported by the JSON wire format. Exact comparison includes acceptance, ordered code/path/severity/detail diagnostics, semantic returned values, and TypeScript input purity. Earlier recorded runs grew from 37 to 143 shared cases with no divergences. | A bounded generated/adversarial corpus cannot prove all possible values or production Figma/Logos files. JavaScript-only states cannot cross JSON. Parser and numeric behavior beyond covered cases remains a limit. |
| Protocol framing, validation response, and adversarial behavior (FG-C3.5/3.6) | **VERIFIED LOCALLY** for the isolated Go/Rust spike | Protocol v2 request/response correlation; 1 MiB framed request/response ceiling; 768 KiB embedded document and diagnostics bounds; duplicate decoded-key rejection; version checks; exact diagnostic validation; no document/commit fields in a validation response. Tests cover malformed/oversized/truncated input, semantic rejection, stale correlation, response shape, and restart after process/session failure. | This is a local spike protocol, not an advertised capability or authenticated production endpoint. Hosted CI was not run. |
| Deterministic semantic-work and parser bounds (FG-C3.6.1) | **VERIFIED** for implemented counters and tested boundaries; **PARTIALLY VERIFIED** as an operational resource guarantee | Parser depth/value limits and a 5,000,000-unit semantic-work budget reject before the corresponding guarded work; diagnostics and response sizes are bounded. Boundary, high-cardinality precharge, overflow, and reset behavior have executable tests. | Work units are accounting, not CPU time or bytes. Rust heap allocations are not individually metered by the semantic budget. No general request memory ceiling or production latency guarantee follows. |
| Process timeout, protocol failure, and local restart path (FG-C3.6/3.7) | **VERIFIED LOCALLY** for tested child-process scenarios | Go coordinator tests cover cancellation/timeout, child termination/reaping, malformed/truncated response and lazy worker restart on a later call. FG-C3.7 exercised user-systemd resource controls and cleanup. | Production packaging, service manager ownership, cross-platform behavior, deployment observability, and whole-service external restart after unit OOM are unverified. The actual coordinator has no retry/backoff or circuit breaker. |
| Planner-specific cgroup/OOM recovery (FG-C3.7.1) | **VERIFIED LOCALLY, HOST-DEPENDENT** | Rust PID membership in a 64 MiB sibling systemd scope; scope `oom_kill` changed 0→1; systemd reported `Result=oom-kill`, `OOMKills=1`; Go test supervisor survived; interrupted validation returned no success; a later explicit call used a new Rust PID and correlated successfully. | This host lacked nested delegated cgroups. A keeper shared the scope; numeric Rust exit status was unavailable. This is convincing local fault evidence, not production isolation or a complete sandbox. |
| Test-only bounded startup retry/backoff | **VERIFIED ONLY IN QUALIFICATION HELPER** | Four validation-only attempts, bounded 25/50/100 ms exponential delays, cancellation and exhaustion tests, non-validation refusal, semantic rejection without retry. | The helper is test code, not the Go coordinator. Do not claim production retries, replay governance, or circuit breaking. |
| Authentication, authorization, and caller identity | **NOT VERIFIED / NOT IMPLEMENTED AT THIS BOUNDARY** | The protocol accepts a validation request but carries no trusted caller identity or authorization decision. No production consumer exists in this repository search. | A future caller must authenticate/authorize before invoking the planner. Planner output cannot establish caller rights. |
| Production deployment / security sandbox | **NOT VERIFIED** | Local cgroup and user-systemd qualification only. | cgroups are resource controls, not a complete sandbox. Executable provenance enforcement, seccomp policy, production filesystem/network policy, supported-host matrix, and operator runbooks remain open. |
| Canonical Document ownership, persistence, collaboration, renderer, undo/redo | **NOT VERIFIED; OUT OF SCOPE** | The isolated operation returns validation status/diagnostics and no canonical document or commit instruction. | No revision fencing, persistence transaction, user authorization, render result, undo record, or collaborative operation is provided. |
| Retained memory/performance and hosted CI | **BLOCKED / NOT MEASURED** | Prior GitHub billing restriction prevented hosted workflow execution. FG-C3.7.1 did not run retained-memory measurement. | G1-B9.6 retained memory stays **NOT MEASURED**. Local timings are not hosted CI or production performance evidence. |

Supporting records: [FG-C3.3 typed schema](FG-C3.3-COMPONENT-INSTANCE-TYPED-SCHEMA.md), [FG-C3.4 semantic parity](FG-C3.4-COMPONENT-INSTANCE-VALIDATION-PARITY.md), [FG-C3.4.1 robustness](FG-C3.4.1-DIFFERENTIAL-ROBUSTNESS.md), [FG-C3.6 protocol qualification](FG-C3.6-ISOLATED-PROTOCOL-QUALIFICATION.md), [FG-C3.6.1 resource bounds](FG-C3.6.1-SEMANTIC-RESOURCE-BOUNDING.md), [FG-C3.7 process qualification](FG-C3.7-SUPERVISED-LOCAL-PROCESS-QUALIFICATION.md), and [FG-C3.7.1 OOM recovery](FG-C3.7.1-PLANNER-OOM-RECOVERY.md).

## Capability consumer and fallback contract

### Current consumer finding

Repository search finds the operation in the Rust planner spike, Go coordinator spike, test suites, fixtures, and architecture documents. It finds no production React, Go service, persistence, renderer, import path, or authenticated user workflow invoking it. The TypeScript validator itself is currently an isolated document-core module and is not wired to production Zustand mutations or persistence. Therefore there is **no justified current consumer** and no concrete use case for which Rust is shown to be necessary where TypeScript is insufficient.

If the architecture team wants to retain an experimental consumer, the defensible current use is an offline/non-production conformance harness that submits the same bounded JSON test inputs to the TypeScript reference and Rust candidate. That consumer has engineering-evidence value, not user-facing product value, and does not justify advertising the operation to application clients.

### Contract if a future consumer is separately approved

| Contract point | Required behavior |
| --- | --- |
| Intended caller | A named, allowlisted internal tool or service with a documented non-production or production purpose. No generic browser/client access. |
| Purpose | Validate a specific Component/Instance v1 JSON snapshot and return diagnostics only. The request is read-only and must not be confused with planning or applying document operations. |
| Invocation boundary | Caller first authorizes the user/resource and selects an immutable input snapshot. The validator receives only the bounded snapshot needed for validation. The caller retains the source and revision identity. |
| Response meaning | Advisory validation result relative to the TypeScript v1 contract. It is not a canonicalization, commit, authorization, rendering, or persistence decision. Until parity and governance are explicitly accepted, Rust must not be the sole acceptance gate. |
| Semantic rejection | A correlated `ok:false` with complete diagnostics is a completed validation result. It is not a process failure. Do not retry as if the request never ran. |
| Timeout, OOM, crash, malformed/stale response | Treat as **unavailable/failed**, never as valid. Do not silently replay a mutation. Retain the caller's original bytes/state and surface the failure. A later explicit read-only validation may run against the same or a newly selected snapshot with a new request ID. |
| Mismatch with TypeScript | TypeScript remains the v1 semantic reference. Record the mismatch, fail closed for any gate depending on parity, and do not normalize away the difference. No automatic Rust preference. |
| Unsupported environment | Do not start uncontained. Return explicit unsupported/unavailable status or use the approved TypeScript path. Capability absence must not be interpreted as successful validation. |
| Fallback | TypeScript remains the v1 semantic reference. A future product fallback must explicitly integrate/invoke it under an approved ownership boundary; the current TypeScript module is not wired into production mutations or persistence. If no qualified validator is available, block the dependent operation; never convert missing evidence into pass. |

The current Rust protocol does not authenticate callers. A future adapter must enforce caller authorization before dispatch and must not trust planner-supplied identity or capability claims.

## Ownership and authority boundaries

TypeScript `componentInstanceDomainV1` is the semantic reference for v1. Rust is a candidate implementation checked against it on a finite corpus. Neither a matching result nor an advertised capability would make Rust the canonical Document owner.

| Domain | Relationship to isolated validation result |
| --- | --- |
| Canonical Document ownership | None. The spike validates an embedded JSON value and returns diagnostics; it does not own or serialize a canonical application document. |
| Revisions | The caller is responsible for choosing and retaining the exact source snapshot/revision. The current request has no authoritative document revision fence. A result cannot be applied to a newer revision without revalidation. |
| User/resource authorization | Must be decided by the caller/service before dispatch. Validation success is not permission. |
| Persistence | None. The result includes no mutation, commit token, database write, or persistence instruction. |
| Rendering | None. It does not prove that an editor or renderer can display the structure. |
| Undo/redo | None. It creates no operation record or inverse. |
| Collaboration | None. It does not resolve concurrent edits, shared revision ordering, or permission changes after validation. |

Any future use that converts validation into an acceptance or mutation decision needs a separately designed binding among authenticated caller, authorized resource, immutable revision, semantic result, and commit boundary. This is outside FG-C3.8 and is not implied by this recommendation.

## Architecture alternatives

| Alternative | Operational cost / complexity | Semantic drift and reliability | Consumer value demonstrated | Assessment |
| --- | --- | --- | --- | --- |
| **A — TypeScript only** | Lowest runtime and deployment cost; no second worker lifecycle. | One semantic reference reduces cross-runtime drift. Existing TypeScript remains the v1 reference but is not currently wired into production mutations or persistence. | No production consumer for Rust was found; no current product flow proves a need for another runtime. | **Preferred direction for any future product behavior.** Integrate the TypeScript reference under an explicit product boundary; keep Rust only if its bounded test value justifies maintenance. |
| **B — Rust candidate, isolated and unadvertised** | Maintains a second implementation and spike infrastructure; no application dispatch or production SLA. | Differential tests can detect drift, but two implementations require ongoing parity work. Local protocol/OOM evidence improves confidence within its test scope. | Engineering conformance evidence only; no user-facing consumer demonstrated. | **Recommended disposition of current work.** Preserve as non-authoritative test/research candidate; keep handshake unsupported. Reassess maintenance value periodically. |
| **C — Advertise to explicitly authorized non-production consumers** | Adds capability negotiation, caller allowlisting, deployment lifecycle, resource limits, observability, fallback and support burden. | Local parity is strong but finite; deployment and identity boundary are not qualified. | No named consumer or need is documented. | **NO-GO now.** Do not change handshake or runtime behavior. Reconsider only after a named consumer and all prerequisites below are approved. |

## Prerequisites for any future exposure

### Isolated non-production advertisement gate

All items require evidence before a separate exposure authorization:

- Named consumer, product/engineering purpose, owner, and explicit allowlist; no general capability discovery that implies availability to all callers.
- Versioned caller contract that labels results validation-only/advisory and distinguishes semantic rejection from unavailable/process failure.
- TypeScript v1 stays the reference; shared differential and adversarial tests run in the target environment with a defined mismatch policy.
- Supported deployment host/runtime and effective memory, CPU, and process limits are verified. The planner cannot silently fall back to an uncontained process.
- Lifecycle owner supervises process startup, shutdown, reaping, restart, backoff and bounded admission queue. Retries are bounded and only safe for explicitly read-only validation; semantic rejection is never retried as infrastructure failure.
- Trusted binary provenance/version, least privileges, filesystem/network policy and applicable syscall restrictions are verified. cgroups alone are insufficient.
- Correlation IDs, latency/failure/resource/OOM observations and sanitized diagnostics are observable without logging document content or secrets.
- Timeouts, OOM, malformed response, stale correlation, unsupported environment, overload, and version mismatch all fail closed and are distinguishable to callers.
- Operational runbook and rollback/removal procedure exist; test/qualification evidence is reproducible on the target non-production host.

Passing this gate would allow only the narrowly named non-production consumer. It would not grant production authority.

### Additional production integration gate

Production use additionally requires a canonical ownership decision; versioned document compatibility and migrations; authenticated authorization tied to resource/revision; stale-revision fencing between validation and commit; persistence and idempotency design; renderer and editor integration evidence; undo/redo semantics; collaborative ordering and revocation behavior; production SLOs, capacity and retained-memory measurements; security review of executable and host boundary; deployment-owner sign-off; and an explicit Chief Architect authorization. None is established by FG-C3.3 through FG-C3.7.1.

## Unresolved risks

1. **No consumer:** product utility and the cost of maintaining a second validator are not established.
2. **Finite semantic evidence:** 338 JSON cases match, but this is not exhaustive equivalence and excludes JavaScript-only values.
3. **Double implementation:** ongoing TypeScript/Rust changes can drift; current generated corpus is not evidence of production file coverage.
4. **Host-dependent supervision:** the demonstrated sibling scope and user systemd manager may not exist in another deployment. Nested delegation and whole-unit external restart are not demonstrated.
5. **Incomplete process policy:** retries/backoff are a test helper, not production coordinator behavior; no advertised queue, circuit breaker, or service-level observability exists.
6. **Security boundary:** no authenticated caller context, enforced binary identity, seccomp qualification, or complete sandbox is demonstrated.
7. **No authority binding:** revision fencing, persistence, authorization, collaboration, rendering and undo/redo are outside the validation protocol.
8. **Missing hosted/runtime evidence:** hosted CI is unavailable; retained-memory measurement is **NOT MEASURED** and production readiness is not established.

## Decision requested from the Chief Architect

**Recommended verdict: NO-GO for capability exposure (Alternative C).** Preserve TypeScript as the sole v1 semantic reference (Alternative A direction); this does not claim it is already integrated into production product flows. Optionally retain Rust as an isolated, unadvertised conformance candidate (Alternative B) only while its test value justifies maintenance. The current handshake must remain `not-supported-here`.

This recommendation does not decide canonical Document ownership and does not authorize capability advertisement, implementation integration, PR creation, merge, or deployment.

### Smallest separately authorizable next milestone

If the Chief Architect wants further exposure work, authorize **FG-C3.8.1 — Named Consumer and Boundary Contract**, a documentation-only, time-boxed decision gate. It must name one actual caller and use case; show why the TypeScript path is insufficient; identify the caller's authenticated authorization boundary, source revision, fallback, operational owner, and supported environment; and state whether the isolated validation result is advisory or blocks a specific workflow. Acceptance is either (a) a complete, approved non-production consumer contract that can be separately evaluated for implementation authorization, or (b) close the exposure proposal with NO-GO and retain/retire the Rust candidate by an explicit ownership decision. No exposure or code change is implied.

## Program status after this package

| Workstream | Status |
| --- | --- |
| FG-C3.3–FG-C3.6.1 semantic/protocol/resource evidence | Accepted/local evidence as documented; bounded scope |
| FG-C3.7.1 local Rust OOM recovery | Accepted, locally qualified; deployment behavior unverified |
| FG-C3.8 decision package | **Accepted and closed by Chief Architect** |
| Capability advertisement | **NO-GO — prohibited by binding decision** |
| Production Rust authority / canonical ownership | Not authorized / pending |
| G1-B9.6 retained memory | **NOT MEASURED** |
| G1-B10 authority transfer | **NO-GO** |

## Final Chief Architect disposition

**Decision: APPROVED — FG-C3.8 CLOSED, NO-GO for Component/Instance capability exposure.** This section records the binding disposition received after the recommendation above. It supersedes the earlier “Decision requested” and “Smallest separately authorizable next milestone” language: those sections describe the proposal before the decision and do not authorize FG-C3.8.1.

| Area | Binding disposition |
| --- | --- |
| TypeScript `componentInstanceDomainV1` | Retained as the v1 semantic reference. |
| Rust Component/Instance validator | Retained only as an isolated conformance candidate for differential testing, regression qualification, and architecture research. |
| Rust capability advertisement | **PROHIBITED.** The capability handshake remains `not-supported-here`. |
| Production React/Go/Rust integration, persistence, and authority transfer | Not authorized; Rust production authority remains **NO-GO**. |
| Canonical Document ownership | Not approved; remains a separate architecture decision. |
| FG-C3.8.1 or further capability-exposure work | Not authorized. No additional exposure, deployment-qualification, or product-integration milestone is open under FG-C3.8. |
| PR, merge, or `main` changes | Not authorized by this disposition. |
| G1-B9.6 retained memory / G1-B10 | **NOT MEASURED / NO-GO**, respectively. |

The strategic recommendation is to redirect future authorized architecture work toward Canonical Document ownership and transaction boundaries. That recommendation does not authorize a new milestone or implementation. Existing qualification evidence from FG-C3.3 through FG-C3.7.1 remains preserved; it does not imply deployment readiness.

