# Logos UX Acceptance Criteria

**Planning baseline:** `fb0ea41d73a553b31d6dea0c7b1a341d68fdba7c` (`main`, 2026-11-13)

**Status:** proposed test candidates. Numeric thresholds below are hypotheses, not validated baselines, release gates, or claims about current performance. Confirm fixtures, hardware, browser, measurement method, and user/task baseline before adopting targets.

These criteria operationalize the [product vision](../product/LOGOS-PRODUCT-VISION.md) and support the staged [roadmap](../roadmap/LOGOS-UNIFIED-PLATFORM-ROADMAP.md). They do not close the active P0.5 gates or approve a route/runtime change.

## Test candidate protocol

For every measurement, identify the supported editor route/build and artifact hashes; browser, OS, CPU/GPU, display/input device, network, and document fixture; cold versus warm cache; operation and start/end markers; sample count; p50/p95/p99; errors and timeouts; and test code/version. Record both input-to-visible time and operation completion where these differ. Run the same task/fixture across supported Logos platforms. Use automated browser traces plus representative moderated task sessions for discoverability and comprehension.

Use pinned Figma workflows as a UX comparison where access permits: same task, source state, input class, fixture complexity, environment, and outcome definition. Log the reference date/build/account capabilities and known mismatch. A Figma comparison is not evidence of internal architectural parity or every Figma feature.

## Proposed interaction targets

The following are concrete candidates for validation. Values should be revised or adopted only after baseline collection demonstrates that they are meaningful on agreed reference hardware and product fixtures.

| Candidate | Proposed target (not yet validated) | How to verify |
|---|---:|---|
| Single and multi-selection feedback | Input-to-visible selection state p95 ≤ **100 ms**, p99 ≤ **200 ms** | Click, modifier-toggle, marquee, and select-all across small/medium/large fixtures; assert selected IDs and rendered overlays, not just handler completion. |
| Tool activation and keyboard command feedback | p95 ≤ **100 ms** from key/button input to visible tool/command acknowledgment | Exercise tool switch, escape/cancel, duplicate/delete, focus traversal, and documented shortcuts; assert focus and resulting state. |
| Move/resize/rotate drag feedback | p95 frame interval ≤ **16.7 ms** during the measured interaction on the named reference setup; p95 input-to-visible update ≤ **50 ms** | Capture browser frame/input trace and final geometry; run short and sustained drags over representative visible object counts. Report dropped frames and long tasks. |
| Undo/redo feedback | p95 ≤ **100 ms** for a single local operation on the reference fixture | Apply, undo, redo, compare the complete semantic document state and rendered output; include grouped and dependent operations. |
| First usable canvas | p95 ≤ **3 s** for the agreed standard document on warm-cache reference hardware | Measure navigation/open request to first correct, interactive frame; separately report cold-cache, network, and server time. This is not an established SLA. |
| Task completion and recovery | ≥ **95%** completion on agreed critical task set; **0** unrecoverable document-state failures in acceptance runs | Representative users complete selection, create/edit, keyboard, undo/redo, save/reopen, and recover from a rejected/conflicted operation. Define cohort and confidence before sign-off. |

Targets for these rows remain provisional even if one run passes. Baseline first, publish the distribution, then obtain product/QA approval for release thresholds.

## Large-document fixture ladder

Use deterministic fixtures at candidate scales of **1,000**, **10,000**, and (where technically representative) **50,000** objects. These are fixture sizes, not supported-capacity or speed claims. Vary complexity independently: path nodes/segments, text runs and fonts, effects, nested frames/components, variables, layout constraints, imported assets, selection size, and page count. Track compressed/uncompressed file size and retained heap as well as object count.

For each fixture, measure open-to-first-usable-frame, select/move/resize, layout recalculation, viewport pan/zoom, undo/redo, save/reopen, and memory/heap over a fixed workflow. Record peak and post-idle heap, leaks over repeated open/close cycles, and browser/device failures. Separate initial load, steady-state interaction, and recovery. Proposed scale is not a gate until the fixture and reference environment are representative and measured.

## Fidelity, safety, and parity criteria

| Area | Acceptance evidence candidate |
|---|---|
| **Interaction correctness** | Browser tests cover selection (single/multiple/marquee), tool activation, pointer/keyboard input, cancellation, creation/edit, and focus. Compare intended state, rendered result, and persisted operation. |
| **Undo/redo and recovery** | Every supported edit is grouped as specified and has a tested inverse/recovery path. Undo/redo returns exact semantic state; save/reopen preserves it. Rejected, timed-out, retried, or conflicted operations produce no unintended partial mutation. |
| **Migration fidelity** | For each source and version, a matrix covers identity/tree/order, geometry, typography, style/effects, assets, components/instances, variables/modes, layout, interactions, and unknown fields. Include structural, deterministic visual, and behavioral checks plus machine-readable loss entries and human preview. Test export/rollback and cost/access assumptions. |
| **Cross-platform parity** | Run a shared scenario suite on every platform declared supported. Compare document semantics, authorization, history, and operation results. Document host-specific input/accessibility/UI differences and any intentionally unavailable feature. |
| **Security and isolation** | Verify unauthorized document/asset reads and writes fail; permission revocation takes effect; agent/plugin tools cannot escape declared grants; imported archives and payloads are bounded; malicious inputs fail safely; audit records identify actor and outcome. |
| **Privacy and data ownership** | Use synthetic/private fixtures to assert that unauthorized document, asset, or team data never enters another user's or an agent's context; outbound model requests are disclosed and scoped to authorized content; retention, deletion, and export outcomes match the published policy. Do not treat absence of a leak in one test as a blanket privacy certification. |
| **Accessibility and discoverability** | Keyboard-only task completion, visible focus, screen-reader names/states, contrast, reduced-motion behavior, and error comprehension are evaluated. Compare task completion and recovery, not visual resemblance alone. |

## Benchmark and reporting requirements

1. Freeze fixture hashes and publish their construction, legal status, and complexity profile.
2. Declare browser/device matrix and minimum supported conditions; never mix environments in a single percentile.
3. Warm-up and repeated-trial policy are explicit. Candidate minimum: 30 completed trials per deterministic interaction/condition; report count and confidence/variance rather than hiding noisy results.
4. Capture traces from input event to visible frame and completion/acknowledgment. Attribute client, worker, renderer, network, and server time where possible.
5. Report p50/p95/p99, frame-time distribution, heap/memory, long tasks, errors, timeouts, and task outcomes. Publish failures as results.
6. Compare with the previous validated Logos baseline and, for selected tasks, a pinned Figma reference. Call out incomparable features or access limitations.
7. Promote a proposed threshold to an acceptance gate only after product, QA, and engineering owners approve fixture representativeness and pass/fail policy.

## Evidence boundary

At the requested baseline, #64 remains the renderer/editor integration pixel blocker and #73 has repaired setup but blocked actual browser/heap measurements. Merged #67/#69/#70/#72 are supporting foundations only. An isolated renderer smoke, ABI test, persistence proof, or synthetic fixture does not by itself satisfy these UX criteria. See [roadmap stabilization gates](../roadmap/LOGOS-UNIFIED-PLATFORM-ROADMAP.md#active-p05-stabilization-gates) and the current [renderer plan](../architecture/RENDERER-EVOLUTION-PLAN.md).
