package changes

import "strings"

// Unwrap expands Logos Transit tagged values (~#shape, ~#matrix, ~#point, …)
// into plain JSON maps and slices suitable for storage in file.data.
func Unwrap(v any) any {
	switch val := v.(type) {
	case map[string]any:
		if len(val) == 1 {
			for k, inner := range val {
				if strings.HasPrefix(k, "~#") {
					return Unwrap(inner)
				}
			}
		}
		out := make(map[string]any, len(val))
		for k, inner := range val {
			out[k] = Unwrap(inner)
		}
		return out
	case []any:
		out := make([]any, len(val))
		for i, item := range val {
			out[i] = Unwrap(item)
		}
		return out
	default:
		return v
	}
}
