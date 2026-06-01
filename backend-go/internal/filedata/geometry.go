package filedata

import (
	"encoding/json"
	"math"
)

// NormalizeShape repairs keyword tokens and geometry used for hit-testing.
func NormalizeShape(shape map[string]any) bool {
	if shape == nil {
		return false
	}
	kwChanged := normalizeShapeKeywords(shape)
	geoChanged := normalizeShapeGeometry(shape)
	return kwChanged || geoChanged
}

func normalizeShapeGeometry(shape map[string]any) bool {
	changed := false
	if c := normalizeMatrixField(shape, "transform"); c {
		changed = true
	}
	if c := normalizeMatrixField(shape, "transform-inverse"); c {
		changed = true
	}
	if shape["transform-inverse"] == nil {
		if inv := invertMatrixMap(shape["transform"]); inv != nil {
			shape["transform-inverse"] = inv
			changed = true
		}
	}
	if pts, ok := shape["points"].([]any); ok {
		if normalized, c := normalizePoints(pts); c {
			shape["points"] = normalized
			changed = true
		}
	}
	if c := normalizeSelrect(shape); c {
		changed = true
	}
	return changed
}

func normalizeMatrixField(shape map[string]any, key string) bool {
	before := shape[key]
	m, ok := matrixToMap(before)
	if !ok {
		return false
	}
	shape[key] = m
	return before == nil || !matricesEqual(before, m)
}

func matricesEqual(a any, m map[string]any) bool {
	other, ok := matrixToMap(a)
	if !ok {
		return false
	}
	for _, k := range []string{"a", "b", "c", "d", "e", "f"} {
		f1, ok1 := toFloat64(m[k])
		f2, ok2 := toFloat64(other[k])
		if !ok1 || !ok2 || f1 != f2 {
			return false
		}
	}
	return true
}

// matrixToMap converts Penpot matrix values to {a,b,c,d,e,f} maps.
// Plain arrays are not decoded as Matrix instances by the frontend.
func matrixToMap(v any) (map[string]any, bool) {
	if v == nil {
		return nil, false
	}
	keys := []string{"a", "b", "c", "d", "e", "f"}
	switch m := v.(type) {
	case map[string]any:
		out := make(map[string]any, 6)
		for _, k := range keys {
			f, ok := toFloat64(m[k])
			if !ok {
				return nil, false
			}
			out[k] = f
		}
		return out, true
	case []any:
		if len(m) != 6 {
			return nil, false
		}
		out := make(map[string]any, 6)
		for i, k := range keys {
			f, ok := toFloat64(m[i])
			if !ok {
				return nil, false
			}
			out[k] = f
		}
		return out, true
	default:
		return nil, false
	}
}

func invertMatrixMap(v any) map[string]any {
	m, ok := matrixToMap(v)
	if !ok {
		return nil
	}
	a, _ := toFloat64(m["a"])
	b, _ := toFloat64(m["b"])
	c, _ := toFloat64(m["c"])
	d, _ := toFloat64(m["d"])
	e, _ := toFloat64(m["e"])
	f, _ := toFloat64(m["f"])
	det := a*d - b*c
	if math.Abs(det) < 1e-12 {
		return matrixMap(1, 0, 0, 1, 0, 0)
	}
	invDet := 1 / det
	return matrixMap(
		d*invDet,
		-b*invDet,
		-c*invDet,
		a*invDet,
		(c*f-d*e)*invDet,
		(b*e-a*f)*invDet,
	)
}

func matrixMap(a, b, c, d, e, f float64) map[string]any {
	return map[string]any{"a": a, "b": b, "c": c, "d": d, "e": e, "f": f}
}

func normalizePoints(points []any) ([]any, bool) {
	changed := false
	out := make([]any, len(points))
	for i, item := range points {
		if pt, c := pointToMap(item); pt != nil {
			out[i] = pt
			if c {
				changed = true
			}
			continue
		}
		out[i] = item
	}
	return out, changed
}

func pointToMap(v any) (map[string]any, bool) {
	if m, ok := v.(map[string]any); ok {
		x, xok := toFloat64(m["x"])
		y, yok := toFloat64(m["y"])
		if xok && yok {
			return map[string]any{"x": x, "y": y}, false
		}
	}
	if arr, ok := v.([]any); ok && len(arr) >= 2 {
		x, xok := toFloat64(arr[0])
		y, yok := toFloat64(arr[1])
		if xok && yok {
			return map[string]any{"x": x, "y": y}, true
		}
	}
	return nil, false
}

func normalizeSelrect(shape map[string]any) bool {
	changed := false
	sr, ok := shape["selrect"].(map[string]any)
	if !ok {
		x, xok := toFloat64(shape["x"])
		y, yok := toFloat64(shape["y"])
		w, wok := toFloat64(shape["width"])
		h, hok := toFloat64(shape["height"])
		if !(xok && yok && wok && hok) {
			return false
		}
		sr = map[string]any{"x": x, "y": y, "width": w, "height": h}
		shape["selrect"] = sr
		changed = true
	}

	x, xok := toFloat64(firstPresent(sr, "x", shape["x"]))
	y, yok := toFloat64(firstPresent(sr, "y", shape["y"]))
	w, wok := toFloat64(firstPresent(sr, "width", shape["width"]))
	h, hok := toFloat64(firstPresent(sr, "height", shape["height"]))
	if !(xok && yok && wok && hok) {
		return changed
	}

	if _, ok := sr["x"]; !ok {
		sr["x"] = x
		changed = true
	}
	if _, ok := sr["y"]; !ok {
		sr["y"] = y
		changed = true
	}
	if _, ok := sr["width"]; !ok {
		sr["width"] = w
		changed = true
	}
	if _, ok := sr["height"]; !ok {
		sr["height"] = h
		changed = true
	}
	if _, ok := sr["x1"]; !ok {
		sr["x1"] = x
		changed = true
	}
	if _, ok := sr["y1"]; !ok {
		sr["y1"] = y
		changed = true
	}
	if _, ok := sr["x2"]; !ok {
		sr["x2"] = x + w
		changed = true
	}
	if _, ok := sr["y2"]; !ok {
		sr["y2"] = y + h
		changed = true
	}
	return changed
}

func firstPresent(m map[string]any, key string, fallback any) any {
	if v, ok := m[key]; ok {
		return v
	}
	return fallback
}

func toFloat64(v any) (float64, bool) {
	switch n := v.(type) {
	case float64:
		return n, true
	case float32:
		return float64(n), true
	case int:
		return float64(n), true
	case int64:
		return float64(n), true
	case json.Number:
		f, err := n.Float64()
		return f, err == nil
	default:
		return 0, false
	}
}
