package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

type cgroupEvidence struct {
	Path   string            `json:"path"`
	Values map[string]string `json:"values"`
}

type poolMeasurement struct {
	Mode             string  `json:"mode"`
	Requests         int     `json:"requests"`
	Completed        int     `json:"completed"`
	Rejected         int     `json:"overloadRejected"`
	ElapsedMs        float64 `json:"elapsedMs"`
	ThroughputPerSec float64 `json:"throughputCompletedPerSecond"`
	P50Ms            float64 `json:"requestLatencyP50Ms"`
	P95Ms            float64 `json:"requestLatencyP95Ms"`
	P95QueueMs       float64 `json:"queueDelayP95Ms"`
	Workers          int     `json:"workers"`
	QueueCapacity    int     `json:"queueCapacity"`
}

type fgC37Evidence struct {
	Status          string          `json:"status"`
	Kernel          string          `json:"kernel"`
	Architecture    string          `json:"architecture"`
	GoVersion       string          `json:"goVersion"`
	Isolation       string          `json:"isolation"`
	Workload        string          `json:"workload"`
	Controller      cgroupEvidence  `json:"cgroup"`
	SingleFlight    poolMeasurement `json:"singleFlight"`
	BoundedPool     poolMeasurement `json:"boundedPool"`
	Overload        poolMeasurement `json:"boundedQueueOverload"`
	Recovery        string          `json:"recovery"`
	CapabilityState string          `json:"componentInstanceCapabilityStatus"`
	PlannerSHA256   string          `json:"plannerExecutableSha256"`
	RestartPolicy   string          `json:"testOnlyRestartBackoff"`
}

// TestFGC37SystemdWorkload is run only by the explicit cgroup qualification
// script. Ordinary go test runs never create a cgroup or an OOM workload.
func TestFGC37SystemdWorkload(t *testing.T) {
	if os.Getenv("FG_C3_7_RUN") != "1" {
		t.Skip("requires the explicit disposable systemd cgroup runner")
	}
	cgroup, err := readCurrentCgroupEvidence()
	if err != nil {
		t.Fatal(err)
	}
	for file, want := range map[string]string{"memory.max": "402653184", "cpu.max": "50000 100000", "pids.max": "64"} {
		if got := strings.TrimSpace(cgroup.Values[file]); got != want {
			t.Fatalf("effective %s = %q, want %q", file, got, want)
		}
	}
	if strings.TrimSpace(cgroup.Values["NoNewPrivs"]) != "1" || strings.TrimSpace(cgroup.Values["nofile"]) != "128" {
		t.Fatalf("process hardening limits not effective: %+v", cgroup.Values)
	}
	if cgroup.Values["oom_kill"] == "" {
		t.Fatal("memory.events oom_kill counter is unavailable")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	path := rustPath(t)
	capabilityRequest := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"fg37-capability","kind":"capabilities"}`)
	capabilityPlanner := NewPlanner(path)
	capabilityRaw, err := capabilityPlanner.Call(ctx, capabilityRequest)
	if err != nil {
		t.Fatalf("Rust capability handshake failed: %v", err)
	}
	var capabilityResponse response
	if err := json.Unmarshal(capabilityRaw, &capabilityResponse); err != nil || !capabilityResponse.OK {
		t.Fatalf("Rust capability response invalid: %s err=%v", capabilityRaw, err)
	}
	domain, ok := capabilityResponse.Capabilities["componentInstanceDomain"].(map[string]any)
	if !ok || domain["status"] != "not-supported-here" {
		t.Fatalf("component-instance capability changed: %v", capabilityResponse.Capabilities)
	}
	capabilityPlanner.Close()
	request := fixture(t, "reorder.json")
	planner := NewPlanner(path)
	first, err := planner.Call(ctx, request)
	if err != nil {
		t.Fatalf("first framed Rust request failed: %v", err)
	}
	var response response
	if err := json.Unmarshal(first, &response); err != nil || !response.OK {
		t.Fatalf("Rust request did not validate successfully: response=%s err=%v", first, err)
	}
	planner.Close()

	single := measureSingleFlight(t, path, request, 32)
	pool := measureBoundedPool(t, path, request, 32, 2, 32)
	overload := measureBoundedPool(t, path, request, 32, 2, 4)
	if pool.Workers != 2 || pool.QueueCapacity != 32 || pool.Completed != pool.Requests || pool.Rejected != 0 {
		t.Fatalf("bounded pool did not complete the comparable workload: %+v", pool)
	}
	if overload.Workers != 2 || overload.QueueCapacity != 4 || overload.Rejected == 0 || overload.Completed+overload.Rejected != overload.Requests {
		t.Fatalf("bounded pool did not enforce overload rejection: %+v", overload)
	}
	if single.Completed != single.Requests {
		t.Fatalf("single-flight workload unexpectedly failed: %+v", single)
	}

	// Force an allocation beyond this unit's 384 MiB memory.max. Only the
	// helper child is expected to be OOM-killed; OOMPolicy=continue keeps the
	// evidence collector alive. The host and other user units are unaffected.
	beforeOOM, err := readCgroupCounter(cgroup.Path, "memory.events", "oom_kill")
	if err != nil {
		t.Fatal(err)
	}
	pythonPath, err := exec.LookPath("python3")
	if err != nil {
		t.Fatalf("python3 is required for the bounded OOM experiment: %v", err)
	}
	allocator := `import time; b=bytearray(768*1024*1024); [b.__setitem__(i,1) for i in range(0,len(b),4096)]; time.sleep(0.1)`
	alloc := exec.CommandContext(ctx, pythonPath, "-c", allocator)
	allocErr := alloc.Run()
	if allocErr == nil {
		t.Fatal("allocation above memory.max unexpectedly succeeded")
	}
	afterOOM, err := readCgroupCounter(cgroup.Path, "memory.events", "oom_kill")
	if err != nil {
		t.Fatal(err)
	}
	if afterOOM <= beforeOOM {
		t.Fatalf("memory limit did not record an OOM kill: before=%d after=%d allocatorErr=%v", beforeOOM, afterOOM, allocErr)
	}
	beforePidsMax, err := readCgroupCounter(cgroup.Path, "pids.events", "max")
	if err != nil {
		t.Fatal(err)
	}
	children := make([]*os.Process, 0, 96)
	for range 96 {
		child := exec.Command("sleep", "10")
		if err := child.Start(); err != nil {
			break
		}
		children = append(children, child.Process)
	}
	for _, child := range children {
		_ = child.Kill()
		_, _ = child.Wait()
	}
	afterPidsMax, err := readCgroupCounter(cgroup.Path, "pids.events", "max")
	if err != nil {
		t.Fatal(err)
	}
	if afterPidsMax <= beforePidsMax {
		t.Fatalf("pids.max did not reject process creation: before=%d after=%d started=%d", beforePidsMax, afterPidsMax, len(children))
	}
	cgroup.Values["processStormChildrenStarted"] = strconv.Itoa(len(children))
	cgroup.Values["pids.maxEventDelta"] = strconv.FormatUint(afterPidsMax-beforePidsMax, 10)
	backoffEvidence := qualifyStartupBackoff(t, path, request)

	// Verify recovery after a real worker restart. A semantic rejection is a
	// valid protocol result; the subsequent valid request must remain correlated.
	recovery := NewPlanner(path)
	if err := recovery.start(ctx); err != nil {
		t.Fatal(err)
	}
	oldPID := recovery.PID()
	recovery.stop()
	if _, err := recovery.Call(ctx, request); err != nil {
		t.Fatalf("request after controlled worker stop failed: %v", err)
	}
	if oldPID == 0 || recovery.PID() == 0 || oldPID == recovery.PID() {
		t.Fatalf("worker did not restart cleanly: before=%d after=%d", oldPID, recovery.PID())
	}
	recovery.Close()

	finalCgroup, err := readCurrentCgroupEvidence()
	if err != nil {
		t.Fatal(err)
	}
	finalCgroup.Values["processStormChildrenStarted"] = cgroup.Values["processStormChildrenStarted"]
	finalCgroup.Values["pids.maxEventDelta"] = cgroup.Values["pids.maxEventDelta"]
	kernel, err := os.ReadFile("/proc/sys/kernel/osrelease")
	if err != nil {
		t.Fatal(err)
	}
	plannerBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	plannerDigest := sha256.Sum256(plannerBytes)
	evidence := fgC37Evidence{
		Status:       "executed-in-disposable-user-systemd-cgroup; local evidence only",
		Kernel:       strings.TrimSpace(string(kernel)),
		Architecture: runtime.GOARCH, GoVersion: runtime.Version(),
		Isolation:  "systemd transient service cgroup; NoNewPrivileges; ProtectControlGroups; PrivateNetwork; ProtectSystem=strict; PrivateTmp; service cleanup by control-group",
		Workload:   "32 stable reorder fixture requests; single-flight versus 2-worker/32-queue test pool; 32-request burst against 2-worker/4-queue pool; 768 MiB allocator under 384 MiB memory.max; bounded process storm",
		Controller: finalCgroup, SingleFlight: single, BoundedPool: pool, Overload: overload,
		Recovery:        "controlled OOM child rejected; memory.events oom_kill incremented; pids.max rejected child creation; child processes reaped; Rust planner restarted and next framed request succeeded",
		CapabilityState: "not-supported-here",
		PlannerSHA256:   hex.EncodeToString(plannerDigest[:]),
		RestartPolicy:   backoffEvidence,
	}
	encoded, err := json.Marshal(evidence)
	if err != nil {
		t.Fatal(err)
	}
	fmt.Printf("FG_C3_7_EVIDENCE %s\n", encoded)
}

func readCurrentCgroupEvidence() (cgroupEvidence, error) {
	raw, err := os.ReadFile("/proc/self/cgroup")
	if err != nil {
		return cgroupEvidence{}, err
	}
	var path string
	for _, line := range strings.Split(string(raw), "\n") {
		if strings.HasPrefix(line, "0::") {
			path = strings.TrimPrefix(line, "0::")
			break
		}
	}
	if path == "" || strings.Contains(path, "..") {
		return cgroupEvidence{}, fmt.Errorf("unusable unified cgroup path %q", path)
	}
	base := filepath.Join("/sys/fs/cgroup", filepath.Clean(path))
	values := map[string]string{}
	for _, file := range []string{"memory.max", "memory.peak", "memory.events", "cpu.max", "cpu.stat", "pids.max", "pids.current", "pids.peak", "pids.events"} {
		value, err := os.ReadFile(filepath.Join(base, file))
		if err != nil {
			return cgroupEvidence{}, fmt.Errorf("read cgroup %s: %w", file, err)
		}
		values[file] = strings.TrimSpace(string(value))
	}
	for _, line := range strings.Split(values["memory.events"], "\n") {
		fields := strings.Fields(line)
		if len(fields) == 2 {
			values[fields[0]] = fields[1]
		}
	}
	status, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return cgroupEvidence{}, err
	}
	for _, line := range strings.Split(string(status), "\n") {
		if strings.HasPrefix(line, "NoNewPrivs:") {
			values["NoNewPrivs"] = strings.TrimSpace(strings.TrimPrefix(line, "NoNewPrivs:"))
		}
		if strings.HasPrefix(line, "CapEff:") {
			values["CapEff"] = strings.TrimSpace(strings.TrimPrefix(line, "CapEff:"))
		}
		if strings.HasPrefix(line, "Seccomp:") {
			values["Seccomp"] = strings.TrimSpace(strings.TrimPrefix(line, "Seccomp:"))
		}
	}
	nofile, err := os.ReadFile("/proc/self/limits")
	if err == nil {
		for _, line := range strings.Split(string(nofile), "\n") {
			if strings.HasPrefix(line, "Max open files") {
				fields := strings.Fields(line)
				if len(fields) >= 4 {
					values["nofile"] = fields[3]
				}
			}
		}
	}
	return cgroupEvidence{Path: path, Values: values}, nil
}

func readCgroupCounter(path, file, key string) (uint64, error) {
	raw, err := os.ReadFile(filepath.Join("/sys/fs/cgroup", filepath.Clean(path), file))
	if err != nil {
		return 0, err
	}
	for _, line := range strings.Split(string(raw), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 2 && fields[0] == key {
			return strconv.ParseUint(fields[1], 10, 64)
		}
	}
	return 0, fmt.Errorf("%s counter missing from %s", key, file)
}

type poolTask struct {
	queued time.Time
	id     string
}

type poolResult struct {
	latency time.Duration
	queue   time.Duration
	err     error
}

func measureSingleFlight(t *testing.T, path string, request []byte, count int) poolMeasurement {
	t.Helper()
	p := NewPlanner(path)
	defer p.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	var service time.Duration
	for i := range 8 {
		started := time.Now()
		if _, err := p.Call(ctx, requestWithID(t, request, fmt.Sprintf("fg37-single-base-%d", i))); err != nil {
			t.Fatalf("single-flight service-time baseline failed: %v", err)
		}
		service += time.Since(started)
	}
	service /= 8
	results := make([]poolResult, count)
	start := time.Now()
	var wg sync.WaitGroup
	for i := range results {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			enqueued := time.Now()
			requestCtx, requestCancel := context.WithTimeout(context.Background(), 20*time.Second)
			defer requestCancel()
			_, err := p.Call(requestCtx, requestWithID(t, request, fmt.Sprintf("fg37-single-%d", i)))
			latency := time.Since(enqueued)
			queue := latency - service
			if queue < 0 {
				queue = 0
			}
			results[i] = poolResult{latency: latency, queue: queue, err: err}
		}(i)
	}
	wg.Wait()
	return summarizePool("single-flight-current", results, time.Since(start), count, 1, 0)
}

func measureBoundedPool(t *testing.T, path string, request []byte, count, workers, capacity int) poolMeasurement {
	t.Helper()
	queue := make(chan poolTask, capacity)
	results := make(chan poolResult, count)
	var wg sync.WaitGroup
	planners := make([]*Planner, workers)
	for i := range planners {
		planners[i] = NewPlanner(path)
		defer planners[i].Close()
		wg.Add(1)
		go func(p *Planner) {
			defer wg.Done()
			for task := range queue {
				started := time.Now()
				ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
				rawResponse, err := p.Call(ctx, requestWithID(t, request, task.id))
				if err == nil {
					var decoded response
					err = json.Unmarshal(rawResponse, &decoded)
					if err == nil && decoded.RequestID != task.id {
						err = fmt.Errorf("response correlation mismatch: got %q want %q", decoded.RequestID, task.id)
					}
				}
				cancel()
				results <- poolResult{latency: time.Since(task.queued), queue: started.Sub(task.queued), err: err}
			}
		}(planners[i])
	}
	start := time.Now()
	rejected := 0
	for i := range count {
		task := poolTask{queued: time.Now(), id: fmt.Sprintf("fg37-pool-%d-%d", capacity, i)}
		select {
		case queue <- task:
		default:
			rejected++
			results <- poolResult{err: fmt.Errorf("overload: bounded queue is full")}
		}
	}
	close(queue)
	wg.Wait()
	close(results)
	all := make([]poolResult, 0, count)
	for result := range results {
		all = append(all, result)
	}
	measurement := summarizePool("fixed-pool", all, time.Since(start), count, workers, capacity)
	measurement.Rejected = rejected
	return measurement
}

func requestWithID(t *testing.T, request []byte, id string) []byte {
	t.Helper()
	var value map[string]any
	if err := json.Unmarshal(request, &value); err != nil {
		t.Fatalf("decode request for unique correlation id: %v", err)
	}
	value["requestId"] = id
	encoded, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("encode request for unique correlation id: %v", err)
	}
	return encoded
}

func qualifyStartupBackoff(t *testing.T, validPath string, request []byte) string {
	t.Helper()
	missing := filepath.Join(t.TempDir(), "planner-not-installed")
	supervisor := qualificationSupervisor{Path: missing, MaxAttempts: 4, InitialBackoff: 25 * time.Millisecond}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	_, err := supervisor.Call(ctx, requestWithID(t, request, "fg37-backoff-fail"))
	cancel()
	if err == nil || supervisor.Attempts != 4 {
		t.Fatalf("repeated startup failures were not bounded/fail-closed: attempts=%d err=%v", supervisor.Attempts, err)
	}
	supervisor.Path = validPath
	ctx, cancel = context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	rawResponse, err := supervisor.Call(ctx, requestWithID(t, request, "fg37-backoff-recovered"))
	if err != nil {
		t.Fatalf("valid planner failed after test-only backoff: %v", err)
	}
	var decoded response
	if err := json.Unmarshal(rawResponse, &decoded); err != nil || !decoded.OK || decoded.RequestID != "fg37-backoff-recovered" {
		t.Fatalf("backoff recovery returned incorrect response: response=%s err=%v", rawResponse, err)
	}
	attemptsBeforeSemanticReject := supervisor.Attempts
	semanticRaw, err := supervisor.Call(ctx, requestWithID(t, fixture(t, "cycle-reject.json"), "fg37-semantic-reject"))
	if err != nil {
		t.Fatalf("semantic rejection was incorrectly escalated to a process failure: %v", err)
	}
	if err := json.Unmarshal(semanticRaw, &decoded); err != nil || decoded.OK || decoded.RequestID != "fg37-semantic-reject" || supervisor.Attempts != attemptsBeforeSemanticReject+1 {
		t.Fatalf("semantic rejection triggered a retry or lost correlation: response=%s err=%v attempts=%d", semanticRaw, err, supervisor.Attempts)
	}
	return "test-only supervisor made 4 bounded startup attempts with exponential 25/50/100ms backoff; failed starts never produced success; valid planner recovered; semantic rejection returned once without retry; policy is not in production Planner"
}

// qualificationSupervisor is isolated to this test file and deliberately not
// used by the coordinator or any production command.
type qualificationSupervisor struct {
	Path           string
	MaxAttempts    int
	InitialBackoff time.Duration
	Attempts       int
}

func (s *qualificationSupervisor) Call(ctx context.Context, request []byte) ([]byte, error) {
	backoff := s.InitialBackoff
	var lastErr error
	for attempt := 0; attempt < s.MaxAttempts; attempt++ {
		if attempt > 0 {
			timer := time.NewTimer(backoff)
			select {
			case <-ctx.Done():
				timer.Stop()
				return nil, ctx.Err()
			case <-timer.C:
			}
			backoff *= 2
		}
		s.Attempts++
		planner := NewPlanner(s.Path)
		response, err := planner.Call(ctx, request)
		planner.Close()
		if err == nil {
			return response, nil
		}
		lastErr = err
	}
	return nil, lastErr
}

func summarizePool(mode string, results []poolResult, elapsed time.Duration, requests, workers, capacity int) poolMeasurement {
	latencies, queues := []float64{}, []float64{}
	completed, rejected := 0, 0
	for _, result := range results {
		if result.err != nil {
			rejected++
			continue
		}
		completed++
		latencies = append(latencies, float64(result.latency.Microseconds())/1000)
		queues = append(queues, float64(result.queue.Microseconds())/1000)
	}
	sort.Float64s(latencies)
	sort.Float64s(queues)
	seconds := elapsed.Seconds()
	throughput := 0.0
	if seconds > 0 {
		throughput = float64(completed) / seconds
	}
	return poolMeasurement{Mode: mode, Requests: requests, Completed: completed, Rejected: rejected, ElapsedMs: float64(elapsed.Microseconds()) / 1000,
		ThroughputPerSec: throughput, P50Ms: percentile(latencies, .50), P95Ms: percentile(latencies, .95), P95QueueMs: percentile(queues, .95), Workers: workers, QueueCapacity: capacity}
}

func percentile(values []float64, p float64) float64 {
	if len(values) == 0 {
		return 0
	}
	index := int(float64(len(values)-1) * p)
	return values[index]
}
