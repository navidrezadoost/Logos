// Command go-coordinator is a non-production fixture runner. It demonstrates
// Go invoking the isolated Rust planner process; it performs no auth, storage,
// revision mutation, or production RPC.
package main

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"time"
)

func invokeRustPlanner(input []byte) ([]byte, error) {
	path := os.Getenv("FG_C3_2_1_RUST_PLANNER")
	if path == "" {
		return nil, fmt.Errorf("FG_C3_2_1_RUST_PLANNER is required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, path)
	cmd.Stdin = bytes.NewReader(input)
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("isolated planner failed (stderr bytes=%d): %w", stderr.Len(), err)
	}
	return stdout.Bytes(), nil
}

func main() {
	input, err := io.ReadAll(io.LimitReader(os.Stdin, 32<<20))
	if err != nil {
		fmt.Fprintln(os.Stderr, "request read failed")
		os.Exit(2)
	}
	output, err := invokeRustPlanner(input)
	if err != nil {
		fmt.Fprintln(os.Stderr, "Rust planner invocation failed")
		os.Exit(3)
	}
	_, _ = os.Stdout.Write(output)
}
