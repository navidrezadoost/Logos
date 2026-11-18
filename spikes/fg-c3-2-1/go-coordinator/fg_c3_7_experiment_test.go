package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
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
	Status          string                    `json:"status"`
	Kernel          string                    `json:"kernel"`
	Architecture    string                    `json:"architecture"`
	GoVersion       string                    `json:"goVersion"`
	Isolation       string                    `json:"isolation"`
	Workload        string                    `json:"workload"`
	Controller      cgroupEvidence            `json:"cgroup"`
	SingleFlight    poolMeasurement           `json:"singleFlight"`
	BoundedPool     poolMeasurement           `json:"boundedPool"`
	Overload        poolMeasurement           `json:"boundedQueueOverload"`
	Recovery        string                    `json:"recovery"`
	CapabilityState string                    `json:"componentInstanceCapabilityStatus"`
	PlannerSHA256   string                    `json:"plannerExecutableSha256"`
	RestartPolicy   string                    `json:"testOnlyRestartBackoff"`
	PlannerOOM      plannerOOMEvidence        `json:"plannerSpecificOOM"`
	FailureDomains  []failureDomainAssessment `json:"failureDomains"`
}

type plannerOOMEvidence struct {
	Status              string `json:"status"`
	PlannerPID          int    `json:"plannerPid"`
	PlannerScope        string `json:"plannerScope"`
	PlannerRSSKiB       string `json:"plannerLastSampledRssKiB"`
	SupervisorPID       int    `json:"goSupervisorPid"`
	SupervisorSurvived  bool   `json:"goSupervisorSurvived"`
	MemoryMax           string `json:"memoryMax"`
	MemoryPeak          string `json:"memoryPeak"`
	OOMKillBefore       uint64 `json:"oomKillBefore"`
	OOMKillAfter        uint64 `json:"oomKillAfter"`
	OOMGroup            string `json:"oomGroup"`
	ScopeResult         string `json:"systemdScopeResult"`
	ScopeExecMainCode   string `json:"systemdScopeExecMainCode"`
	ScopeExecMainStatus string `json:"systemdScopeExecMainStatus"`
	ScopeOOMKills       string `json:"systemdScopeOomKills"`
	InterruptedRequest  string `json:"interruptedRequestResult"`
	RestartRequestID    string `json:"restartRequestId"`
	RestartWorkerPID    int    `json:"restartWorkerPid"`
	OrphanCheck         string `json:"orphanCheck"`
}

type failureDomainAssessment struct {
	Model  string `json:"model"`
	Status string `json:"status"`
	Reason string `json:"reason"`
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
	plannerOOM := qualifyPlannerSpecificOOM(t, path)

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
		PlannerOOM:      plannerOOM,
		FailureDomains: []failureDomainAssessment{
			{Model: "A — independent planner cgroup", Status: "VERIFIED LOCALLY", Reason: "user systemd created a sibling transient scope with its own memory.max; the Rust PID was the OOM victim while the Go test supervisor survived, the interrupted validation failed closed, and a new worker returned a correctly correlated result; nested delegated child-cgroup authority was unavailable"},
			{Model: "B — whole-unit recovery", Status: "PARTIALLY VERIFIED", Reason: "FG-C3.7 demonstrated shared-unit memory/task controls and process-tree cleanup; external restart of the whole unit after unit-level OOM was not executed in this milestone"},
		},
	}
	encoded, err := json.Marshal(evidence)
	if err != nil {
		t.Fatal(err)
	}
	fmt.Printf("FG_C3_7_EVIDENCE %s\n", encoded)
}

func TestFGC371ConcurrentCallersRecoverAfterWorkerFailure(t *testing.T) {
	path := rustPath(t)
	planner := NewPlanner(path)
	defer planner.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	capabilities := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"fg371-concurrent-ready","kind":"capabilities"}`)
	if _, err := planner.Call(ctx, capabilities); err != nil {
		t.Fatal(err)
	}
	deadPID := planner.PID()
	process, err := os.FindProcess(deadPID)
	if err != nil {
		t.Fatal(err)
	}
	if err := process.Kill(); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(2 * time.Second)
	for processExists(deadPID) && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if processExists(deadPID) {
		t.Fatalf("failed worker PID %d did not terminate", deadPID)
	}
	type outcome struct {
		id  string
		err error
	}
	const callers = 24
	start := make(chan struct{})
	results := make(chan outcome, callers)
	var wg sync.WaitGroup
	request := fixture(t, "reorder.json")
	for i := range callers {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			id := fmt.Sprintf("fg371-concurrent-%02d", i)
			<-start
			raw, err := planner.Call(ctx, requestWithID(t, request, id))
			if err == nil {
				var got response
				err = json.Unmarshal(raw, &got)
				if err == nil && (!got.OK || got.RequestID != id) {
					err = fmt.Errorf("response mismatch: id=%q ok=%t", got.RequestID, got.OK)
				}
			}
			results <- outcome{id: id, err: err}
		}(i)
	}
	close(start)
	wg.Wait()
	close(results)
	failed, recovered := 0, 0
	for result := range results {
		if result.err != nil {
			failed++
		} else {
			recovered++
		}
	}
	if failed == 0 || recovered == 0 || failed+recovered != callers {
		t.Fatalf("concurrent worker-failure outcomes were not fail-closed and recoverable: failures=%d recovered=%d", failed, recovered)
	}
}

func qualifyPlannerSpecificOOM(t *testing.T, rustPath string) plannerOOMEvidence {
	t.Helper()
	if !strings.Contains(rustPath, "fg-c3-2-1-rust-planner-spike") {
		t.Fatalf("unexpected planner binary for test-only OOM qualification: %q", rustPath)
	}
	dir := t.TempDir()
	unitFile := filepath.Join(dir, "unit-name")
	evidenceFile := filepath.Join(dir, "planner-evidence")
	triggerFile := filepath.Join(dir, "oom-trigger")
	wrapper := filepath.Join(dir, "planner-scope.sh")
	wrapperText := `#!/bin/sh
set -eu
unit="logos-fg-c371-planner-$PPID-$$"
printf '%s\n' "$unit" > "$FG_C3_7_UNIT_FILE"
exec systemd-run --user --scope --quiet --expand-environment=no --unit="$unit" \
  --property=MemoryMax=64M --property=MemorySwapMax=0 --property=CPUQuota=50% \
  --property=TasksMax=16 -- /bin/sh -c 'sleep 15 >/dev/null 2>&1 & exec "$FG_C3_7_RUST_PLANNER"'
`
	if err := os.WriteFile(wrapper, []byte(wrapperText), 0o700); err != nil {
		t.Fatal(err)
	}
	planner := NewPlanner(wrapper)
	planner.Env = []string{
		"FG_C3_7_UNIT_FILE=" + unitFile,
		"FG_C3_7_RUST_PLANNER=" + rustPath,
		"FG_C3_7_TEST_EVIDENCE_FILE=" + evidenceFile,
		"FG_C3_7_TEST_OOM_TRIGGER_FILE=" + triggerFile,
		"FG_C3_7_TEST_OOM_BYTES=268435456",
	}
	var units []string
	stopUnits := func() {
		for _, unit := range units {
			ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
			_ = exec.CommandContext(ctx, "systemctl", "--user", "stop", unit+".scope").Run()
			_ = exec.CommandContext(ctx, "systemctl", "--user", "reset-failed", unit+".scope").Run()
			cancel()
		}
	}
	defer stopUnits()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	capabilityRequest := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"fg371-scope-ready","kind":"capabilities"}`)
	capabilityRaw, err := planner.Call(ctx, capabilityRequest)
	if err != nil {
		t.Fatalf("isolated Rust planner failed startup handshake: %v", err)
	}
	var capability response
	if err := json.Unmarshal(capabilityRaw, &capability); err != nil || !capability.OK {
		t.Fatalf("isolated planner returned invalid capabilities: %s err=%v", capabilityRaw, err)
	}
	identity := readKeyValueFile(t, evidenceFile)
	unit := strings.TrimSpace(readFile(t, unitFile))
	units = append(units, unit)
	rustPID, err := strconv.Atoi(identity["pid"])
	if err != nil || rustPID <= 0 {
		t.Fatalf("Rust planner PID evidence invalid: %v %v", identity, err)
	}
	cgroup := strings.TrimPrefix(identity["cgroup"], "/")
	if cgroup == "" || strings.Contains(cgroup, "..") || !strings.Contains(cgroup, unit+".scope") {
		t.Fatalf("Rust planner is not in its dedicated systemd scope: unit=%s cgroup=%s", unit, identity["cgroup"])
	}
	cgroupDir := filepath.Join("/sys/fs/cgroup", cgroup)
	memoryMax := strings.TrimSpace(readFile(t, filepath.Join(cgroupDir, "memory.max")))
	if memoryMax != "67108864" {
		t.Fatalf("planner scope memory.max=%q, want 67108864", memoryMax)
	}
	if got := strings.TrimSpace(readFile(t, filepath.Join(cgroupDir, "pids.max"))); got != "16" {
		t.Fatalf("planner scope pids.max=%q, want 16", got)
	}
	if !fileContainsPID(t, filepath.Join(cgroupDir, "cgroup.procs"), rustPID) {
		t.Fatalf("Rust planner PID %d is not a member of its recorded scope", rustPID)
	}
	goCgroup := currentCgroupPath(t)
	if strings.TrimPrefix(goCgroup, "/") == identity["cgroup"] || !processExists(os.Getpid()) {
		t.Fatalf("Go supervisor is not alive outside the planner scope: go=%s rust=%s", goCgroup, identity["cgroup"])
	}
	oomBefore, err := readCgroupCounter(cgroup, "memory.events", "oom_kill")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(triggerFile, []byte("test-only out-of-band trigger"), 0o600); err != nil {
		t.Fatal(err)
	}
	validation := componentRequest("fg371-oom-interrupted", 1, `{"version":1,"components":{},"componentSets":{},"instances":{}}`)
	interrupted, oomErr := planner.Call(ctx, validation)
	if oomErr == nil || interrupted != nil {
		t.Fatalf("OOM-interrupted validation was accepted: response=%s err=%v", interrupted, oomErr)
	}
	if planner.PID() != 0 {
		t.Fatalf("failed scoped worker was not reaped by the coordinator: pid=%d", planner.PID())
	}
	deadline := time.Now().Add(2 * time.Second)
	for processExists(rustPID) && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if processExists(rustPID) {
		t.Fatalf("OOM-killed Rust process %d still exists", rustPID)
	}
	oomAfter, err := readCgroupCounter(cgroup, "memory.events", "oom_kill")
	if err != nil || oomAfter <= oomBefore {
		t.Fatalf("planner scope did not record its OOM kill: before=%d after=%d err=%v", oomBefore, oomAfter, err)
	}
	memoryPeak := strings.TrimSpace(readFile(t, filepath.Join(cgroupDir, "memory.peak")))
	oomGroup := strings.TrimSpace(readFile(t, filepath.Join(cgroupDir, "memory.oom.group")))
	if oomGroup != "0" {
		t.Fatalf("planner scope unexpectedly enabled group OOM: %q", oomGroup)
	}
	lastRSS := readKeyValueFile(t, evidenceFile)["rss_kib"]
	if !fileContainsOtherPID(t, filepath.Join(cgroupDir, "cgroup.procs"), rustPID) {
		t.Fatalf("planner scope keeper disappeared before OOM evidence collection: %s", cgroupDir)
	}
	unitProperties, err := exec.Command("systemctl", "--user", "show", unit+".scope", "-p", "Result", "-p", "ExecMainCode", "-p", "ExecMainStatus", "-p", "OOMKills").Output()
	if err != nil {
		t.Fatalf("failed to read planner scope exit evidence: %v", err)
	}
	unitEvidence := parseKeyValue(string(unitProperties))
	if unitEvidence["Result"] != "oom-kill" || unitEvidence["OOMKills"] != "1" {
		t.Fatalf("systemd did not confirm the planner scope OOM: %+v", unitEvidence)
	}
	goSurvived := processExists(os.Getpid()) && planner.PID() == 0
	if !goSurvived {
		t.Fatal("Go supervisor did not survive planner-specific OOM")
	}
	stopUnits()
	units = nil
	if err := os.Remove(triggerFile); err != nil && !os.IsNotExist(err) {
		t.Fatal(err)
	}
	var validationResult componentResult
	responseBytes, err := planner.Call(ctx, validation)
	if err != nil {
		t.Fatalf("correlated post-restart validation failed: %v", err)
	}
	restartedIdentity := readKeyValueFile(t, evidenceFile)
	restartedPID, err := strconv.Atoi(restartedIdentity["pid"])
	if err != nil || restartedPID <= 0 || restartedPID == rustPID {
		t.Fatalf("restart did not create a new Rust worker PID: old=%d evidence=%v err=%v", rustPID, restartedIdentity, err)
	}
	restartedUnit := strings.TrimSpace(readFile(t, unitFile))
	units = append(units, restartedUnit)
	if err := json.Unmarshal(responseBytes, &validationResult); err != nil || !validationResult.OK || validationResult.RequestID != "fg371-oom-interrupted" {
		t.Fatalf("post-restart response lost validation correlation: %s err=%v", responseBytes, err)
	}
	planner.Close()
	return plannerOOMEvidence{
		Status:     "VERIFIED LOCALLY — Rust planner was OOM-killed in its dedicated sibling scope; Go supervisor survived",
		PlannerPID: rustPID, PlannerScope: identity["cgroup"], PlannerRSSKiB: lastRSS,
		SupervisorPID: os.Getpid(), SupervisorSurvived: goSurvived,
		MemoryMax: memoryMax, MemoryPeak: memoryPeak, OOMKillBefore: oomBefore, OOMKillAfter: oomAfter, OOMGroup: oomGroup,
		ScopeResult: unitEvidence["Result"], ScopeExecMainCode: unitEvidence["ExecMainCode"],
		ScopeExecMainStatus: unitEvidence["ExecMainStatus"], ScopeOOMKills: unitEvidence["OOMKills"],
		InterruptedRequest: "FAIL — Go returned nil response plus transport error; no validation success accepted",
		RestartRequestID:   validationResult.RequestID, RestartWorkerPID: restartedPID,
		OrphanCheck: "OOM planner PID absent, scope keeper remained until evidence captured, then unit stopped; restarted worker closed and its scope stopped",
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(data)
}

func readKeyValueFile(t *testing.T, path string) map[string]string {
	t.Helper()
	result := map[string]string{}
	for _, line := range strings.Split(readFile(t, path), "\n") {
		parts := strings.SplitN(line, "=", 2)
		if len(parts) == 2 {
			result[parts[0]] = parts[1]
		}
	}
	return result
}

func parseKeyValue(raw string) map[string]string {
	result := map[string]string{}
	for _, line := range strings.Split(raw, "\n") {
		parts := strings.SplitN(line, "=", 2)
		if len(parts) == 2 {
			result[parts[0]] = parts[1]
		}
	}
	return result
}

func currentCgroupPath(t *testing.T) string {
	t.Helper()
	data, err := os.ReadFile("/proc/self/cgroup")
	if err != nil {
		t.Fatal(err)
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "0::") {
			return strings.TrimPrefix(line, "0::")
		}
	}
	t.Fatal("current process has no unified cgroup path")
	return ""
}

func fileContainsPID(t *testing.T, path string, pid int) bool {
	t.Helper()
	for _, line := range strings.Fields(readFile(t, path)) {
		value, err := strconv.Atoi(line)
		if err == nil && value == pid {
			return true
		}
	}
	return false
}

func fileContainsOtherPID(t *testing.T, path string, pid int) bool {
	t.Helper()
	for _, line := range strings.Fields(readFile(t, path)) {
		value, err := strconv.Atoi(line)
		if err == nil && value != pid {
			return true
		}
	}
	return false
}

func processExists(pid int) bool {
	_, err := os.Stat(fmt.Sprintf("/proc/%d", pid))
	return err == nil
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
	request = componentRequest("fg37-supervisor-valid", 1, `{"version":1,"components":{},"componentSets":{},"instances":{}}`)
	supervisor := qualificationSupervisor{Path: missing, MaxAttempts: 4, InitialBackoff: 25 * time.Millisecond, MaxBackoff: 100 * time.Millisecond}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	_, err := supervisor.Call(ctx, requestWithID(t, request, "fg37-backoff-fail"))
	cancel()
	if err == nil || !strings.Contains(err.Error(), "restart attempts exhausted") || supervisor.Attempts != 4 {
		t.Fatalf("repeated startup failures were not bounded/fail-closed: attempts=%d err=%v", supervisor.Attempts, err)
	}
	if len(supervisor.BackoffDelays) != 3 || supervisor.BackoffDelays[0] != 25*time.Millisecond || supervisor.BackoffDelays[1] != 50*time.Millisecond || supervisor.BackoffDelays[2] != 100*time.Millisecond {
		t.Fatalf("backoff did not grow exponentially and cap at 100ms: %v", supervisor.BackoffDelays)
	}
	blockedAttempts := supervisor.Attempts
	if _, err := supervisor.Call(context.Background(), fixture(t, "reorder.json")); err == nil || supervisor.Attempts != blockedAttempts {
		t.Fatalf("supervisor allowed replay of a non-validation operation: attempts=%d err=%v", supervisor.Attempts, err)
	}
	canceling := qualificationSupervisor{Path: missing, MaxAttempts: 4, InitialBackoff: 150 * time.Millisecond, MaxBackoff: 150 * time.Millisecond}
	cancelCtx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	_, cancelErr := canceling.Call(cancelCtx, request)
	cancel()
	if !errors.Is(cancelErr, context.DeadlineExceeded) || canceling.Attempts != 1 {
		t.Fatalf("backoff wait ignored cancellation or started an extra worker: attempts=%d err=%v", canceling.Attempts, cancelErr)
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
	semanticRaw, err := supervisor.Call(ctx, componentRequest("fg37-semantic-reject", 1, "{"))
	if err != nil {
		t.Fatalf("semantic rejection was incorrectly escalated to a process failure: %v", err)
	}
	if err := json.Unmarshal(semanticRaw, &decoded); err != nil || decoded.OK || decoded.RequestID != "fg37-semantic-reject" || supervisor.Attempts != attemptsBeforeSemanticReject+1 {
		t.Fatalf("semantic rejection triggered a retry or lost correlation: response=%s err=%v attempts=%d", semanticRaw, err, supervisor.Attempts)
	}
	return "test-only supervisor made 4 bounded validation-only startup attempts with exponential 25/50/100ms backoff capped at 100ms; exhaustion and cancellation were explicit; non-validation operations were not replayed; semantic rejection returned once without retry; policy is not in production Planner"
}

// qualificationSupervisor is isolated to this test file and deliberately not
// used by the coordinator or any production command.
type qualificationSupervisor struct {
	Path           string
	MaxAttempts    int
	InitialBackoff time.Duration
	MaxBackoff     time.Duration
	Attempts       int
	BackoffDelays  []time.Duration
}

func (s *qualificationSupervisor) Call(ctx context.Context, request []byte) ([]byte, error) {
	if requestKind(request) != "validateComponentInstanceV1" {
		return nil, errors.New("qualification supervisor refuses replay outside validation-only requests")
	}
	if s.MaxAttempts < 1 || s.InitialBackoff < 0 || s.MaxBackoff < s.InitialBackoff {
		return nil, errors.New("qualification supervisor retry bounds are invalid")
	}
	backoff := s.InitialBackoff
	var lastErr error
	for attempt := 0; attempt < s.MaxAttempts; attempt++ {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if attempt > 0 {
			s.BackoffDelays = append(s.BackoffDelays, backoff)
			timer := time.NewTimer(backoff)
			select {
			case <-ctx.Done():
				timer.Stop()
				return nil, ctx.Err()
			case <-timer.C:
			}
			backoff *= 2
			if backoff > s.MaxBackoff {
				backoff = s.MaxBackoff
			}
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
	return nil, fmt.Errorf("planner restart attempts exhausted after %d attempts: %w", s.MaxAttempts, lastErr)
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
