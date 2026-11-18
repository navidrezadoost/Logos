package main

import (
	"context"
	"encoding/json"
	"os"
	"testing"
	"time"
)

func validCaps(id string) map[string]any {
	return map[string]any{"protocolVersion": 2, "semanticVersion": 1, "diagnosticVersion": 1, "requestId": id, "ok": true,
		"limits":       map[string]any{"maxRequestBytes": MaxRequestBytes, "maxResponseBytes": MaxResponseBytes, "framing": "uint32-be-length-prefixed"},
		"capabilities": map[string]any{"documentOperations": map[string]any{"version": 1, "operations": []string{"applyOperation", "validateDocument"}}, "projectionIdentity": map[string]any{"version": 1, "operations": []string{"deriveIds", "checkDetachIds"}}, "componentInstanceDomain": map[string]any{"status": "not-supported-here", "owner": "TypeScript componentInstanceDomainV1"}, "responseContract": map[string]any{"version": 1, "correlation": []string{"requestId", "operationKind"}}, "diagnostics": map[string]any{"version": 1}}}
}

func TestStrictResponseEnvelopeRejectsAdversarialResponses(t *testing.T) {
	request := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","kind":"capabilities"}`)
	base, _ := json.Marshal(validCaps("x"))
	if err := validateResponse(request, base); err != nil {
		t.Fatalf("valid capabilities rejected: %v", err)
	}
	mutations := []func(map[string]any){
		func(m map[string]any) { m["protocolVersion"] = 9 },
		func(m map[string]any) { delete(m, "ok") },
		func(m map[string]any) { m["requestId"] = "stale" },
		func(m map[string]any) { m["unknownProtocolField"] = true },
		func(m map[string]any) { m["capabilities"].(map[string]any)["unexpected"] = true },
		func(m map[string]any) {
			m["capabilities"].(map[string]any)["componentInstanceDomain"].(map[string]any)["status"] = "supported"
		},
		func(m map[string]any) { m["ok"] = false },
	}
	for i, mutate := range mutations {
		candidate, _ := json.Marshal(validCaps("x"))
		var obj map[string]any
		_ = json.Unmarshal(candidate, &obj)
		mutate(obj)
		candidate, _ = json.Marshal(obj)
		if err := validateResponse(request, candidate); err == nil {
			t.Errorf("mutation %d passed validation", i)
		}
	}
}

func TestStrictDocumentResponseRequiresRequestAndOperationIdentity(t *testing.T) {
	request := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","kind":"evaluate","document":{"id":"doc-1"},"operation":{"kind":"updateShape"}}`)
	good := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","ok":true,"phase":"evaluated","operationKind":"updateShape","document":{"id":"doc-1","schema-version":1,"pages":[],"shapes":[]},"diagnostics":[]}`)
	if err := validateResponse(request, good); err != nil {
		t.Fatalf("valid document result rejected: %v", err)
	}
	for name, bad := range map[string]string{
		"wrong document":  `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","ok":true,"phase":"evaluated","operationKind":"updateShape","document":{"id":"other"},"diagnostics":[]}`,
		"wrong operation": `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","ok":true,"phase":"evaluated","operationKind":"deleteSubtree","document":{"id":"doc-1"},"diagnostics":[]}`,
		"unknown phase":   `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","ok":false,"phase":"half-done","diagnostics":[]}`,
		"missing ok":      `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","phase":"evaluated","operationKind":"updateShape","document":{"id":"doc-1"},"diagnostics":[]}`,
		"bad diagnostic":  `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","ok":false,"phase":"operation-rejected","diagnostics":[{"code":"bad code","path":""}]}`,
		"contradiction":   `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","ok":true,"phase":"evaluated","operationKind":"updateShape","document":{"id":"doc-1"},"diagnostics":[],"error":{"code":"X","path":"/","message":"x"}}`,
	} {
		if err := validateResponse(request, []byte(bad)); err == nil {
			t.Errorf("%s passed validation", name)
		}
	}
}

func TestPlannerStartupCapabilityHandshake(t *testing.T) {
	for _, mode := range []string{"valid", "bad-version", "corrupt-capabilities", "partial-exit", "oversized-frame"} {
		t.Run(mode, func(t *testing.T) {
			p := NewPlanner(os.Args[0])
			p.Args = []string{"-test.run=TestPlannerHelperProcess"}
			p.Env = []string{"FG_TEST_PLANNER_HELPER=1", "FG_TEST_PLANNER_MODE=" + mode}
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			_, err := p.Call(ctx, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"x","kind":"capabilities"}`))
			defer p.Close()
			if mode == "valid" && err != nil {
				t.Fatalf("valid handshake failed: %v", err)
			}
			if mode != "valid" && err == nil {
				t.Fatal("incompatible startup handshake became ready")
			}
			if mode != "valid" && p.PID() != 0 {
				t.Fatal("failed handshake left child process alive")
			}
		})
	}
}

func TestPlannerHelperProcess(t *testing.T) {
	if os.Getenv("FG_TEST_PLANNER_HELPER") != "1" {
		return
	}
	mode := os.Getenv("FG_TEST_PLANNER_MODE")
	for {
		body, err := readFrame(os.Stdin, MaxRequestBytes)
		if err != nil {
			return
		}
		var req map[string]json.RawMessage
		_ = json.Unmarshal(body, &req)
		if mode == "partial-exit" {
			_, _ = os.Stdout.Write([]byte{0, 0, 0, 64, '{', '"', 'o'})
			return
		}
		if mode == "oversized-frame" {
			_, _ = os.Stdout.Write([]byte{0, 0x10, 0, 1})
			return
		}
		id := stringField(req["requestId"])
		response := validCaps("invalid")
		if id != nil {
			response["requestId"] = *id
		}
		if mode == "bad-version" {
			response["protocolVersion"] = 99
		}
		if mode == "corrupt-capabilities" {
			response["capabilities"].(map[string]any)["documentOperations"].(map[string]any)["operations"] = []string{"validateDocument"}
		}
		encoded, _ := json.Marshal(response)
		if writeFrame(os.Stdout, encoded) != nil {
			return
		}
	}
}

func TestOneShotOutputUsesBoundedCapture(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	script := t.TempDir() + "/noisy"
	if err := os.WriteFile(script, []byte("#!/bin/sh\nhead -c 1100000 /dev/zero\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	if _, err := CallOneShot(ctx, script, []byte(`{}`)); err == nil {
		t.Fatal("unbounded one-shot output was accepted")
	}
}
