## P1.7 — Benchmark tooling

This directory contains the **P1.7 memory-profiling regression suite** for the
Logos performance initiative. It ensures that Phase 1 rendering improvements
(P1.1–P1.6) do not regress as the codebase evolves.

---

### Files

| File | Purpose |
|------|---------|
| `generate_file.clj` | Clojure script that produces `fixtures/large-canvas.logos` (10,000+ shapes, 5 pages) |
| `memory_profile.js`  | Playwright benchmark: loads fixture, runs undo/redo + page navigation, asserts heap limits |
| `analyse_snapshot.js`| Heap snapshot parser — reads `.heapsnapshot` JSON, extracts actionable metrics |
| `deps.edn`           | Clojure deps for the generator (includes `common/` namespaces) |
| `package.json`       | Node deps (`playwright`) |

---

### Prerequisites

1. **Logos running** at `http://localhost:3449` (all services — backend, frontend, exporter).
2. **Test account** with email `benchmark@logos.test`, password `benchmark123!` (or override via env vars).
3. **Java 17+** and **Clojure CLI** for the generator.
4. **Node 20+** for the Playwright script.

---

### Quick start

```bash
# 1. Generate the benchmark file (one-time, ~30 s)
cd test/benchmarks
clojure -M:gen-benchmark
# → writes fixtures/large-canvas.logos

# 2. Install Playwright browsers
npm install
npx playwright install chromium

# 3. Run the benchmark (requires running Logos)
node memory_profile.js

# 4. Inspect results
ls snapshots/
#  baseline.heapsnapshot  final.heapsnapshot  report-<timestamp>.json
```

---

### Thresholds

| Metric | Limit | Rationale |
|--------|-------|-----------|
| Heap size after scenario | ≤ 300 MB | external design tool ≈ 350–500 MB on comparable files; we target better. |
| Retained size growth (final − baseline) | ≤ 50 MB | Undo/redo and page nav should not leak retained memory. |
| Detached DOM nodes | ≤ 100 | Indicates component teardown failures. |
| ArrayBuffer count growth | ≤ 10 | Indicates buffer leaks in serialisation or WASM transfers. |

Override via environment variables (e.g. `LOGOS_HEAP_LIMIT_MB=400`).

---

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `LOGOS_URL` | `http://localhost:3449` | Base URL of the running Logos instance |
| `LOGOS_EMAIL` | `benchmark@logos.test` | Auth user email |
| `LOGOS_PASSWORD` | `benchmark123!` | Auth user password |
| `BENCHMARK_FILE` | `fixtures/large-canvas.logos` | Path to the .logos benchmark file |

---

### Interpreting results

The JSON report in `snapshots/report-<timestamp>.json` is machine-readable and
can be ingested by Grafana or OpenTelemetry (Phase 3). Watch for gradual creep:
a heap that grows from 250 MB → 290 MB over 30 commits is a warning sign even
before the 300 MB hard limit is reached.

Load the `.heapsnapshot` files in **Chrome DevTools → Memory → Load profile**
for interactive flamegraph inspection.

---

### References

- Brendan Gregg — *Systems Performance*, USE method applied to JavaScript heap.
- *Game Programming Patterns*, ch. 6 — time budget and measurement patterns.
- Chrome DevTools Protocol — `HeapProfiler.takeHeapSnapshot` specification.
