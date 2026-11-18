// Command go-coordinator is a non-production framed fixture runner. It keeps
// one Rust planner process alive and performs no auth, storage, or writes.
package main

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"time"
)

func main() {
	path := os.Getenv("FG_C3_2_2_RUST_PLANNER")
	if path == "" {
		fmt.Fprintln(os.Stderr, "FG_C3_2_2_RUST_PLANNER is required")
		os.Exit(2)
	}
	if parseBenchmarkArgs(os.Args, path) {
		return
	}
	planner := NewPlanner(path)
	defer planner.Close()
	for {
		payload, err := readFrame(os.Stdin, MaxRequestBytes)
		if err == io.EOF {
			return
		}
		if err != nil {
			if errors.Is(err, errFrameTooLarge) {
				writeErrorFrame(os.Stdout, "REQUEST_TOO_LARGE", "/frame", "Request frame exceeds the coordinator limit.")
			} else {
				writeErrorFrame(os.Stdout, "FRAME_INVALID", "/frame", "Input frame is malformed.")
			}
			return
		}
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		response, callErr := planner.Call(ctx, payload)
		cancel()
		if callErr != nil {
			response = errorResponse(payload, "PLANNER_UNAVAILABLE", "Planner failed, timed out, or returned an invalid frame.")
		}
		if writeFrame(os.Stdout, response) != nil {
			return
		}
	}
}

func readFrame(reader io.Reader, max int) ([]byte, error) {
	var header [4]byte
	if _, err := io.ReadFull(reader, header[:]); err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(header[:])
	if uint64(n) > uint64(max) {
		return nil, errFrameTooLarge
	}
	body := make([]byte, int(n))
	if _, err := io.ReadFull(reader, body); err != nil {
		return nil, err
	}
	return body, nil
}

var errFrameTooLarge = errors.New("frame too large")

func writeFrame(writer io.Writer, body []byte) error {
	if len(body) > MaxResponseBytes {
		body = errorResponse(nil, "RESPONSE_TOO_LARGE", "Response exceeds the coordinator limit.")
	}
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(body)))
	if err := writeAll(writer, header[:]); err != nil {
		return err
	}
	return writeAll(writer, body)
}

func writeAll(writer io.Writer, body []byte) error {
	for len(body) > 0 {
		count, err := writer.Write(body)
		if err != nil {
			return err
		}
		if count == 0 {
			return io.ErrShortWrite
		}
		body = body[count:]
	}
	return nil
}

func writeErrorFrame(writer io.Writer, code, path, message string) {
	body, _ := json.Marshal(map[string]any{
		"protocolVersion":   2,
		"semanticVersion":   1,
		"diagnosticVersion": 1,
		"requestId":         nil,
		"ok":                false,
		"error":             map[string]string{"code": code, "path": path, "message": message},
	})
	_ = writeFrame(writer, body)
}

func errorResponse(request []byte, code, message string) []byte {
	var requestID any
	var envelope map[string]json.RawMessage
	if json.Unmarshal(request, &envelope) == nil {
		var id string
		if rawID, ok := envelope["requestId"]; ok && json.Unmarshal(rawID, &id) == nil && id != "" && len(id) <= 128 {
			requestID = id
		}
	}
	body, _ := json.Marshal(map[string]any{
		"protocolVersion":   2,
		"semanticVersion":   1,
		"diagnosticVersion": 1,
		"requestId":         requestID,
		"ok":                false,
		"error":             map[string]string{"code": code, "path": "/", "message": message},
	})
	return body
}
