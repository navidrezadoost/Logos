package transit

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestEncodePlainStringMap_emptyMap(t *testing.T) {
	out, err := EncodePlainStringMap(map[string]string{})
	if err != nil {
		t.Fatal(err)
	}
	if len(out) == 0 {
		t.Fatal("empty map must encode to non-empty transit body")
	}
	if !IsTransitMapBody(out) {
		t.Fatalf("expected transit map body, got %q", string(out))
	}
}
func TestEncodePlainStringMap_keepsStringKeysAndValues(t *testing.T) {
	objectID := "2f709b19-9811-4462-ab0e-ddc67949a1f5/f177fa33-591f-4651-9687-8ec4c805fffc/3ef24a0f/frame"
	mediaID := "ae691012-77e9-42f6-88eb-46444a93c6dd"

	out, err := EncodePlainStringMap(map[string]string{objectID: mediaID})
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if strings.Contains(s, "~:"+objectID) {
		t.Fatalf("object-id must stay a plain string, got keyword encoding:\n%s", s)
	}
	if strings.Contains(s, "~u"+mediaID) {
		t.Fatalf("media-id must stay a plain string, got UUID encoding:\n%s", s)
	}
	if !strings.Contains(s, objectID) || !strings.Contains(s, mediaID) {
		t.Fatalf("expected plain strings in output:\n%s", s)
	}
}

func TestJSONToTransit_UUIDMapKeys(t *testing.T) {
	pageID := "9e92f20b-9b14-4ba9-bf99-df606c2fee83"
	rootID := "00000000-0000-0000-0000-000000000000"

	raw, err := json.Marshal(map[string]any{
		"data": map[string]any{
			"pages": []any{pageID},
			"pages-index": map[string]any{
				pageID: map[string]any{
					"id":   pageID,
					"name": "Page 1",
					"objects": map[string]any{
						rootID: map[string]any{
							"id":   rootID,
							"type": "frame",
						},
					},
				},
			},
		},
	})
	if err != nil {
		t.Fatal(err)
	}

	out, err := JSONToTransit(raw)
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)

	for _, token := range []string{
		"~u" + pageID,
		"~u" + rootID,
	} {
		if !strings.Contains(s, token) {
			t.Fatalf("expected transit UUID token %q in output:\n%s", token, s)
		}
	}
	if strings.Contains(s, "~:"+pageID) {
		t.Fatalf("page id must not be encoded as keyword:\n%s", s)
	}
	if strings.Contains(s, "~:"+rootID) {
		t.Fatalf("root shape id must not be encoded as keyword:\n%s", s)
	}
}

func TestJSONToTransit_GeometryTags(t *testing.T) {
	shape := map[string]any{
		"transform": map[string]any{"a": 1.0, "b": 0.0, "c": 0.0, "d": 1.0, "e": 10.0, "f": 20.0},
		"selrect": map[string]any{
			"x": 10.0, "y": 20.0, "width": 120.0, "height": 80.0,
			"x1": 10.0, "y1": 20.0, "x2": 130.0, "y2": 100.0,
		},
		"points": []any{
			map[string]any{"x": 10.0, "y": 20.0},
			map[string]any{"x": 130.0, "y": 20.0},
		},
	}
	raw, err := json.Marshal(shape)
	if err != nil {
		t.Fatal(err)
	}
	out, err := JSONToTransit(raw)
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	for _, token := range []string{`"~#matrix"`, `"~#rect"`, `"~#point"`} {
		if !strings.Contains(s, token) {
			t.Fatalf("expected %s in output:\n%s", token, s)
		}
	}
}

func TestTransitToJSON_GeometryTags(t *testing.T) {
	in := []any{
		"~#matrix",
		[]any{"^ ", "~:a", 1.0, "~:b", 0.0, "~:c", 0.0, "~:d", 1.0, "~:e", 0.0, "~:f", 0.0},
	}
	raw, err := json.Marshal(in)
	if err != nil {
		t.Fatal(err)
	}
	out, err := TransitToJSON(raw)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(out, &m); err != nil {
		t.Fatal(err)
	}
	if m["a"].(float64) != 1.0 || m["f"].(float64) != 0.0 {
		t.Fatalf("matrix = %#v", m)
	}
}

func TestJSONToTransit_KeywordGeometryKeys(t *testing.T) {
	shape := map[string]any{
		"~:x": 1.0, "~:y": 2.0,
	}
	raw, err := json.Marshal(map[string]any{"point": shape})
	if err != nil {
		t.Fatal(err)
	}
	// Simulate wrapped ~#point map as stored by Clojure exports.
	wrapped, err := json.Marshal(map[string]any{
		"~#point": map[string]any{"~:x": 10.0, "~:y": 20.0},
	})
	if err != nil {
		t.Fatal(err)
	}
	out, err := JSONToTransit(wrapped)
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if !strings.Contains(s, `"~#point"`) || !strings.Contains(s, `"~:x"`) {
		t.Fatalf("expected tagged point with keyword keys:\n%s", s)
	}
	if strings.Contains(s, `"~#point",{`) {
		t.Fatalf("inner point map must be transit array, not JSON object:\n%s", s)
	}
	_ = raw
}

func TestJSONToTransit_KeywordMapKeys(t *testing.T) {
	raw, err := json.Marshal(map[string]any{
		"name": "Page 1",
		"options": map[string]any{
			"components-v2": true,
		},
	})
	if err != nil {
		t.Fatal(err)
	}

	out, err := JSONToTransit(raw)
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if !strings.Contains(s, "~:name") || !strings.Contains(s, "~:options") || !strings.Contains(s, "~:components-v2") {
		t.Fatalf("expected kebab keyword keys in output:\n%s", s)
	}
}
