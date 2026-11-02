// Package transit provides minimal Transit+JSON encoding/decoding for
// compatibility with the compiled ClojureScript frontend.
//
// Transit+JSON wire format summary (JSON mode):
//   - Maps:     ["^ ", "~:key1", val1, "~:key2", val2, ...]
//   - Keywords: "~:name"  (as both map keys and values)
//   - UUIDs:    "~u<uuid>"
//   - Datetimes:"~t<iso>"
//   - Escaped~: "~~rest"
//
// Reference: https://github.com/cognitect/transit-format
package transit

import (
	"encoding/json"
	"fmt"
	"regexp"
	"strings"
	"time"
)

// Keyword is a Go type whose JSON serialisation is the Transit keyword token
// "~:name".  Use it for struct fields that carry enum-like keyword values in
// Clojure (e.g. :type, :code in error maps).
type Keyword string

// Instant is a time value encoded as a Transit instant token (~t…).
type Instant struct {
	time.Time
}

// MarshalJSON implements json.Marshaler.
func (t Instant) MarshalJSON() ([]byte, error) {
	return json.Marshal("~t" + t.UTC().Format(time.RFC3339Nano))
}

// MarshalJSON implements json.Marshaler.
func (k Keyword) MarshalJSON() ([]byte, error) {
	return json.Marshal("~:" + string(k))
}

// uuidRE matches standard UUID strings (case-insensitive).
var uuidRE = regexp.MustCompile(`(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)

// ─── Encoding (Go → Transit+JSON) ────────────────────────────────────────────

// Encode marshals v to JSON then converts the result to Transit+JSON.
func Encode(v any) ([]byte, error) {
	jsonBytes, err := json.Marshal(v)
	if err != nil {
		return nil, err
	}
	return JSONToTransit(jsonBytes)
}

// transitMapKey encodes a JSON object key for Transit map entries.
// Logos file data uses UUIDs as map keys (pages-index, shape objects, …).
func transitMapKey(k string) string {
	if strings.HasPrefix(k, "~") {
		return k
	}
	if uuidRE.MatchString(k) {
		return "~u" + k
	}
	return "~:" + k
}

// transitMapKeyDecode reverses transitMapKey for inbound Transit maps.
func transitMapKeyDecode(k string) string {
	switch {
	case strings.HasPrefix(k, "~u"):
		return k[2:]
	case strings.HasPrefix(k, "~:"):
		return k[2:]
	default:
		return k
	}
}

// EncodePlainStringMap encodes a map with plain string keys and values.
// Logos RPC results such as get-file-object-thumbnails use
// [:map-of string string] — object-id paths and media-id UUIDs must stay
// strings, not Transit keywords or ~u UUID tokens.
func EncodePlainStringMap(m map[string]string) ([]byte, error) {
	arr := make([]any, 0, 1+len(m)*2)
	arr = append(arr, "^ ")
	for k, v := range m {
		arr = append(arr, k, v)
	}
	return json.Marshal(arr)
}

// IsTransitMapBody reports whether b is already a Transit map array (["^ ", …]).
func IsTransitMapBody(b []byte) bool {
	return len(b) >= 5 && b[0] == '[' && string(b[:5]) == `["^ "`
}

// JSONToTransit converts plain JSON bytes to Transit+JSON bytes.
// Maps become ["^ ", "~:key", val, ...]; UUID strings become "~u<uuid>".
func JSONToTransit(data []byte) ([]byte, error) {
	var raw any
	if err := json.Unmarshal(data, &raw); err != nil {
		return nil, fmt.Errorf("transit encode: json parse: %w", err)
	}
	return json.Marshal(toTransit(raw))
}

func toTransit(v any) any {
	switch val := v.(type) {
	case map[string]any:
		// Re-wrap values already stored as {"~#matrix": {...}} JSON maps.
		if len(val) == 1 {
			for tag, inner := range val {
				if strings.HasPrefix(tag, "~#") {
					return []any{tag, toTransit(inner)}
				}
			}
		}
		if isMatrixMap(val) {
			return []any{"~#matrix", transitMapFromKeys(val, matrixKeys)}
		}
		if isRectMap(val) {
			return []any{"~#rect", transitMapFromKeys(val, rectKeys(val))}
		}
		if isPointMap(val) {
			return []any{"~#point", transitMapFromKeys(val, pointKeys)}
		}
		arr := make([]any, 0, 1+len(val)*2)
		arr = append(arr, "^ ")
		for k, v2 := range val {
			arr = append(arr, transitMapKey(k), toTransit(v2))
		}
		return arr

	case []any:
		result := make([]any, len(val))
		for i, item := range val {
			result[i] = toTransit(item)
		}
		return result

	case string:
		// Already a Transit token (Keyword, UUID, datetime) — pass through.
		if strings.HasPrefix(val, "~:") ||
			strings.HasPrefix(val, "~u") ||
			strings.HasPrefix(val, "~t") {
			return val
		}
		// Auto-detect UUID values.
		if uuidRE.MatchString(val) {
			return "~u" + val
		}
		// Escape leading tilde/caret so the client doesn't misinterpret.
		if strings.HasPrefix(val, "~") {
			return "~~" + val[1:]
		}
		if strings.HasPrefix(val, "^") {
			return "~^" + val[1:]
		}
		return val

	default:
		return v
	}
}

// ─── Decoding (Transit+JSON → Go) ────────────────────────────────────────────

// TransitToJSON converts Transit+JSON bytes to plain JSON bytes.
// Keyword keys/values have their "~:" prefix stripped; UUID "~u…" → bare UUID string.
func TransitToJSON(data []byte) ([]byte, error) {
	var raw any
	if err := json.Unmarshal(data, &raw); err != nil {
		return nil, fmt.Errorf("transit decode: json parse: %w", err)
	}
	return json.Marshal(fromTransit(raw))
}

var (
	matrixKeys = []string{"a", "b", "c", "d", "e", "f"}
	pointKeys  = []string{"x", "y"}
)

func rectKeys(m map[string]any) []string {
	keys := []string{"x", "y", "width", "height", "x1", "y1", "x2", "y2"}
	out := make([]string, 0, len(keys))
	for _, k := range keys {
		if _, ok := geomFloat(m, k); ok {
			out = append(out, k)
		} else if _, ok := m[k]; ok {
			out = append(out, k)
		} else if _, ok := m["~:"+k]; ok {
			out = append(out, k)
		}
	}
	return out
}

func geomFloat(m map[string]any, key string) (float64, bool) {
	if v, ok := m[key]; ok {
		return transitFloat(v)
	}
	if v, ok := m["~:"+key]; ok {
		return transitFloat(v)
	}
	return 0, false
}

func isMatrixMap(m map[string]any) bool {
	for _, k := range matrixKeys {
		if _, ok := geomFloat(m, k); !ok {
			return false
		}
	}
	return true
}

func isPointMap(m map[string]any) bool {
	if _, ok := geomFloat(m, "x"); !ok {
		return false
	}
	if _, ok := geomFloat(m, "y"); !ok {
		return false
	}
	if _, ok := geomFloat(m, "width"); ok {
		return false
	}
	if _, ok := geomFloat(m, "a"); ok {
		return false
	}
	return true
}

func isRectMap(m map[string]any) bool {
	for _, k := range []string{"x", "y", "width", "height"} {
		if _, ok := geomFloat(m, k); !ok {
			return false
		}
	}
	// Full shapes also carry x/y/width/height — only tag selrect-like maps.
	for _, k := range []string{"type", "id", "fills", "strokes", "name", "parent-id", "frame-id", "~:type", "~:id", "~:fills", "~:strokes", "~:name", "~:parent-id", "~:frame-id"} {
		if _, ok := m[k]; ok {
			return false
		}
	}
	return true
}

func transitMapFromKeys(m map[string]any, keys []string) []any {
	arr := make([]any, 0, 1+len(keys)*2)
	arr = append(arr, "^ ")
	for _, k := range keys {
		if v, ok := geomFloat(m, k); ok {
			arr = append(arr, transitMapKey(k), v)
			continue
		}
		if v, ok := m[k]; ok {
			arr = append(arr, transitMapKey(k), toTransit(v))
		}
	}
	return arr
}

func transitFloat(v any) (float64, bool) {
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

func fromTransit(v any) any {
	switch val := v.(type) {
	case []any:
		// Logos geometry tags: ["~#matrix", ["^ ", ...]] → plain map for storage.
		if len(val) == 2 {
			if tag, ok := val[0].(string); ok && strings.HasPrefix(tag, "~#") {
				return fromTransit(val[1])
			}
		}
		// Transit map representation: ["^ ", key1, val1, key2, val2, ...]
		if len(val) > 0 {
			if s, ok := val[0].(string); ok && s == "^ " {
				m := make(map[string]any, (len(val)-1)/2)
				for i := 1; i+1 < len(val); i += 2 {
					keyStr, ok := val[i].(string)
					if !ok {
						continue
					}
					key := transitMapKeyDecode(keyStr)
					m[key] = fromTransit(val[i+1])
				}
				return m
			}
		}
		// Regular array.
		result := make([]any, len(val))
		for i, item := range val {
			result[i] = fromTransit(item)
		}
		return result

	case map[string]any:
		// JSON-verbose Transit map.
		m := make(map[string]any, len(val))
		for k, v2 := range val {
			m[transitMapKeyDecode(k)] = fromTransit(v2)
		}
		return m

	case string:
		switch {
		case strings.HasPrefix(val, "~:"):
			return val[2:] // keyword → plain string
		case strings.HasPrefix(val, "~u"):
			return val[2:] // UUID
		case strings.HasPrefix(val, "~t"):
			return val[2:] // datetime
		case strings.HasPrefix(val, "~~"):
			return "~" + val[2:] // unescape
		case strings.HasPrefix(val, "~^"):
			return "^" + val[2:] // unescape
		}
		return val

	default:
		return v
	}
}
