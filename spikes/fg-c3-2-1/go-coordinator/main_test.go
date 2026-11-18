package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

type response struct {
	ProtocolVersion   int               `json:"protocolVersion"`
	SemanticVersion   int               `json:"semanticVersion"`
	DiagnosticVersion int               `json:"diagnosticVersion"`
	RequestID         string            `json:"requestId"`
	OK                bool              `json:"ok"`
	Phase             string            `json:"phase"`
	Document          json.RawMessage   `json:"document"`
	Diagnostics       []json.RawMessage `json:"diagnostics"`
	IDs               []string          `json:"ids"`
	Error             struct {
		Code string `json:"code"`
	} `json:"error"`
	Capabilities map[string]any `json:"capabilities"`
}

func rustPath(t *testing.T) string {
	t.Helper()
	path := os.Getenv("FG_C3_2_2_RUST_PLANNER")
	if path == "" {
		t.Fatal("FG_C3_2_2_RUST_PLANNER is required")
	}
	return path
}

func fixture(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "fixtures", name))
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func invoke(t *testing.T, planner *Planner, raw []byte) (response, []byte) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	output, err := planner.Call(ctx, raw)
	if err != nil {
		t.Fatal(err)
	}
	var got response
	if err := json.Unmarshal(output, &got); err != nil {
		t.Fatalf("invalid response: %v: %s", err, output)
	}
	if got.ProtocolVersion != 2 {
		t.Fatalf("protocol version = %d", got.ProtocolVersion)
	}
	if got.SemanticVersion != 1 || got.DiagnosticVersion != 1 {
		t.Fatalf("response versions semantic=%d diagnostic=%d", got.SemanticVersion, got.DiagnosticVersion)
	}
	return got, output
}

func TestCapabilitiesSeparateComponentDomainOwnership(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	got, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"capabilities","kind":"capabilities"}`))
	if !got.OK || got.Capabilities["componentInstanceDomain"] == nil {
		t.Fatalf("capability report missing: %+v", got)
	}
	component := got.Capabilities["componentInstanceDomain"].(map[string]any)
	if component["status"] != "not-supported-here" || component["owner"] != "TypeScript componentInstanceDomainV1" {
		t.Fatalf("component domain capability is ambiguous: %v", component)
	}
}

func TestPersistentSequentialRequestsAndProtocolConformance(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	first, _ := invoke(t, p, fixture(t, "reorder.json"))
	pid := p.PID()
	if !first.OK || pid == 0 {
		t.Fatalf("valid reorder rejected or process not started: %+v", first)
	}
	for _, name := range []string{"cycle-reject.json", "detach-ids.json", "detach-collision.json", "reorder.json"} {
		invoke(t, p, fixture(t, name))
	}
	if p.PID() != pid {
		t.Fatalf("planner process changed across sequential calls: initial=%d current=%d", pid, p.PID())
	}
	cycleA, rawA := invoke(t, p, fixture(t, "cycle-reject.json"))
	_, rawB := invoke(t, p, fixture(t, "cycle-reject.json"))
	if cycleA.OK || cycleA.Phase != "operation-rejected" || len(cycleA.Diagnostics) == 0 || !bytes.Equal(rawA, rawB) {
		t.Fatalf("cycle response not stable: %s %s", rawA, rawB)
	}
	var firstDiagnostic map[string]any
	if err := json.Unmarshal(cycleA.Diagnostics[0], &firstDiagnostic); err != nil || firstDiagnostic["code"] != "CYCLE" {
		t.Fatalf("cycle diagnostic = %v, err=%v", firstDiagnostic, err)
	}
}

func TestProjectionIdentityFixtureRelations(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	var cases []struct {
		Name     string          `json:"name"`
		Request  json.RawMessage `json:"request"`
		Relation string          `json:"relation"`
	}
	if err := json.Unmarshal(fixture(t, "projection-identities.json"), &cases); err != nil {
		t.Fatal(err)
	}
	for _, tc := range cases {
		t.Run(tc.Name, func(t *testing.T) {
			got, _ := invoke(t, p, tc.Request)
			if !got.OK || len(got.IDs) != 2 {
				t.Fatalf("bad identity response: %+v", got)
			}
			equal := got.IDs[0] == got.IDs[1]
			if (tc.Relation == "same") != equal {
				t.Fatalf("IDs %v relation=%s", got.IDs, tc.Relation)
			}
		})
	}
}

func TestConcurrentCallersReceiveCorrelatedResponses(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	request := fixture(t, "detach-ids.json")
	var wg sync.WaitGroup
	errors := make(chan string, 32)
	for i := 0; i < 32; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			var envelope map[string]any
			if err := json.Unmarshal(request, &envelope); err != nil {
				errors <- err.Error()
				return
			}
			id := "parallel-" + strconv.Itoa(i)
			envelope["requestId"] = id
			raw, _ := json.Marshal(envelope)
			got, _ := invoke(t, p, raw)
			if got.RequestID != id || !got.OK {
				errors <- "request correlation mismatch: " + got.RequestID + " expected " + id
			}
		}(i)
	}
	wg.Wait()
	close(errors)
	for err := range errors {
		t.Error(err)
	}
}

func TestPlannerCrashRestartPreservesDeterministicDiagnostics(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	request := fixture(t, "cycle-reject.json")
	_, before := invoke(t, p, request)
	firstPID := p.PID()
	crasher := filepath.Join(t.TempDir(), "crash-planner")
	if err := os.WriteFile(crasher, []byte("#!/bin/sh\nkill -9 $$\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	p.stop()
	p.Path = crasher
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	_, err := p.Call(ctx, request) // terminate during an in-flight framed request
	cancel()
	if err == nil {
		t.Fatal("crashed planner returned a successful response")
	}
	p.Path = rustPath(t)
	_, after := invoke(t, p, request)
	if firstPID == 0 || p.PID() == 0 || p.PID() == firstPID || !bytes.Equal(before, after) {
		t.Fatalf("restart was not isolated/stable: old=%d new=%d", firstPID, p.PID())
	}
}

func TestUnavailablePlannerCanRecoverAfterPathCorrection(t *testing.T) {
	p := NewPlanner(filepath.Join(t.TempDir(), "missing-planner"))
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	_, err := p.Call(ctx, fixture(t, "reorder.json"))
	cancel()
	if err == nil {
		t.Fatal("missing planner unexpectedly started")
	}
	p.Path = "/bin/false"
	ctx, cancel = context.WithTimeout(context.Background(), time.Second)
	_, err = p.Call(ctx, fixture(t, "reorder.json"))
	cancel()
	if err == nil {
		t.Fatal("planner that exits before a response unexpectedly succeeded")
	}
	p.Path = rustPath(t)
	got, _ := invoke(t, p, fixture(t, "reorder.json"))
	if !got.OK {
		t.Fatalf("planner did not recover after path correction: %+v", got)
	}
}

func TestTimeoutCancelsPlannerAndNextCallRestarts(t *testing.T) {
	script := filepath.Join(t.TempDir(), "slow-planner")
	if err := os.WriteFile(script, []byte("#!/bin/sh\nexec sleep 5\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	p := NewPlanner(script)
	request := fixture(t, "reorder.json")
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	_, err := p.Call(ctx, request)
	cancel()
	if err == nil {
		t.Fatal("hung planner was not cancelled")
	}
	if p.PID() != 0 {
		t.Fatal("timed out planner process remains active")
	}
	p.Path = rustPath(t)
	got, _ := invoke(t, p, request)
	if !got.OK || p.PID() == 0 {
		t.Fatalf("planner did not restart after cancellation: %+v", got)
	}
	p.Close()
}

func TestOversizedRequestsAndResponsesFailClosed(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	oversized := bytes.Repeat([]byte("x"), MaxRequestBytes+1)
	got, _ := invoke(t, p, oversized)
	if got.Error.Code != "REQUEST_TOO_LARGE" || p.PID() != 0 {
		t.Fatalf("oversized request behavior: %+v pid=%d", got, p.PID())
	}
	p.Env = []string{"FG_C3_2_2_TEST_RESPONSE_LIMIT=256"}
	capabilities := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"large-response","kind":"capabilities"}`)
	large, _ := invoke(t, p, capabilities)
	if large.Error.Code != "RESPONSE_TOO_LARGE" {
		t.Fatalf("oversized planner response was not rejected: %+v", large)
	}
	small, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"after-limit","kind":"deriveIds","addresses":[]}`))
	if !small.OK || small.RequestID != "after-limit" {
		t.Fatalf("response limit corrupted the next frame: %+v", small)
	}
}

func TestMalformedAndUnsupportedFramesDoNotCorruptFollowingRequest(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	malformed, _ := invoke(t, p, []byte(`{broken`))
	if malformed.Error.Code != "REQUEST_INVALID" {
		t.Fatalf("malformed JSON response: %+v", malformed)
	}
	invalidCorrelation, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":7,"kind":"capabilities"}`))
	if invalidCorrelation.Error.Code != "REQUEST_INVALID" {
		t.Fatalf("malformed correlation envelope response: %+v", invalidCorrelation)
	}
	emptyCorrelation, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"","kind":"capabilities"}`))
	if emptyCorrelation.Error.Code != "REQUEST_ID_INVALID" {
		t.Fatalf("empty correlation ID response: %+v", emptyCorrelation)
	}
	largeIDRequest := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"` + strings.Repeat("x", MaxRequestBytes-160) + `","kind":"capabilities"}`)
	largeID, _ := invoke(t, p, largeIDRequest)
	if largeID.Error.Code != "REQUEST_ID_INVALID" || largeID.RequestID != "" {
		t.Fatalf("oversized correlation ID response was not bounded: %+v", largeID)
	}
	oversizedID, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"`+strings.Repeat("x", 256)+`","kind":"capabilities"}`))
	if oversizedID.Error.Code != "REQUEST_ID_INVALID" || oversizedID.RequestID != "" {
		t.Fatalf("oversized correlation ID response was not safely truncated: %+v", oversizedID)
	}
	unsupported, _ := invoke(t, p, []byte(`{"protocolVersion":9,"semanticVersion":1,"diagnosticVersion":1,"requestId":"bad-version","kind":"capabilities"}`))
	if unsupported.Error.Code != "PROTOCOL_VERSION_UNSUPPORTED" {
		t.Fatalf("unsupported protocol response: %+v", unsupported)
	}
	semantic, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":9,"diagnosticVersion":1,"requestId":"bad-semantic","kind":"capabilities"}`))
	if semantic.Error.Code != "SEMANTIC_VERSION_UNSUPPORTED" {
		t.Fatalf("unsupported semantic response: %+v", semantic)
	}
	diagnostic, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":9,"requestId":"bad-diagnostic","kind":"capabilities"}`))
	if diagnostic.Error.Code != "DIAGNOSTIC_VERSION_UNSUPPORTED" {
		t.Fatalf("unsupported diagnostic response: %+v", diagnostic)
	}
	valid, _ := invoke(t, p, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"valid-after-errors","kind":"capabilities"}`))
	if !valid.OK || valid.RequestID != "valid-after-errors" {
		t.Fatalf("bad frame corrupted next request: %+v", valid)
	}
	utf8Error, _ := invoke(t, p, []byte{0xff, 0xfe, '{'})
	if utf8Error.Error.Code != "REQUEST_INVALID_UTF8" {
		t.Fatalf("invalid UTF-8 response: %+v", utf8Error)
	}
}

func TestOversizedAndPartialFramesReturnStructuredErrors(t *testing.T) {
	path := rustPath(t)
	oversizedCmd := exec.Command(path)
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], MaxRequestBytes+1)
	oversizedCmd.Stdin = bytes.NewReader(header[:])
	var oversizedOutput bytes.Buffer
	oversizedCmd.Stdout = &oversizedOutput
	if err := oversizedCmd.Run(); err != nil {
		t.Fatal(err)
	}
	oversizedResponse, err := readRawFrame(&oversizedOutput, MaxResponseBytes)
	if err != nil {
		t.Fatal(err)
	}
	var tooLarge response
	if err := json.Unmarshal(oversizedResponse, &tooLarge); err != nil || tooLarge.Error.Code != "REQUEST_TOO_LARGE" {
		t.Fatalf("oversized frame response: %s err=%v", oversizedResponse, err)
	}

	partialCmd := exec.Command(path)
	partialCmd.Stdin = bytes.NewReader(append([]byte{0, 0, 0, 10}, '{', 'x'))
	var partialOutput bytes.Buffer
	partialCmd.Stdout = &partialOutput
	if err := partialCmd.Run(); err != nil {
		t.Fatal(err)
	}
	partialResponse, err := readRawFrame(&partialOutput, MaxResponseBytes)
	if err != nil {
		t.Fatal(err)
	}
	var truncated response
	if err := json.Unmarshal(partialResponse, &truncated); err != nil || truncated.Error.Code != "FRAME_TRUNCATED" {
		t.Fatalf("partial frame response: %s err=%v", partialResponse, err)
	}

	headerCmd := exec.Command(path)
	headerCmd.Stdin = bytes.NewReader([]byte{0})
	var headerOutput bytes.Buffer
	headerCmd.Stdout = &headerOutput
	if err := headerCmd.Run(); err != nil {
		t.Fatal(err)
	}
	responseBytes, err := readRawFrame(&headerOutput, MaxResponseBytes)
	if err != nil {
		t.Fatal(err)
	}
	var partialHeader response
	if err := json.Unmarshal(responseBytes, &partialHeader); err != nil || partialHeader.Error.Code != "FRAME_TRUNCATED" {
		t.Fatalf("partial header response: %s err=%v", responseBytes, err)
	}
}

func TestOneShotBaselineUsesSamePlannerSemantics(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	request := fixture(t, "reorder.json")
	got, err := CallOneShot(ctx, rustPath(t), request)
	if err != nil {
		t.Fatal(err)
	}
	var actual response
	if err := json.Unmarshal(got, &actual); err != nil || !actual.OK {
		t.Fatalf("one-shot baseline rejected fixture: %+v err=%v", actual, err)
	}
}
