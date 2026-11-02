package handler

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"strconv"
)

// rpcParam reads an RPC parameter from the query string or JSON request body.
// The compiled frontend POSTs Transit maps (decoded to JSON objects) to
// /api/main/methods/*; legacy handlers only checked URL query params.
func rpcParam(r *http.Request, keys ...string) string {
	for _, key := range keys {
		if v := r.URL.Query().Get(key); v != "" {
			return v
		}
	}

	if r.Method == http.MethodGet || r.Method == http.MethodHead {
		return ""
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		return ""
	}
	r.Body = io.NopCloser(bytes.NewReader(body))
	if len(body) == 0 {
		return ""
	}

	var params map[string]any
	if err := json.Unmarshal(body, &params); err != nil {
		return ""
	}

	for _, key := range keys {
		if v, ok := params[key]; ok {
			if s := scalarString(v); s != "" {
				return s
			}
		}
	}
	return ""
}

func scalarString(v any) string {
	switch val := v.(type) {
	case string:
		return val
	case json.Number:
		return val.String()
	case bool:
		if val {
			return "true"
		}
		return "false"
	default:
		return ""
	}
}

// jsonFieldString reads the first present string-like field from a decoded JSON body.
func jsonFieldString(params map[string]any, keys ...string) string {
	for _, key := range keys {
		if v, ok := params[key]; ok {
			if s := scalarString(v); s != "" {
				return s
			}
		}
	}
	return ""
}

func jsonFieldBool(params map[string]any, keys ...string) bool {
	for _, key := range keys {
		if v, ok := params[key]; ok {
			switch val := v.(type) {
			case bool:
				return val
			case string:
				return val == "true"
			}
		}
	}
	return false
}

// jsonFieldStringSlice reads a string slice from plain arrays or Transit sets.
func jsonFieldStringSlice(params map[string]any, keys ...string) []string {
	for _, key := range keys {
		if v, ok := params[key]; ok {
			if ss := coerceStringSlice(v); len(ss) > 0 {
				return ss
			}
		}
	}
	return nil
}

func coerceStringSlice(v any) []string {
	switch val := v.(type) {
	case []string:
		return val
	case []any:
		out := make([]string, 0, len(val))
		for _, item := range val {
			if s := scalarString(item); s != "" {
				out = append(out, s)
			}
		}
		return out
	case map[string]any:
		for _, k := range []string{"~#set", "#set", "set"} {
			if arr, ok := val[k]; ok {
				return coerceStringSlice(arr)
			}
		}
	}
	return nil
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return ""
}

func jsonFieldInt64(params map[string]any, keys ...string) int64 {
	for _, key := range keys {
		if v, ok := params[key]; ok {
			switch val := v.(type) {
			case float64:
				return int64(val)
			case json.Number:
				i, _ := val.Int64()
				return i
			case int64:
				return val
			case int:
				return int64(val)
			case string:
				i, _ := strconv.ParseInt(val, 10, 64)
				return i
			}
		}
	}
	return 0
}
