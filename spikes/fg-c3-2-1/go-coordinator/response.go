package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"regexp"
	"strings"
)

const (
	protocolVersion   = 2
	semanticVersion   = 1
	diagnosticVersion = 1
)

var uuidText = regexp.MustCompile(`^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`)
var knownDiagnosticCodes = words(`DOCUMENT_SCHEMA_INVALID UNSUPPORTED_SCHEMA_VERSION MALFORMED_ID RESERVED_RENDERER_ID DUPLICATE_ID MISSING_PAGE MISSING_SHAPE CROSS_PAGE_REFERENCE ROOT_PARENT_MISMATCH DUPLICATE_ROOT_MEMBERSHIP DUPLICATE_CHILD DUPLICATE_CHILD_MEMBERSHIP INVALID_SIBLING_ORDER PARENT_CHILD_MISMATCH NON_CONTAINER_PARENT CYCLE ORPHAN_SHAPE INVALID_GEOMETRY GEOMETRY_OUT_OF_RANGE NON_FINITE_TRANSFORM SINGULAR_TRANSFORM TRANSFORM_INVERSE_MISMATCH INVALID_OPACITY INVALID_FRAME_REFERENCE FRAME_MEMBERSHIP_MISMATCH INVALID_COMPONENT_REFERENCE EXTENSION_KEY_COLLISION OPERATION_PRECONDITION_FAILED INVALID_OPERATION REQUEST_INVALID REQUEST_INVALID_UTF8 REQUEST_ID_INVALID REQUEST_TOO_LARGE PROTOCOL_VERSION_UNSUPPORTED SEMANTIC_VERSION_UNSUPPORTED DIAGNOSTIC_VERSION_UNSUPPORTED OPERATION_INVALID PROJECTION_ADDRESS_INVALID DETACH_ID_INVALID DETACH_ID_COLLISION RESPONSE_TOO_LARGE FRAME_TRUNCATED PLANNER_INTERNAL_ERROR`)
var knownErrorCodes = words(`REQUEST_INVALID REQUEST_INVALID_UTF8 REQUEST_ID_INVALID REQUEST_TOO_LARGE PROTOCOL_VERSION_UNSUPPORTED SEMANTIC_VERSION_UNSUPPORTED DIAGNOSTIC_VERSION_UNSUPPORTED OPERATION_INVALID PROJECTION_ADDRESS_INVALID DETACH_ID_INVALID DETACH_ID_COLLISION RESPONSE_TOO_LARGE FRAME_TRUNCATED PLANNER_INTERNAL_ERROR`)

type protocolDiagnostic struct {
	Code      string  `json:"code"`
	Path      string  `json:"path"`
	ObjectID  *string `json:"objectId,omitempty"`
	RelatedID *string `json:"relatedId,omitempty"`
}
type plannerError struct {
	Code    string `json:"code"`
	Path    string `json:"path"`
	Message string `json:"message"`
}

// validateResponse validates protocol metadata and every protocol-owned field.
// The document candidate remains an open payload, but its identity is checked.
func validateResponse(request, raw []byte) error {
	var req map[string]json.RawMessage
	if err := json.Unmarshal(request, &req); err != nil {
		req = nil
	}
	wantID := stringField(req["requestId"])
	if wantID != nil && (*wantID == "" || len(*wantID) > 128) {
		wantID = nil
	}
	var envelope map[string]json.RawMessage
	dec := json.NewDecoder(bytes.NewReader(raw))
	if err := dec.Decode(&envelope); err != nil {
		return errors.New("response is not a JSON object")
	}
	if envelope == nil {
		return errors.New("response is not an object")
	}
	var extra any
	if err := dec.Decode(&extra); err != io.EOF {
		return errors.New("response contains trailing or malformed JSON")
	}
	allowed := set("protocolVersion", "semanticVersion", "diagnosticVersion", "requestId", "ok", "phase", "document", "operationKind", "diagnostics", "ids", "error", "capabilities", "limits")
	for key := range envelope {
		if !allowed[key] {
			return fmt.Errorf("unknown response field %q", key)
		}
	}
	if !intFieldIs(envelope["protocolVersion"], protocolVersion) || !intFieldIs(envelope["semanticVersion"], semanticVersion) || !intFieldIs(envelope["diagnosticVersion"], diagnosticVersion) {
		return errors.New("response version is unsupported or missing")
	}
	if id, exists := envelope["requestId"]; !exists {
		return errors.New("response requestId is missing")
	} else if wantID == nil {
		if string(id) != "null" {
			return errors.New("response requestId must be null for an uncorrelatable request")
		}
	} else if stringField(id) == nil || *stringField(id) != *wantID {
		return errors.New("response requestId does not match request")
	}
	ok, okPresent := boolField(envelope["ok"])
	if !okPresent {
		return errors.New("response ok is missing or malformed")
	}
	if rawErr, hasErr := envelope["error"]; hasErr {
		if ok || hasErr && hasAny(envelope, "phase", "document", "operationKind", "ids", "capabilities", "limits") {
			return errors.New("contradictory error and result fields")
		}
		var e plannerError
		if err := decodeStrict(rawErr, &e); err != nil || !knownErrorCodes[e.Code] || e.Path == "" || e.Message == "" {
			return errors.New("malformed structured planner error")
		}
		if err := validateDiagnostics(envelope["diagnostics"], true); err != nil {
			return err
		}
		var diagnostics []protocolDiagnostic
		if err := json.Unmarshal(envelope["diagnostics"], &diagnostics); err != nil || len(diagnostics) != 1 || diagnostics[0].Code != e.Code || diagnostics[0].Path != e.Path {
			return errors.New("planner error and diagnostic are contradictory")
		}
		return nil
	}
	kind := stringField(req["kind"])
	if kind == nil {
		return errors.New("successful response cannot be correlated to a known request kind")
	}
	switch *kind {
	case "capabilities":
		if !ok || hasAny(envelope, "phase", "document", "operationKind", "ids", "diagnostics") {
			return errors.New("capabilities response has contradictory fields")
		}
		return validateCapabilities(envelope)
	case "evaluate":
		phase := stringField(envelope["phase"])
		if phase == nil || (*phase != "evaluated" && *phase != "input-validation" && *phase != "operation-rejected") {
			return errors.New("unknown or missing evaluation phase")
		}
		if err := validateDiagnostics(envelope["diagnostics"], true); err != nil {
			return err
		}
		if ok {
			var successDiagnostics []json.RawMessage
			if err := json.Unmarshal(envelope["diagnostics"], &successDiagnostics); err != nil || len(successDiagnostics) != 0 {
				return errors.New("successful evaluation must not contain diagnostics")
			}
			if *phase != "evaluated" || hasAny(envelope, "error", "ids", "capabilities", "limits") {
				return errors.New("successful evaluation has contradictory fields")
			}
			var doc map[string]json.RawMessage
			if err := json.Unmarshal(envelope["document"], &doc); err != nil || doc == nil {
				return errors.New("successful evaluation document is missing or malformed")
			}
			requestedDoc := rawObjectID(req["document"])
			candidateDoc := stringField(doc["id"])
			if requestedDoc == nil || candidateDoc == nil || *requestedDoc != *candidateDoc {
				return errors.New("candidate document does not belong to requested document")
			}
			var schemaVersion int
			var pages, shapes []json.RawMessage
			if json.Unmarshal(doc["schema-version"], &schemaVersion) != nil || schemaVersion != 1 || json.Unmarshal(doc["pages"], &pages) != nil || pages == nil || json.Unmarshal(doc["shapes"], &shapes) != nil || shapes == nil {
				return errors.New("candidate document does not match the supported document result shape")
			}
			if !has(envelope, "operationKind") {
				return errors.New("candidate operation kind is missing")
			}
			requestedOperation := rawObjectKind(req["operation"])
			returnedOperation := stringField(envelope["operationKind"])
			if requestedOperation == nil || returnedOperation == nil || *requestedOperation != *returnedOperation {
				return errors.New("candidate operation kind does not match request")
			}
		} else {
			if *phase == "evaluated" || hasAny(envelope, "document", "operationKind", "ids", "error", "capabilities", "limits") {
				return errors.New("rejected evaluation has contradictory fields")
			}
			var rejectedDiagnostics []json.RawMessage
			if err := json.Unmarshal(envelope["diagnostics"], &rejectedDiagnostics); err != nil || len(rejectedDiagnostics) == 0 {
				return errors.New("rejected evaluation must contain at least one diagnostic")
			}
		}
		return nil
	case "deriveIds", "checkDetachIds":
		if !ok || hasAny(envelope, "phase", "document", "operationKind", "diagnostics", "error", "capabilities", "limits") {
			return errors.New("identity response has contradictory fields")
		}
		var ids []string
		if err := decodeStrict(envelope["ids"], &ids); err != nil {
			return errors.New("identity result is missing or malformed")
		}
		if *kind == "deriveIds" {
			return validateDerivedIdentities(req, ids)
		}
		return validateDetachIdentities(req, ids)
	default:
		return errors.New("unknown request operation cannot produce a successful response")
	}
}

type definitionStep struct {
	ComponentID           string `json:"componentId"`
	ThroughInstanceNodeID string `json:"throughInstanceNodeId"`
}
type slotStep struct {
	PropertyID     string `json:"propertyId"`
	AuthoredNodeID string `json:"authoredNodeId"`
}
type projectionAddress struct {
	DocumentID       string           `json:"documentId"`
	InstanceID       string           `json:"instanceId"`
	DefinitionRoute  []definitionStep `json:"definitionRoute"`
	DefinitionNodeID string           `json:"definitionNodeId"`
	SlotRoute        []slotStep       `json:"slotRoute"`
	ExternalRevision *string          `json:"externalRevision"`
}

func validateDerivedIdentities(req map[string]json.RawMessage, ids []string) error {
	var rawAddresses []json.RawMessage
	if req == nil || json.Unmarshal(req["addresses"], &rawAddresses) != nil || rawAddresses == nil || len(rawAddresses) != len(ids) {
		return errors.New("derived identity count does not match valid addresses")
	}
	byAddress := make(map[string]string, len(ids))
	byIdentity := make(map[string]string, len(ids))
	for index, rawAddress := range rawAddresses {
		address, err := decodeProjectionAddress(rawAddress)
		if err != nil {
			return fmt.Errorf("projection address %d is malformed", index)
		}
		id, err := canonicalUUID(ids[index])
		if err != nil {
			return fmt.Errorf("derived identity %d is not a valid UUID", index)
		}
		encoded, err := json.Marshal(address)
		if err != nil {
			return errors.New("projection address cannot be normalized")
		}
		key := string(encoded)
		if previous, exists := byAddress[key]; exists && previous != id {
			return errors.New("equivalent projection addresses returned different identities")
		}
		if previous, exists := byIdentity[id]; exists && previous != key {
			return errors.New("distinct projection addresses returned a colliding identity")
		}
		byAddress[key], byIdentity[id] = id, key
	}
	return nil
}

func decodeProjectionAddress(raw json.RawMessage) (projectionAddress, error) {
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || fields == nil {
		return projectionAddress{}, errors.New("address is not an object")
	}
	for _, key := range []string{"documentId", "instanceId", "definitionRoute", "definitionNodeId"} {
		if _, ok := fields[key]; !ok {
			return projectionAddress{}, fmt.Errorf("missing %s", key)
		}
	}
	var address projectionAddress
	if json.Unmarshal(raw, &address) != nil || address.InstanceID == "" || address.DefinitionNodeID == "" {
		return projectionAddress{}, errors.New("invalid address fields")
	}
	if _, err := canonicalUUID(address.DocumentID); err != nil {
		return projectionAddress{}, err
	}
	if address.DefinitionRoute == nil {
		return projectionAddress{}, errors.New("definitionRoute is not an array")
	}
	var route []json.RawMessage
	if json.Unmarshal(fields["definitionRoute"], &route) != nil || route == nil {
		return projectionAddress{}, errors.New("definitionRoute is malformed")
	}
	for _, step := range route {
		if !hasStringFields(step, "componentId", "throughInstanceNodeId") {
			return projectionAddress{}, errors.New("definition route step is malformed")
		}
	}
	if slotRaw, exists := fields["slotRoute"]; exists {
		var slots []json.RawMessage
		if json.Unmarshal(slotRaw, &slots) != nil || slots == nil {
			return projectionAddress{}, errors.New("slotRoute is malformed")
		}
		for _, step := range slots {
			if !hasStringFields(step, "propertyId", "authoredNodeId") {
				return projectionAddress{}, errors.New("slot route step is malformed")
			}
		}
	} else {
		address.SlotRoute = []slotStep{}
	}
	if revision, exists := fields["externalRevision"]; exists && string(revision) != "null" {
		var value string
		if json.Unmarshal(revision, &value) != nil {
			return projectionAddress{}, errors.New("externalRevision is malformed")
		}
		address.ExternalRevision = &value
	}
	if address.DefinitionRoute == nil {
		address.DefinitionRoute = []definitionStep{}
	}
	return address, nil
}

func hasStringFields(raw json.RawMessage, required ...string) bool {
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || fields == nil {
		return false
	}
	for _, key := range required {
		if stringField(fields[key]) == nil {
			return false
		}
	}
	return true
}

func validateDetachIdentities(req map[string]json.RawMessage, ids []string) error {
	if req == nil {
		return errors.New("detach identity request is missing")
	}
	var candidates, existing []string
	if json.Unmarshal(req["candidateIds"], &candidates) != nil || candidates == nil || json.Unmarshal(req["existingIds"], &existing) != nil || existing == nil || len(candidates) != len(ids) {
		return errors.New("detach identity result count does not match request")
	}
	seen := make(map[string]bool, len(existing)+len(candidates))
	for _, value := range existing {
		id, err := canonicalUUID(value)
		if err != nil {
			return errors.New("existing authored identity is not a valid UUID")
		}
		if seen[id] {
			return errors.New("existing authored identities contain a duplicate")
		}
		seen[id] = true
	}
	for index, value := range ids {
		id, err := canonicalUUID(value)
		if err != nil {
			return fmt.Errorf("detach identity %d is not a valid UUID", index)
		}
		candidate, err := canonicalUUID(candidates[index])
		if err != nil || value != candidates[index] || candidate != id {
			return fmt.Errorf("detach identity %d does not match its requested candidate", index)
		}
		if seen[id] {
			return fmt.Errorf("detach identity %d collides with an existing or prior identity", index)
		}
		seen[id] = true
	}
	return nil
}

func canonicalUUID(value string) (string, error) {
	if !uuidText.MatchString(value) {
		return "", errors.New("invalid UUID text")
	}
	return strings.ToLower(value), nil
}

func validateCapabilities(envelope map[string]json.RawMessage) error {
	var caps struct {
		DocumentOperations struct {
			Version    int      `json:"version"`
			Operations []string `json:"operations"`
		} `json:"documentOperations"`
		ProjectionIdentity struct {
			Version    int      `json:"version"`
			Operations []string `json:"operations"`
		} `json:"projectionIdentity"`
		ComponentInstanceDomain struct {
			Status string `json:"status"`
			Owner  string `json:"owner"`
		} `json:"componentInstanceDomain"`
		ResponseContract struct {
			Version     int      `json:"version"`
			Correlation []string `json:"correlation"`
		} `json:"responseContract"`
		Diagnostics struct {
			Version int `json:"version"`
		} `json:"diagnostics"`
	}
	if err := decodeStrict(envelope["capabilities"], &caps); err != nil {
		return errors.New("capability set is malformed or has unknown fields")
	}
	var limits struct {
		MaxRequestBytes  int    `json:"maxRequestBytes"`
		MaxResponseBytes int    `json:"maxResponseBytes"`
		Framing          string `json:"framing"`
	}
	if err := decodeStrict(envelope["limits"], &limits); err != nil {
		return errors.New("capability limits are malformed")
	}
	if caps.DocumentOperations.Version != 1 || !sameStrings(caps.DocumentOperations.Operations, []string{"applyOperation", "validateDocument"}) || caps.ProjectionIdentity.Version != 1 || !sameStrings(caps.ProjectionIdentity.Operations, []string{"deriveIds", "checkDetachIds"}) || caps.ComponentInstanceDomain.Status != "not-supported-here" || caps.ComponentInstanceDomain.Owner != "TypeScript componentInstanceDomainV1" || caps.ResponseContract.Version != 1 || !sameStrings(caps.ResponseContract.Correlation, []string{"requestId", "operationKind"}) || caps.Diagnostics.Version != diagnosticVersion || limits.MaxRequestBytes != MaxRequestBytes || limits.MaxResponseBytes != MaxResponseBytes || limits.Framing != "uint32-be-length-prefixed" {
		return errors.New("planner capabilities are incompatible")
	}
	return nil
}

func validateDiagnostics(raw json.RawMessage, required bool) error {
	if len(raw) == 0 {
		if required {
			return errors.New("diagnostics are missing")
		}
		return nil
	}
	var ds []json.RawMessage
	if err := json.Unmarshal(raw, &ds); err != nil || ds == nil {
		return errors.New("diagnostics must be an array")
	}
	for _, item := range ds {
		var d protocolDiagnostic
		if err := decodeStrict(item, &d); err != nil || !knownDiagnosticCodes[d.Code] || !validDiagnosticPath(d.Path) || d.ObjectID != nil && !uuidText.MatchString(*d.ObjectID) || d.RelatedID != nil && !uuidText.MatchString(*d.RelatedID) {
			return errors.New("diagnostic has an invalid shape")
		}
	}
	return nil
}

func decodeStrict(raw []byte, target any) error {
	if len(raw) == 0 || string(raw) == "null" {
		return errors.New("missing value")
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	if err := d.Decode(target); err != nil {
		return err
	}
	var trailing any
	if err := d.Decode(&trailing); err != io.EOF {
		return errors.New("trailing or malformed JSON")
	}
	return nil
}
func rawObjectID(raw json.RawMessage) *string {
	var obj map[string]json.RawMessage
	if json.Unmarshal(raw, &obj) != nil {
		return nil
	}
	return stringField(obj["id"])
}
func rawObjectKind(raw json.RawMessage) *string {
	var obj map[string]json.RawMessage
	if json.Unmarshal(raw, &obj) != nil {
		return nil
	}
	return stringField(obj["kind"])
}
func stringField(raw json.RawMessage) *string {
	var v string
	if len(raw) == 0 || json.Unmarshal(raw, &v) != nil {
		return nil
	}
	return &v
}
func boolField(raw json.RawMessage) (bool, bool) {
	var v bool
	err := json.Unmarshal(raw, &v)
	return v, err == nil
}
func intFieldIs(raw json.RawMessage, expected int) bool {
	var v int
	return json.Unmarshal(raw, &v) == nil && v == expected
}
func has(m map[string]json.RawMessage, key string) bool { _, ok := m[key]; return ok }
func hasAny(m map[string]json.RawMessage, keys ...string) bool {
	for _, k := range keys {
		if has(m, k) {
			return true
		}
	}
	return false
}
func set(keys ...string) map[string]bool {
	m := map[string]bool{}
	for _, k := range keys {
		m[k] = true
	}
	return m
}
func words(value string) map[string]bool {
	result := map[string]bool{}
	for _, word := range bytes.Fields([]byte(value)) {
		result[string(word)] = true
	}
	return result
}
func validDiagnosticPath(path string) bool { return path == "$" || len(path) > 0 && path[0] == '/' }
func sameStrings(got, expected []string) bool {
	if len(got) != len(expected) {
		return false
	}
	seen := map[string]bool{}
	for _, v := range got {
		seen[v] = true
	}
	for _, v := range expected {
		if !seen[v] {
			return false
		}
	}
	return true
}
