package filedata

import (
	"strings"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/transit"
)

func TestNormalizeShapeGeometryMatrixAndPoints(t *testing.T) {
	shape := map[string]any{
		"type":   "ellipse",
		"x":      100.0,
		"y":      200.0,
		"width":  120.0,
		"height": 80.0,
		"transform": []any{
			1.0, 0.0, 0.0, 1.0, 0.0, 0.0,
		},
		"points": []any{
			[]any{100.0, 200.0},
			[]any{220.0, 200.0},
			[]any{220.0, 280.0},
			[]any{100.0, 280.0},
		},
	}
	if !NormalizeShape(shape) {
		t.Fatal("expected geometry normalization")
	}
	transform, ok := shape["transform"].(map[string]any)
	if !ok || transform["a"] != 1.0 {
		t.Fatalf("transform = %T %#v", shape["transform"], shape["transform"])
	}
	inverse, ok := shape["transform-inverse"].(map[string]any)
	if !ok || inverse["a"] != 1.0 {
		t.Fatalf("transform-inverse = %T %#v", shape["transform-inverse"], shape["transform-inverse"])
	}
	sr, ok := shape["selrect"].(map[string]any)
	if !ok {
		t.Fatalf("selrect = %T", shape["selrect"])
	}
	for _, key := range []string{"x1", "y1", "x2", "y2"} {
		if _, ok := sr[key]; !ok {
			t.Fatalf("selrect missing %q: %#v", key, sr)
		}
	}
	pts := shape["points"].([]any)
	pt0, ok := pts[0].(map[string]any)
	if !ok || pt0["x"] != 100.0 || pt0["y"] != 200.0 {
		t.Fatalf("point[0] = %#v", pts[0])
	}
}

func TestNormalizeShapeKeywordsEllipse(t *testing.T) {
	shape := map[string]any{
		"id":   "22222222-3333-4444-5555-666666666666",
		"type": "ellipse",
		"name": "Ellipse",
		"fills": []any{
			map[string]any{"fill-color": "#112769", "fill-opacity": 1},
		},
	}
	if !NormalizeShape(shape) {
		t.Fatal("expected keyword normalization")
	}
	if shape["type"] != Kw("ellipse") {
		t.Fatalf("type = %v, want %q", shape["type"], Kw("ellipse"))
	}
}

func TestMatrixTransitMapNotArray(t *testing.T) {
	shape := map[string]any{
		"transform": matrixMap(1, 0, 0, 1, 0, 0),
	}
	out, err := transit.JSONToTransit(mustJSON(shape))
	if err != nil {
		t.Fatal(err)
	}
	s := string(out)
	if strings.Contains(s, `"~:transform",[1`) {
		t.Fatalf("transform must not be plain array in transit:\n%s", s)
	}
	if !strings.Contains(s, "~:a") {
		t.Fatalf("expected matrix map keys in transit:\n%s", s)
	}
	if !strings.Contains(s, `"~#matrix"`) {
		t.Fatalf("expected ~#matrix tag in transit:\n%s", s)
	}
}
