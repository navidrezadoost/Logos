# FG-C3.7.1 — Planner-Specific Failure Isolation and OOM Recovery

**Status:** Executed locally; planner-specific OOM and recovery verified on this host only. Operational feasibility remains partial.

**Baseline:** `aa36294a7ea5ce0c33948ba6a25e96da96473e15`  
**Branch:** `core/g1-b9-6-memory-evidence-foundation`  
**Host:** Linux `6.12.112-1-MANJARO`, x86_64, systemd 261.3, cgroup v2; user manager has no delegated nested child-cgroup authority.

## Failure-domain comparison

| Model | Result | Evidence and limit |
| --- | --- | --- |
| A — independently resource-contained planner | **Verified locally with a sibling transient systemd scope.** | `systemd-run --user --scope` placed the Rust process in a separately metered scope with `MemoryMax=64M`, `MemorySwapMax=0`, `CPUQuota=50%`, and `TasksMax=16`. This host did not provide nested cgroup delegation. The scope also held a small `sleep` keeper so its OOM counters and systemd result remained readable after Rust exited. This is not evidence of a planner-only scope membership or of a production service arrangement. |
| B — whole coordinator/planner unit is the failure domain | **Partially verified by FG-C3.7.** | FG-C3.7 enforced limits on a shared transient service and tested process-tree cleanup. External restart of that whole unit after a unit-level OOM was not executed. |

No global cgroup settings, other user services, or production workloads were changed. The experiment did not require privileged delegation. A future deployment must independently establish its own systemd/cgroup ownership and lifecycle model.

## Rust planner OOM experiment

The qualification runner builds the Rust spike with the test-only Cargo feature `fg-c3-7-test-oom`. That feature adds an out-of-band marker-file trigger and PID/cgroup/RSS evidence. The trigger runs immediately before document validation, is not a protocol operation or document field, and is absent from a default build. It allocates and touches bounded memory only after the test creates the marker. The runner launches it in a disposable user-systemd scope and removes the scope during cleanup.

Observed run:

| Observation | Result |
| --- | --- |
| Rust planner PID / Go test supervisor PID | `675743` / `675661` |
| Rust `/proc/self/cgroup` | `/user.slice/user-1000.slice/user@1000.service/app.slice/logos-fg-c371-planner-675661-675743.scope` |
| Scope limits | `memory.max=67108864`, `memory.oom.group=0`, `pids.max=16` |
| Scope `memory.peak` | `67108864` bytes |
| Scope `memory.events` `oom_kill` | `0` before, `1` after |
| systemd scope properties | `Result=oom-kill`, `OOMKills=1` |
| Rust last sampled RSS | `69160 KiB`; sample was recorded during allocation before the kill |
| Interrupted validation | Go returned a nil response with a transport error; no successful validation was accepted |
| Recovery | Go supervisor remained alive; worker PID `675769` handled request ID `fg371-oom-interrupted` and returned a successful, correlated result |
| Cleanup | OOM victim PID was absent; keeper remained until evidence collection, then both original and restarted scopes were stopped |

The scope's `ExecMainCode` and `ExecMainStatus` properties were empty, so this run does not claim a numeric Rust exit status. The direct evidence for the Rust OOM victim is the recorded PID and cgroup membership, its disappearance, the scope OOM event and systemd `oom-kill` result, while the Go supervisor remains alive. The scope held both Rust and a small keeper; its cgroup counters therefore describe the scope, not an individually metered Rust process.

The OOM-interrupted call was not retried as successful. A later, explicit call through the existing `Planner` started a new worker and returned the same validation request ID successfully. This demonstrates recovery in the local test harness; it does not establish durable operation replay semantics or production recovery behavior.

## Lifecycle qualification

The test-only `qualificationSupervisor` is separate from the coordinator's actual `Planner`. It is restricted to `validateComponentInstanceV1` requests and refuses non-validation operations. Four missing-executable attempts exhausted after 25/50/100 ms delays, capped at 100 ms. Cancellation during backoff stopped further attempts. Semantic rejection returned a protocol response once without retry. This qualifies the experiment helper only; production startup backoff, circuit breaking, and process restart policy remain unimplemented.

Existing planner/coordinator tests cover malformed and truncated responses, startup handshake failure, crash, timeout/cancellation, and semantic rejection. A concurrent worker-failure regression launches 24 callers and checks that failures fail closed while recovered responses retain their request IDs. No unsuccessful result may be treated as a successful validation.

## Reproduction

From the repository root on Linux with cgroup v2, an active user systemd manager, and the required controllers:

```sh
./scripts/fg-c3-7-qualify.sh
```

The script builds with the test-only OOM feature, starts the existing FG-C3.7 outer disposable service, runs its resource and planner-specific experiments, and cleans up the transient scopes. It fails rather than falling back to uncontained execution when required host facilities are absent. The Rust planner executable hash and full machine-readable observations are printed in `FG_C3_7_EVIDENCE`.

The exact observed planner executable SHA-256 for the run above was `dae7bf683317bc21bb27586a7c6f36bc8afa4f33a3bd771380d77d87ed1fe9fb`. Measurements and PIDs are host/run-specific and are not expected to repeat.

## Security and readiness limits

- cgroups provide resource accounting and limits, not a complete security sandbox. This run does not establish seccomp, trusted executable provenance, hostile-code escape resistance, production filesystem policy, or deployment identity controls.
- The planner and Go supervisor were separate cgroup members; the planner scope was a sibling transient scope created by the user systemd manager, not a delegated nested child cgroup.
- The experiment did not modify production React, Go, or Rust integration; persistence; schemas; or capability advertisement. `componentInstanceDomain.status` remains `not-supported-here`.
- No hosted CI was run. Local evidence is not equivalent to hosted CI or independent reproduction.
- FG-C3.7.1 does not measure retained memory or production performance. G1-B9.6 retained memory remains **NOT MEASURED**.
- Production deployment and authority transfer remain unauthorized. Capability advertisement remains **NO-GO** pending subsequent architecture and operational gates.

## Regression checks

The applicable local regression commands are recorded in the FG-C3.7 report. FG-C3.7.1 execution on this host additionally ran the OOM qualification and recovery path above. Hosted CI remains unavailable due to the previously reported GitHub billing restriction.
