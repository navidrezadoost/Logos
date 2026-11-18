package main

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
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

func TestIdentityResponsesEnforceUUIDAndOperationRules(t *testing.T) {
	derive := func(addresses string, ids string) error {
		request := []byte(`{"requestId":"d","kind":"deriveIds","addresses":` + addresses + `}`)
		return validateResponse(request, []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"d","ok":true,"ids":`+ids+`}`))
	}
	duplicateAddress := `[{"documentId":"11111111-1111-4111-8111-111111111111","instanceId":"22222222-2222-4222-8222-222222222222","definitionRoute":[],"definitionNodeId":"33333333-3333-4333-8333-333333333333","slotRoute":[],"externalRevision":null},{"documentId":"11111111-1111-4111-8111-111111111111","instanceId":"22222222-2222-4222-8222-222222222222","definitionRoute":[],"definitionNodeId":"33333333-3333-4333-8333-333333333333","slotRoute":[],"externalRevision":null}]`
	id := `"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"`
	if err := derive(duplicateAddress, `[`+id+`,`+id+`]`); err != nil {
		t.Fatalf("semantically duplicate address requests should be allowed: %v", err)
	}
	if err := derive(duplicateAddress, `[`+id+`,"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"]`); err == nil {
		t.Fatal("equivalent address returned inconsistent identities")
	}
	if err := derive(duplicateAddress, `["not-a-uuid","not-a-uuid"]`); err == nil {
		t.Fatal("malformed UUIDs were accepted")
	}
	distinct := strings.Replace(duplicateAddress, `"definitionNodeId":"33333333-3333-4333-8333-333333333333","slotRoute"`, `"definitionNodeId":"44444444-4444-4444-8444-444444444444","slotRoute"`, 1)
	if err := derive(distinct, `[`+id+`,`+id+`]`); err == nil {
		t.Fatal("distinct addresses collided")
	}

	request := func(existing, candidates string) []byte {
		return []byte(`{"requestId":"c","kind":"checkDetachIds","existingIds":` + existing + `,"candidateIds":` + candidates + `}`)
	}
	response := func(ids string) []byte {
		return []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"c","ok":true,"ids":` + ids + `}`)
	}
	goodExisting := `["11111111-1111-4111-8111-111111111111"]`
	goodCandidates := `["22222222-2222-4222-8222-222222222222","33333333-3333-4333-8333-333333333333"]`
	if err := validateResponse(request(goodExisting, goodCandidates), response(goodCandidates)); err != nil {
		t.Fatalf("valid detach identity list rejected: %v", err)
	}
	for label, tc := range map[string]struct{ existing, candidates, returned string }{
		"malformed UUID":               {goodExisting, ` ["not-a-uuid"]`, ` ["not-a-uuid"]`},
		"collision with existing":      {goodExisting, ` ["11111111-1111-4111-8111-111111111111"]`, ` ["11111111-1111-4111-8111-111111111111"]`},
		"duplicate detach identities":  {goodExisting, ` ["22222222-2222-4222-8222-222222222222","22222222-2222-4222-8222-222222222222"]`, ` ["22222222-2222-4222-8222-222222222222","22222222-2222-4222-8222-222222222222"]`},
		"planner substituted identity": {goodExisting, ` ["22222222-2222-4222-8222-222222222222"]`, ` ["33333333-3333-4333-8333-333333333333"]`},
	} {
		if err := validateResponse(request(tc.existing, tc.candidates), response(tc.returned)); err == nil {
			t.Errorf("%s passed validation", label)
		}
	}
}

func TestEvaluationDiagnosticsMustMatchStatus(t *testing.T) {
	request := []byte(`{"requestId":"e","kind":"evaluate","document":{"id":"doc"},"operation":{"kind":"updateShape"}}`)
	base := `{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"e","ok":true,"phase":"evaluated","operationKind":"updateShape","document":{"id":"doc","schema-version":1,"pages":[],"shapes":[]},"diagnostics":[]}`
	if err := validateResponse(request, []byte(base)); err != nil {
		t.Fatalf("empty success diagnostics rejected: %v", err)
	}
	withDiagnostic := strings.Replace(base, `"diagnostics":[]`, `"diagnostics":[{"code":"CYCLE","path":"/shapes"}]`, 1)
	if err := validateResponse(request, []byte(withDiagnostic)); err == nil {
		t.Fatal("successful result with error diagnostic passed")
	}
	noDiagnostic := strings.Replace(base, `"ok":true`, `"ok":false`, 1)
	noDiagnostic = strings.Replace(noDiagnostic, `"phase":"evaluated"`, `"phase":"operation-rejected"`, 1)
	if err := validateResponse(request, []byte(noDiagnostic)); err == nil {
		t.Fatal("rejected result without diagnostics passed")
	}
	contradictory := strings.Replace(noDiagnostic, `"diagnostics":[]`, `"diagnostics":[{"code":"CYCLE","path":"/"}],"document":{"id":"doc"}`, 1)
	if err := validateResponse(request, []byte(contradictory)); err == nil {
		t.Fatal("rejection containing success-only document passed")
	}
	contradictoryError := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"e","ok":false,"error":{"code":"OPERATION_INVALID","path":"/operation","message":"invalid"},"diagnostics":[{"code":"CYCLE","path":"/shapes"}]}`)
	if err := validateResponse(request, contradictoryError); err == nil {
		t.Fatal("protocol error inconsistent with diagnostic passed")
	}
}

func TestInvalidSemanticResponsesCloseSessionAndRenegotiate(t *testing.T) {
	for _, mode := range []string{"identity-restart", "evaluation-restart"} {
		t.Run(mode, func(t *testing.T) {
			counter := filepath.Join(t.TempDir(), "starts")
			p := NewPlanner(os.Args[0])
			p.Args = []string{"-test.run=TestPlannerHelperProcess"}
			p.Env = []string{"FG_TEST_PLANNER_HELPER=1", "FG_TEST_PLANNER_MODE=" + mode, "FG_TEST_PLANNER_COUNTER=" + counter}
			defer p.Close()
			request := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"invalid-response","kind":"deriveIds","addresses":[{"documentId":"11111111-1111-4111-8111-111111111111","instanceId":"22222222-2222-4222-8222-222222222222","definitionRoute":[],"definitionNodeId":"33333333-3333-4333-8333-333333333333","slotRoute":[],"externalRevision":null}]}`)
			if mode == "evaluation-restart" {
				request = fixture(t, "reorder.json")
			}
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			_, firstErr := p.Call(ctx, request)
			cancel()
			if firstErr == nil || p.PID() != 0 {
				t.Fatalf("invalid response was exposed or child remained alive: err=%v pid=%d", firstErr, p.PID())
			}
			ctx, cancel = context.WithTimeout(context.Background(), 2*time.Second)
			_, secondErr := p.Call(ctx, request)
			cancel()
			if secondErr != nil || p.PID() == 0 {
				t.Fatalf("valid post-restart response failed: err=%v pid=%d", secondErr, p.PID())
			}
			starts, err := os.ReadFile(counter)
			if err != nil {
				t.Fatal(err)
			}
			count, err := strconv.Atoi(strings.TrimSpace(string(starts)))
			if err != nil || count != 2 {
				t.Fatalf("expected a fresh process and second handshake, starts=%q err=%v", starts, err)
			}
		})
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
	instance := 1
	if counterPath := os.Getenv("FG_TEST_PLANNER_COUNTER"); counterPath != "" {
		current, _ := os.ReadFile(counterPath)
		instance, _ = strconv.Atoi(strings.TrimSpace(string(current)))
		instance++
		if err := os.WriteFile(counterPath, []byte(strconv.Itoa(instance)), 0o600); err != nil {
			return
		}
	}
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
		kind := stringField(req["kind"])
		response := validCaps("invalid")
		if id != nil {
			response["requestId"] = *id
		}
		if kind != nil && *kind == "deriveIds" && (mode == "identity-restart") {
			identity := "not-a-uuid"
			if instance > 1 {
				identity = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"
			}
			response = map[string]any{"protocolVersion": 2, "semanticVersion": 1, "diagnosticVersion": 1, "requestId": id, "ok": true, "ids": []string{identity}}
		}
		if kind != nil && *kind == "evaluate" && (mode == "evaluation-restart") {
			var requestDoc, operation map[string]json.RawMessage
			_ = json.Unmarshal(req["document"], &requestDoc)
			_ = json.Unmarshal(req["operation"], &operation)
			var document any
			_ = json.Unmarshal(req["document"], &document)
			response = map[string]any{"protocolVersion": 2, "semanticVersion": 1, "diagnosticVersion": 1, "requestId": id, "ok": true, "phase": "evaluated", "operationKind": stringField(operation["kind"]), "document": document, "diagnostics": []any{}}
			if instance == 1 {
				response["diagnostics"] = []map[string]string{{"code": "CYCLE", "path": "/shapes"}}
			}
			_ = requestDoc
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
