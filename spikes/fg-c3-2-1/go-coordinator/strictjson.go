package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"unicode/utf8"
)

var errDuplicateMember = errors.New("duplicate decoded JSON object member")
var errJSONDepthLimit = errors.New("JSON nesting limit exceeded")
var errJSONWorkLimit = errors.New("JSON value work limit exceeded")

const (
	maxJSONDepth  = 64
	maxJSONValues = 100_000
)

// scanUniqueJSON rejects duplicate decoded member names before map decoding can
// discard them. Decoder.Token resolves JSON escapes, so "a" and "\u0061"
// collide. It also bounds recursive token walking and total values.
func scanUniqueJSON(raw []byte) error {
	if !utf8.Valid(raw) {
		return errors.New("JSON input is not UTF-8")
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	values := 0
	if err := scanJSONValue(decoder, 0, &values); err != nil {
		return err
	}
	if _, err := decoder.Token(); err != io.EOF {
		return errors.New("trailing or malformed JSON")
	}
	return nil
}

func scanJSONValue(decoder *json.Decoder, depth int, values *int) error {
	*values++
	if *values > maxJSONValues {
		return errJSONWorkLimit
	}
	token, err := decoder.Token()
	if err != nil {
		return err
	}
	delimiter, ok := token.(json.Delim)
	if !ok {
		return nil
	}
	if delimiter != '{' && delimiter != '[' {
		return errors.New("unexpected closing delimiter")
	}
	depth++
	if depth > maxJSONDepth {
		return errJSONDepthLimit
	}
	if delimiter == '{' {
		seen := make(map[string]struct{})
		for decoder.More() {
			keyToken, err := decoder.Token()
			if err != nil {
				return err
			}
			key, ok := keyToken.(string)
			if !ok {
				return errors.New("object member name is not a string")
			}
			if _, exists := seen[key]; exists {
				return errDuplicateMember
			}
			seen[key] = struct{}{}
			if err := scanJSONValue(decoder, depth, values); err != nil {
				return err
			}
		}
		end, err := decoder.Token()
		if err != nil || end != json.Delim('}') {
			return errors.New("object ended unexpectedly")
		}
		return nil
	}
	for decoder.More() {
		if err := scanJSONValue(decoder, depth, values); err != nil {
			return err
		}
	}
	end, err := decoder.Token()
	if err != nil || end != json.Delim(']') {
		return errors.New("array ended unexpectedly")
	}
	return nil
}
