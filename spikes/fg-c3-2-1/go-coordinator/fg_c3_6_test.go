package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"
)

type componentResult struct {
	ProtocolVersion          int                             `json:"protocolVersion"`
	SemanticVersion          int                             `json:"semanticVersion"`
	DiagnosticVersion        int                             `json:"diagnosticVersion"`
	ComponentSemanticVersion int                             `json:"componentSemanticVersion"`
	RequestID                string                          `json:"requestId"`
	OperationKind            string                          `json:"operationKind"`
	OK                       bool                            `json:"ok"`
	Phase                    string                          `json:"phase"`
	Diagnostics              []componentValidationDiagnostic `json:"diagnostics"`
	Error                    *struct {
		Code string `json:"code"`
	} `json:"error"`
}

func componentRequest(id string, version int, document string) []byte {
	request, _ := json.Marshal(map[string]any{
		"protocolVersion": 2, "semanticVersion": 1, "diagnosticVersion": 1,
		"requestId": id, "kind": "validateComponentInstanceV1",
		"componentSemanticVersion": version, "documentJson": document,
	})
	return request
}

func componentFixture(t *testing.T) map[string]any {
	t.Helper()
	var document map[string]any
	path := filepath.Join("..", "..", "..", "test", "fixtures", "figma-semantics", "component-instance-domain-v1.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, &document); err != nil {
		t.Fatal(err)
	}
	return document
}

func componentFixtureText(t *testing.T, doc map[string]any) string {
	t.Helper()
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}

func expectedFixtureDiagnostics(t *testing.T, scenarioID string) []componentValidationDiagnostic {
	t.Helper()
	path := filepath.Join("..", "..", "..", "test", "fixtures", "figma-semantics", "fg-c3-5-protocol-adversarial-v1.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var contract struct {
		Cases []struct {
			ID       string `json:"id"`
			Expected struct {
				Diagnostics []componentValidationDiagnostic `json:"diagnosticsExact"`
			} `json:"expected"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(data, &contract); err != nil {
		t.Fatal(err)
	}
	for _, scenario := range contract.Cases {
		if scenario.ID == scenarioID {
			return scenario.Expected.Diagnostics
		}
	}
	t.Fatalf("scenario %q is absent from FG-C3.5 fixture", scenarioID)
	return nil
}

func callComponent(t *testing.T, p *Planner, raw []byte) (componentResult, []byte) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	response, err := p.Call(ctx, raw)
	if err != nil {
		t.Fatal(err)
	}
	var result componentResult
	if err := json.Unmarshal(response, &result); err != nil {
		t.Fatalf("invalid component response: %v: %s", err, response)
	}
	return result, response
}

func TestFGC36FramedSemanticScenariosAndRecovery(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	base := componentFixture(t)
	valid := componentFixture(t)
	valid["components"].(map[string]any)["card"].(map[string]any)["properties"].(map[string]any)["content-slot"].(map[string]any)["guidance"].(map[string]any)["maximumEntries"] = float64(2)
	delete(valid["instances"].(map[string]any), "external-instance")
	delete(valid["instances"].(map[string]any), "unresolved-instance")
	validText := componentFixtureText(t, valid)

	clean, _ := callComponent(t, p, componentRequest("fg36-clean", 1, validText))
	if !clean.OK || clean.Phase != "component-validation" || clean.OperationKind != "validateComponentInstanceV1" || clean.ComponentSemanticVersion != 1 || clean.RequestID != "fg36-clean" || len(clean.Diagnostics) != 0 {
		t.Fatalf("clean exact result mismatch: %+v", clean)
	}
	pid := p.PID()

	warning, _ := callComponent(t, p, componentRequest("fg36-warning", 1, componentFixtureText(t, base)))
	if !warning.OK || warning.Phase != "component-validation" || len(warning.Diagnostics) == 0 {
		t.Fatalf("warning-only result rejected: %+v", warning)
	}
	for _, diagnostic := range warning.Diagnostics {
		if diagnostic.Severity != "warning" {
			t.Fatalf("warning-only result includes non-warning: %+v", warning.Diagnostics)
		}
	}
	// Exact codes intentionally include all diagnostics, not just a required subset.
	wantWarningDiagnostics := expectedFixtureDiagnostics(t, "valid-warning-only")
	if !reflect.DeepEqual(warning.Diagnostics, wantWarningDiagnostics) {
		t.Fatalf("warning exact diagnostic array changed:\n got: %#v\nwant: %#v", warning.Diagnostics, wantWarningDiagnostics)
	}

	invalid := componentFixture(t)
	invalid["instances"].(map[string]any)["button-instance"].(map[string]any)["source"].(map[string]any)["componentId"] = "missing-component"
	rejected, _ := callComponent(t, p, componentRequest("fg36-invalid", 1, componentFixtureText(t, invalid)))
	wantRejectedDiagnostics := expectedFixtureDiagnostics(t, "rejected-component-graph")
	if rejected.OK || rejected.Phase != "component-validation" || !reflect.DeepEqual(rejected.Diagnostics, wantRejectedDiagnostics) {
		t.Fatalf("semantic rejection mismatch: %+v", rejected)
	}
	if p.PID() != pid {
		t.Fatalf("semantic rejection replaced worker: before=%d after=%d", pid, p.PID())
	}

	unsupported, _ := callComponent(t, p, componentRequest("fg36-version", 2, validText))
	if unsupported.Error == nil || unsupported.Error.Code != "COMPONENT_SEMANTIC_VERSION_UNSUPPORTED" || unsupported.RequestID != "fg36-version" {
		t.Fatalf("unsupported version result: %+v", unsupported)
	}
	malformed, _ := callComponent(t, p, []byte(`{"protocolVersion":2,"requestId":`))
	if malformed.Error == nil || malformed.Error.Code != "REQUEST_INVALID" {
		t.Fatalf("malformed envelope result: %+v", malformed)
	}
	if got, _ := callComponent(t, p, componentRequest("fg36-after-malformed", 1, validText)); !got.OK {
		t.Fatalf("worker did not continue after request rejection: %+v", got)
	}

	outerDuplicate := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"first","request\u0049d":"second","kind":"capabilities"}`)
	duplicate, _ := callComponent(t, p, outerDuplicate)
	if duplicate.Error == nil || duplicate.Error.Code != "DUPLICATE_JSON_MEMBER" || duplicate.RequestID != "" {
		t.Fatalf("outer duplicate handling: %+v", duplicate)
	}
	innerDuplicate := `{"version":1,"nested":{"property":1,"pro\u0070erty":2},"components":{},"componentSets":{},"instances":{}}`
	duplicate, _ = callComponent(t, p, componentRequest("fg36-inner-duplicate", 1, innerDuplicate))
	if duplicate.Error == nil || duplicate.Error.Code != "DUPLICATE_JSON_MEMBER" || duplicate.RequestID != "fg36-inner-duplicate" {
		t.Fatalf("embedded duplicate handling: %+v", duplicate)
	}

	tooLargeDocument := strings.Repeat(" ", MaxComponentDocumentBytes+1)
	tooLarge, _ := callComponent(t, p, componentRequest("fg36-doc-limit", 1, tooLargeDocument))
	if tooLarge.Error == nil || tooLarge.Error.Code != "COMPONENT_INPUT_TOO_LARGE" {
		t.Fatalf("document boundary +1: %+v", tooLarge)
	}
	exactDocumentBytes, _ := callComponent(t, p, componentRequest("fg36-doc-limit-exact", 1, strings.Repeat(" ", MaxComponentDocumentBytes)))
	if exactDocumentBytes.Error != nil || exactDocumentBytes.OK || len(exactDocumentBytes.Diagnostics) != 1 || exactDocumentBytes.Diagnostics[0].Code != "COMPONENT_JSON_INVALID" {
		t.Fatalf("document exact limit should reach semantic JSON validation: %+v", exactDocumentBytes)
	}

	tooDeep := strings.Repeat("[", maxJSONDepth+1) + "0" + strings.Repeat("]", maxJSONDepth+1)
	deep, _ := callComponent(t, p, componentRequest("fg36-depth-limit", 1, tooDeep))
	if deep.Error == nil || deep.Error.Code != "COMPONENT_RESOURCE_LIMIT" {
		t.Fatalf("depth boundary +1: %+v", deep)
	}
	depthAtLimit := strings.Repeat("[", maxJSONDepth) + "0" + strings.Repeat("]", maxJSONDepth)
	exactDepth, _ := callComponent(t, p, componentRequest("fg36-depth-limit-exact", 1, depthAtLimit))
	if exactDepth.Error != nil || exactDepth.OK || len(exactDepth.Diagnostics) != 1 || exactDepth.Diagnostics[0].Code != "COMPONENT_JSON_INVALID" {
		t.Fatalf("depth exact limit should reach semantic validation: %+v", exactDepth)
	}

	values := "[" + strings.TrimSuffix(strings.Repeat("0,", maxJSONValues), ",") + "]"
	work, _ := callComponent(t, p, componentRequest("fg36-work-limit", 1, values))
	if work.Error == nil || work.Error.Code != "COMPONENT_RESOURCE_LIMIT" {
		t.Fatalf("value-work boundary +1: %+v", work)
	}
	valuesAtLimit := "[" + strings.TrimSuffix(strings.Repeat("0,", maxJSONValues-1), ",") + "]"
	exactWork, _ := callComponent(t, p, componentRequest("fg36-work-limit-exact", 1, valuesAtLimit))
	if exactWork.Error != nil || exactWork.OK || len(exactWork.Diagnostics) != 1 || exactWork.Diagnostics[0].Code != "COMPONENT_JSON_INVALID" {
		t.Fatalf("value-work exact limit should reach semantic JSON validation: %+v", exactWork)
	}

	componentsWithDiagnostics := func(count int) map[string]any {
		many := make(map[string]any, count)
		for i := 0; i < count; i++ {
			many[fmt.Sprintf("bad-%03d", i)] = nil
		}
		return many
	}
	exactDiagnosticsDocument := componentFixtureText(t, map[string]any{"version": 1, "components": componentsWithDiagnostics(512), "componentSets": map[string]any{}, "instances": map[string]any{}})
	exactDiagnostics, _ := callComponent(t, p, componentRequest("fg36-diagnostic-limit-exact", 1, exactDiagnosticsDocument))
	if exactDiagnostics.Error != nil || exactDiagnostics.OK || len(exactDiagnostics.Diagnostics) != 512 {
		t.Fatalf("diagnostic count exact limit should return a complete semantic rejection: error=%+v count=%d", exactDiagnostics.Error, len(exactDiagnostics.Diagnostics))
	}
	manyComponents := componentsWithDiagnostics(513)
	resource := componentFixtureText(t, map[string]any{"version": 1, "components": manyComponents, "componentSets": map[string]any{}, "instances": map[string]any{}})
	tooManyDiagnostics, _ := callComponent(t, p, componentRequest("fg36-diagnostic-limit", 1, resource))
	if tooManyDiagnostics.Error == nil || tooManyDiagnostics.Error.Code != "COMPONENT_DIAGNOSTIC_LIMIT" || p.PID() != pid {
		t.Fatalf("diagnostic overflow should be bounded and recoverable: %+v pid=%d", tooManyDiagnostics, p.PID())
	}
	if after, _ := callComponent(t, p, componentRequest("fg36-after-rejection", 1, validText)); !after.OK || p.PID() != pid {
		t.Fatalf("worker did not recover on same process after semantic/resource rejection: %+v pid=%d", after, p.PID())
	}
}

func TestFGC36FixtureContainsExactlyTwelveQualifiedScenarios(t *testing.T) {
	path := filepath.Join("..", "..", "..", "test", "fixtures", "figma-semantics", "fg-c3-5-protocol-adversarial-v1.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var fixtureContract struct {
		ExecutionStatus string `json:"executionStatus"`
		Cases           []struct {
			ID string `json:"id"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(data, &fixtureContract); err != nil {
		t.Fatal(err)
	}
	want := []string{"valid-clean", "valid-warning-only", "rejected-component-graph", "unsupported-component-semantic-version", "malformed-request-envelope", "duplicate-json-member-policy", "oversized-document-payload", "oversized-request-frame", "excessive-diagnostic-output", "stale-response-correlation", "truncated-input-frame", "restart-after-semantic-rejection"}
	got := make([]string, len(fixtureContract.Cases))
	for i, scenario := range fixtureContract.Cases {
		got[i] = scenario.ID
	}
	if fixtureContract.ExecutionStatus != "executed-by-fg-c3.6-local-tests" || !reflect.DeepEqual(got, want) {
		t.Fatalf("FG-C3.5 fixture execution manifest mismatch: status=%q ids=%v", fixtureContract.ExecutionStatus, got)
	}
}

func TestFGC36SemanticExpansionDepthIsSeparateFromJSONDepth(t *testing.T) {
	makeDocument := func(componentCount int) string {
		components := make(map[string]any, componentCount)
		for i := 0; i < componentCount; i++ {
			id := fmt.Sprintf("component-%02d", i)
			nodeID := fmt.Sprintf("node-%02d", i)
			node := map[string]any{"id": nodeID, "kind": "shape", "parentId": nil, "childIds": []any{}}
			if i < componentCount-1 {
				node["kind"] = "instance"
				node["source"] = map[string]any{"kind": "local-component", "componentId": fmt.Sprintf("component-%02d", i+1)}
				node["variantSelection"] = map[string]any{}
			}
			components[id] = map[string]any{"id": id, "name": id, "setId": nil, "rootNodeIds": []any{nodeID}, "nodes": map[string]any{nodeID: node}, "properties": map[string]any{}, "variantValues": map[string]any{}}
		}
		document := map[string]any{"version": 1, "components": components, "componentSets": map[string]any{}, "instances": map[string]any{"root": map[string]any{"id": "root", "source": map[string]any{"kind": "local-component", "componentId": "component-00"}, "variantSelection": map[string]any{}, "propertyValues": map[string]any{}, "overrides": []any{}, "slotContent": map[string]any{}}}}
		return componentFixtureText(t, document)
	}
	p := NewPlanner(rustPath(t))
	defer p.Close()
	exact, _ := callComponent(t, p, componentRequest("fg36-semantic-depth-exact", 1, makeDocument(64)))
	if !exact.OK {
		t.Fatalf("semantic expansion depth exact limit should pass: %+v", exact)
	}
	over, _ := callComponent(t, p, componentRequest("fg36-semantic-depth-over", 1, makeDocument(65)))
	if over.Error == nil || over.Error.Code != "COMPONENT_RESOURCE_LIMIT" {
		t.Fatalf("semantic expansion depth +1 should independently fail closed: %+v", over)
	}
}

func TestFGC36SemanticExpansionWorkExactAndOneOver(t *testing.T) {
	const propertyCount = 38
	const instancesAtExactLimit = 2439 // 1 graph step + 2,439 * (3 fixed expansion steps + 38 property steps) == 100,000.
	properties := make(map[string]any, propertyCount)
	for i := 0; i < propertyCount; i++ {
		id := fmt.Sprintf("property-%02d", i)
		properties[id] = map[string]any{"id": id, "name": id, "kind": "boolean", "targetNodeId": "root-node", "defaultValue": false}
	}
	component := map[string]any{"id": "root-component", "name": "Root", "setId": nil, "rootNodeIds": []any{"root-node"}, "nodes": map[string]any{"root-node": map[string]any{"id": "root-node", "kind": "shape", "parentId": nil, "childIds": []any{}}}, "properties": properties, "variantValues": map[string]any{}}
	makeDocument := func(instanceCount int) string {
		instances := make(map[string]any, instanceCount)
		for i := 0; i < instanceCount; i++ {
			id := fmt.Sprintf("instance-%04d", i)
			instances[id] = map[string]any{"id": id, "source": map[string]any{"kind": "local-component", "componentId": "root-component"}, "variantSelection": map[string]any{}, "propertyValues": map[string]any{}, "overrides": []any{}, "slotContent": map[string]any{}}
		}
		return componentFixtureText(t, map[string]any{"version": 1, "components": map[string]any{"root-component": component}, "componentSets": map[string]any{}, "instances": instances})
	}
	p := NewPlanner(rustPath(t))
	defer p.Close()
	exact, _ := callComponent(t, p, componentRequest("fg36-expansion-work-exact", 1, makeDocument(instancesAtExactLimit)))
	if !exact.OK {
		t.Fatalf("semantic work exact limit should pass: %+v", exact)
	}
	over, _ := callComponent(t, p, componentRequest("fg36-expansion-work-over", 1, makeDocument(instancesAtExactLimit+1)))
	if over.Error == nil || over.Error.Code != "COMPONENT_RESOURCE_LIMIT" {
		t.Fatalf("semantic work limit + 1 should reject: %+v", over)
	}
}

func TestFGC36OuterParserDepthHasExactBoundary(t *testing.T) {
	p := NewPlanner(rustPath(t))
	defer p.Close()
	for depth, expected := range map[int]string{maxJSONDepth - 1: "REQUEST_INVALID", maxJSONDepth: "REQUEST_RESOURCE_LIMIT"} {
		request := []byte(fmt.Sprintf(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"fg36-outer-depth-%d","kind":"capabilities","unknown":%s}`, depth, strings.Repeat("[", depth)+"0"+strings.Repeat("]", depth)))
		result, _ := callComponent(t, p, request)
		if result.Error == nil || result.Error.Code != expected {
			t.Errorf("outer depth %d response=%+v want error %s", depth, result, expected)
		}
	}
}

func TestFGC36OuterFrameAndTruncationBoundaries(t *testing.T) {
	path := rustPath(t)
	// The real binary rejects the announced size before reading/allocating a body.
	cmd := exec.Command(path)
	var stdin bytes.Buffer
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], MaxRequestBytes+1)
	stdin.Write(header[:])
	cmd.Stdin = &stdin
	var stdout bytes.Buffer
	cmd.Stdout = &stdout
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	response, err := readFrame(bytes.NewReader(stdout.Bytes()), MaxResponseBytes)
	if err != nil {
		t.Fatal(err)
	}
	var oversized componentResult
	if err := json.Unmarshal(response, &oversized); err != nil || oversized.Error == nil || oversized.Error.Code != "REQUEST_TOO_LARGE" {
		t.Fatalf("oversized frame response=%s err=%v", response, err)
	}

	truncated := exec.Command(path)
	var partial bytes.Buffer
	binary.BigEndian.PutUint32(header[:], 20)
	partial.Write(header[:])
	partial.Write([]byte{1, 2, 3, 4, 5, 6, 7})
	truncated.Stdin = &partial
	var truncatedOut bytes.Buffer
	truncated.Stdout = &truncatedOut
	if err := truncated.Run(); err != nil {
		t.Fatal(err)
	}
	response, err = readFrame(bytes.NewReader(truncatedOut.Bytes()), MaxResponseBytes)
	if err != nil {
		t.Fatal(err)
	}
	var truncatedResult componentResult
	if err := json.Unmarshal(response, &truncatedResult); err != nil || truncatedResult.Error == nil || truncatedResult.Error.Code != "FRAME_TRUNCATED" {
		t.Fatalf("truncated frame response=%s err=%v", response, err)
	}
}

func TestFGC36InvalidResponseCorrelationInvalidatesWorker(t *testing.T) {
	p := helperPlanner(t, "stale")
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	_, err := p.ValidateComponentInstanceV1(ctx, "fg36-current", []byte(`{"version":1,"components":{},"componentSets":{},"instances":{}}`))
	cancel()
	if err == nil || p.PID() != 0 {
		t.Fatalf("stale response must invalidate session: err=%v pid=%d", err, p.PID())
	}
	p.Path = rustPath(t)
	result, _ := callComponent(t, p, componentRequest("fg36-recovered", 1, componentFixtureText(t, componentFixture(t))))
	if !result.OK || result.RequestID != "fg36-recovered" {
		t.Fatalf("worker did not recover after stale response: %+v", result)
	}
	p.Close()
}

func TestFGC36ValidationTimeoutTerminatesAndRestartsWorker(t *testing.T) {
	p := helperPlanner(t, "hang")
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	_, err := p.ValidateComponentInstanceV1(ctx, "fg36-timeout", []byte(`{"version":1,"components":{},"componentSets":{},"instances":{}}`))
	cancel()
	if err == nil || p.PID() != 0 {
		t.Fatalf("validation cancellation must kill worker: err=%v pid=%d", err, p.PID())
	}
	p.Path = rustPath(t)
	doc := componentFixture(t)
	doc["components"].(map[string]any)["card"].(map[string]any)["properties"].(map[string]any)["content-slot"].(map[string]any)["guidance"].(map[string]any)["maximumEntries"] = float64(2)
	result, _ := callComponent(t, p, componentRequest("fg36-after-timeout", 1, componentFixtureText(t, doc)))
	if !result.OK {
		t.Fatalf("worker did not restart after cancellation: %+v", result)
	}
	p.Close()
}

func TestFGC36DirectValidationCallUsesTwoSecondDeadline(t *testing.T) {
	p := helperPlanner(t, "hang")
	start := time.Now()
	_, err := p.Call(context.Background(), componentRequest("fg36-direct-timeout", 1, `{"version":1}`))
	elapsed := time.Since(start)
	if err == nil || elapsed < ComponentValidationTimeout-250*time.Millisecond || elapsed > ComponentValidationTimeout+time.Second || p.PID() != 0 {
		t.Fatalf("direct protocol call deadline mismatch: err=%v elapsed=%s pid=%d", err, elapsed, p.PID())
	}
	p.Path = rustPath(t)
	doc := componentFixture(t)
	doc["components"].(map[string]any)["card"].(map[string]any)["properties"].(map[string]any)["content-slot"].(map[string]any)["guidance"].(map[string]any)["maximumEntries"] = float64(2)
	delete(doc["instances"].(map[string]any), "external-instance")
	delete(doc["instances"].(map[string]any), "unresolved-instance")
	result, _ := callComponent(t, p, componentRequest("fg36-direct-timeout-restart", 1, componentFixtureText(t, doc)))
	if !result.OK {
		t.Fatalf("worker did not recover after automatic operation deadline: %+v", result)
	}
	p.Close()
}

func helperPlanner(t *testing.T, mode string) *Planner {
	t.Helper()
	p := NewPlanner(os.Args[0])
	p.Args = []string{"-test.run=TestFGC36PlannerHelperProcess"}
	p.Env = append(os.Environ(), "FG_C36_HELPER=1", "FG_C36_HELPER_MODE="+mode)
	return p
}

func TestFGC36PlannerHelperProcess(t *testing.T) {
	if os.Getenv("FG_C36_HELPER") != "1" {
		return
	}
	reader := bufio.NewReader(os.Stdin)
	writer := bufio.NewWriter(os.Stdout)
	for {
		var header [4]byte
		if _, err := io.ReadFull(reader, header[:]); err != nil {
			os.Exit(0)
		}
		body := make([]byte, binary.BigEndian.Uint32(header[:]))
		if _, err := io.ReadFull(reader, body); err != nil {
			os.Exit(3)
		}
		var request map[string]json.RawMessage
		_ = json.Unmarshal(body, &request)
		var id, kind string
		_ = json.Unmarshal(request["requestId"], &id)
		_ = json.Unmarshal(request["kind"], &kind)
		var response []byte
		if kind == "capabilities" {
			response, _ = json.Marshal(validCaps(id))
		} else if os.Getenv("FG_C36_HELPER_MODE") == "hang" {
			time.Sleep(30 * time.Second)
			os.Exit(0)
		} else {
			response = []byte(fmt.Sprintf(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"componentSemanticVersion":1,"requestId":"stale-id","operationKind":"validateComponentInstanceV1","ok":true,"phase":"component-validation","diagnostics":[]}`))
		}
		binary.BigEndian.PutUint32(header[:], uint32(len(response)))
		if _, err := writer.Write(header[:]); err != nil {
			os.Exit(4)
		}
		if _, err := writer.Write(response); err != nil {
			os.Exit(5)
		}
		if err := writer.Flush(); err != nil {
			os.Exit(6)
		}
	}
}

func TestFGC36GoComponentResponseContractRejectsContradictions(t *testing.T) {
	request := componentRequest("x", 1, `{"version":1}`)
	valid := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"componentSemanticVersion":1,"requestId":"x","operationKind":"validateComponentInstanceV1","ok":true,"phase":"component-validation","diagnostics":[]}`)
	if err := validateResponse(request, valid); err != nil {
		t.Fatalf("valid empty result rejected: %v", err)
	}
	for name, response := range map[string]string{
		"document returned":         strings.Replace(string(valid), `"diagnostics":[]`, `"diagnostics":[],"document":{}`, 1),
		"missing version":           strings.Replace(string(valid), `"componentSemanticVersion":1,`, ``, 1),
		"warning and error success": strings.Replace(string(valid), `"diagnostics":[]`, `"diagnostics":[{"code":"COMPONENT_SLOT_GUIDANCE_EXCEEDED","path":"$.instances.x","severity":"error","detail":"bad"}]`, 1),
		"error without diagnostic":  strings.Replace(string(valid), `"ok":true`, `"ok":false`, 1),
		"unknown diagnostic field":  strings.Replace(string(valid), `"diagnostics":[]`, `"diagnostics":[{"code":"COMPONENT_SLOT_GUIDANCE_EXCEEDED","path":"$.instances.x","severity":"warning","detail":"warn","extra":true}]`, 1),
		"diagnostic code order":     strings.Replace(string(valid), `"diagnostics":[]`, `"diagnostics":[{"code":"COMPONENT_SLOT_GUIDANCE_EXCEEDED","path":"$.x","severity":"warning","detail":"a"},{"code":"COMPONENT_REFERENCE_UNRESOLVED","path":"$.x","severity":"warning","detail":"b"}]`, 1),
	} {
		t.Run(name, func(t *testing.T) {
			if err := validateResponse(request, []byte(response)); err == nil {
				t.Fatal("contradictory response accepted")
			}
		})
	}
}

func TestFGC36ResponsePayloadSizeUsesSerializedDiagnosticBytes(t *testing.T) {
	request := componentRequest("x", 1, `{}`)
	diagnosticPrefix := `[{"code":"COMPONENT_SLOT_GUIDANCE_EXCEEDED","path":"$.x","severity":"warning","detail":"`
	diagnosticSuffix := `"}]`
	limit := 768 << 10
	exactDiagnosticPayload := diagnosticPrefix + strings.Repeat("x", limit-len(diagnosticPrefix)-len(diagnosticSuffix)) + diagnosticSuffix
	if len(exactDiagnosticPayload) != limit {
		t.Fatalf("exact diagnostic payload size=%d want=%d", len(exactDiagnosticPayload), limit)
	}
	responseFor := func(diagnostics string) []byte {
		return []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"componentSemanticVersion":1,"requestId":"x","operationKind":"validateComponentInstanceV1","ok":true,"phase":"component-validation","diagnostics":` + diagnostics + `}`)
	}
	exactResponse := responseFor(exactDiagnosticPayload)
	if len(exactResponse) >= MaxResponseBytes {
		t.Fatal("test payload exceeds response limit")
	}
	if err := validateResponse(request, exactResponse); err != nil {
		t.Fatalf("exact serialized diagnostic limit rejected: %v", err)
	}
	overDiagnosticPayload := diagnosticPrefix + strings.Repeat("x", limit-len(diagnosticPrefix)-len(diagnosticSuffix)+1) + diagnosticSuffix
	if err := validateResponse(request, responseFor(overDiagnosticPayload)); err == nil {
		t.Fatal("diagnostic array one byte beyond limit accepted")
	}
}

func TestFGC36GoStrictDuplicateScannerBoundaries(t *testing.T) {
	if err := scanUniqueJSON([]byte(`{"a":1,"nested":{"property":1,"pro\u0070erty":2}}`)); err != errDuplicateMember {
		t.Fatalf("escaped nested duplicate = %v", err)
	}
	if err := scanUniqueJSON([]byte(strings.Repeat("[", maxJSONDepth+1) + "0" + strings.Repeat("]", maxJSONDepth+1))); err != errJSONDepthLimit {
		t.Fatalf("depth boundary +1 = %v", err)
	}
	values := "[" + strings.TrimSuffix(strings.Repeat("0,", maxJSONValues), ",") + "]"
	if err := scanUniqueJSON([]byte(values)); err != errJSONWorkLimit {
		t.Fatalf("work boundary +1 = %v", err)
	}
}

func TestFGC36RequestBoundaryIsRejectedBeforeWorkerLaunch(t *testing.T) {
	p := NewPlanner(filepath.Join(t.TempDir(), "does-not-exist"))
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	result, err := p.Call(ctx, bytes.Repeat([]byte("x"), MaxRequestBytes+1))
	if err != nil || p.PID() != 0 {
		t.Fatalf("oversize request should fail locally: err=%v pid=%d", err, p.PID())
	}
	var parsed componentResult
	if json.Unmarshal(result, &parsed) != nil || parsed.Error == nil || parsed.Error.Code != "REQUEST_TOO_LARGE" {
		t.Fatalf("local oversized request result=%s", result)
	}
	valid := []byte(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"fg36-frame-exact","kind":"capabilities"}`)
	valid = append(valid, bytes.Repeat([]byte(" "), MaxRequestBytes-len(valid))...)
	if len(valid) != MaxRequestBytes {
		t.Fatalf("test request size=%d want=%d", len(valid), MaxRequestBytes)
	}
	p.Path = rustPath(t)
	response, err := p.Call(ctx, valid)
	if err != nil {
		t.Fatalf("exact frame limit rejected: %v", err)
	}
	if err := validateResponse(valid, response); err != nil {
		t.Fatalf("exact frame limit response invalid: %v", err)
	}
}
