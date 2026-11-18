# FG-C3.7 — Supervised Local Process Qualification

**Status:** Executed locally in a disposable user-systemd service cgroup; operational feasibility is partial. Production integration and capability advertisement remain unauthorized.

**Baseline:** `50990afa5a73b9c434b0030dba0afbe94e539447`

**Branch:** `core/g1-b9-6-memory-evidence-foundation`
**Host:** Linux `6.12.112-1-MANJARO`, x86_64, systemd 261.3, Go `1.27.1-X:nodwarf5`

This report records the original FG-C3.7 shared-unit experiment. Its statements that Rust-planner OOM recovery was not demonstrated are historical and were addressed by the separately scoped follow-up in [FG-C3.7.1](FG-C3.7.1-PLANNER-OOM-RECOVERY.md). FG-C3.7.1 does not change this report's historical measurements or qualify production behavior.

## Result

The host exposes cgroup v2 `cpu`, `memory`, and `pids` controllers. The active user systemd manager can create and collect a disposable transient service with enforced resource properties. The test exercised a Rust planner inside that service, verified resource events, and emitted a machine-readable `FG_C3_7_EVIDENCE` record.

This is **local process-containment feasibility evidence**, not production readiness. The user service has `Delegate=no`; the supervisor does not receive authority to create nested child cgroups. The test therefore puts the Go test harness, Rust planner, and helper processes in one dedicated transient service cgroup. The planner is lifecycle-contained within the unit, but does not have an independently metered child cgroup. The experiment did not alter global controllers, other units, or production processes.

`systemd-run --user` applied `MemoryMax=384M`, `MemorySwapMax=0`, `CPUQuota=50%`, `TasksMax=64`, `LimitNOFILE=128`, and `OOMPolicy=continue`. The service also used `KillMode=control-group`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateTmp=yes`, `PrivateNetwork=yes`, and `ProtectControlGroups=yes`. The workload observed `NoNewPrivs=1`, zero effective capabilities, `Seccomp=0`, and `RLIMIT_NOFILE=128`. The cgroup filesystem was readable for telemetry and protected by the service's control-group mount policy.

## Controlled execution evidence

The qualification runner used 32 copies of the stable `reorder.json` request fixture, with unique request IDs. The fixed-pool comparison completed the same 32 requests with no errors; a separate 32-request burst used a two-worker pool with queue capacity four to exercise overload rejection. Percentiles use nearest lower rank. Single-flight queue delay is estimated by subtracting the measured warm service baseline from end-to-end duration; pool queue delay is measured from enqueue to worker start.

| Workload | Completed / rejected | Elapsed | Throughput | P50 / P95 request latency | P95 queue delay |
| --- | ---: | ---: | ---: | ---: | ---: |
| Existing single-flight planner, 32 requests | 32 / 0 | 86.059 ms | 371.8 req/s | 66.110 / 83.073 ms | 81.541 ms estimated |
| Test-only two-process pool, queue 32 | 32 / 0 | 83.839 ms | 381.7 req/s | 72.709 / 82.541 ms | 81.171 ms |
| Test-only two-process pool, queue 4 burst | 6 / 26 | 4.951 ms | 1,211.7 completed req/s for this burst only | 3.656 / 4.936 ms | 3.656 ms |

These short local timings varied between repeated runs. The final 32-request sample shows only a small pool difference with higher P95 latency than its P50, which is not evidence of a repeatable performance benefit. They do not establish a production performance result or capacity. The overload case proves rejection at the configured queue boundary; its throughput is not comparable to the all-completed runs.

| Resource observation | Result |
| --- | --- |
| `memory.max` | 402,653,184 bytes (384 MiB) |
| `memory.peak` | 402,653,184 bytes; the configured ceiling was reached |
| Controlled allocation | A separate Python child attempted 768 MiB; it failed and `memory.events` recorded `oom=1`, `oom_kill=1` |
| CPU | `cpu.max=50000 100000`; `cpu.stat` recorded 4 throttled periods in this run |
| Tasks | `pids.max=64`; `pids.peak=64`; 49 child starts succeeded before the limit rejected further starts; `pids.events max` increased by 1 |
| File descriptors | Soft and hard limit 128 |
| Cleanup | The child processes were killed and waited; `systemd-run --collect` removed the transient service after completion |

The OOM workload targeted its allocator helper, not the Rust planner. It establishes that the cgroup memory cap and OOM telemetry work and that the evidence collector survived under `OOMPolicy=continue`. Rust planner process death from OOM is **not directly demonstrated** by this run. Existing coordinator tests separately cover crash, malformed/truncated protocol, failed startup handshake, timeout/cancellation, semantic rejection, and restart behavior.

Repeated missing-executable starts were exercised through a test-only qualification supervisor: four attempts with 25/50/100 ms exponential delays returned failure, then a valid planner succeeded with the expected request ID. Semantic rejection remains a protocol result (`err == nil`) and is returned without process retry. This backoff code exists only in the qualification test; the coordinator's `Planner` itself has no restart backoff or circuit breaker.

The planner binary used for this run had SHA-256 `d53ed69ef0623a04592ff78c8c823b25c16e3a7f94b319a30f4486239a6cd2d9`.

## Threat model and limits

- cgroups account for and constrain resources; they are not a complete security sandbox. The run used systemd namespace and filesystem properties in addition to cgroup limits, but no seccomp filter (`Seccomp=0`) was active.
- `PrivateNetwork=yes` isolates the service network namespace. This does not establish a complete network policy for a future deployment.
- The service can read repository code and executable inputs from the home directory; those paths are read-only. `PrivateTmp` provides a disposable writable temporary directory. No production files or external service credentials were used.
- `ProtectControlGroups=yes` prevents the workload from managing cgroups through its service view. Systemd owns the transient unit and kills its remaining processes as a group on unit teardown. Nested cgroup delegation, a planner-only child cgroup, arbitrary hostile native-code escape resistance, and deployment-level identity controls remain unverified.
- The experiment did not change the production Go launcher. Its timeout path still kills and waits for the direct child; production descendant containment and restart policy are not supplied by this spike.
- OOM victim selection is kernel-controlled. `OOMPolicy=continue` allowed the test collector to record an individual helper kill. A production policy for planner OOM, whole-unit termination, and subsequent request behavior remains undecided.
- Local measurements use one host, one debug planner executable, and a small synthetic fixture. They are not production load, memory-retention, or service-capacity results.

The Rust capability handshake was checked during the contained run and remains `componentInstanceDomain.status = "not-supported-here"`. No authority transfer, persistent document access, component capability advertisement, or production service integration occurred.

## Reproduction

From the repository root on Linux with cgroup v2 and an active user systemd manager:

```sh
./scripts/fg-c3-7-qualify.sh
```

The script fails closed if cgroup v2, required controllers, user systemd, or Python are unavailable. It builds the Rust binary, compiles the Go test binary, launches one uniquely named transient user service with the limits above, runs the controlled experiment, and collects the unit. It does not create nested cgroups, change global resource controls, or fall back to an uncontained run.

The local regression set was run separately:

```sh
cargo fmt --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml -- --check
cargo test --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml --workspace
node --test test/figma-semantics/component-instance-domain-v1.test.mjs
node --test test/figma-semantics/fg-c3-5-protocol-contract.test.mjs
node test/figma-semantics/component-instance-differential.mjs
FG_C3_2_2_RUST_PLANNER="$PWD/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" go -C spikes/fg-c3-2-1/go-coordinator test ./... -count=1
FG_C3_2_2_RUST_PLANNER="$PWD/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" go -C spikes/fg-c3-2-1/go-coordinator test -race ./... -count=1
cargo build --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml --target wasm32-unknown-unknown --features wasm
wasm-bindgen spikes/fg-c3-2-1/rust-planner/target/wasm32-unknown-unknown/debug/fg_c3_2_1_rust_planner_spike.wasm --target nodejs --out-dir /tmp/fg-c3-7-wasm-node
FG_C3_6_WASM_NODE_MODULE=/tmp/fg-c3-7-wasm-node/fg_c3_2_1_rust_planner_spike.js FG_C3_6_RUST_PLANNER="$PWD/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" node test/figma-semantics/component-instance-protocol-wasm-parity.mjs
```

## Verification status and decision

| Check | Result |
| --- | --- |
| Rust planner workspace | 18 passed, 0 failed |
| Rust planner formatting | Passed |
| Go coordinator | Passed locally |
| Go coordinator race detector | Passed locally |
| TypeScript Component/Instance tests | 13 passed, 0 failed; runner emitted a non-failing notice that port 24678 was already in use |
| FG-C3.5 protocol contract | 2 passed, 0 failed |
| FG-C3.5 differential corpus | 339 cases: 338 matched, 0 divergences, 1 unsupported JavaScript-only NaN case |
| Native Rust / Node-hosted WASM parity | 4/4 matched |
| Hosted CI | Not run; the GitHub billing restriction remains an infrastructure blocker |
| Production execution | Not performed or authorized |

**Operational feasibility verdict:** Partial. A user-systemd transient service can enforce and report cgroup v2 limits on this host, contain the complete test process tree for the unit lifecycle, and safely exercise OOM and task limits. A separately delegated child cgroup for the Rust planner, direct Rust-planner OOM recovery, stronger syscall isolation, production packaging, and deployment controls remain open. Do not advertise the Component/Instance capability or interpret this experiment as production readiness. Hosted CI evidence remains unavailable; local results are not hosted CI.
