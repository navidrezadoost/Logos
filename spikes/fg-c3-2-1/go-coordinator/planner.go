package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"sync"
)

const (
	MaxRequestBytes  = 1 << 20
	MaxResponseBytes = 1 << 20
)

type Planner struct {
	Path  string
	Args  []string
	Env   []string
	gate  chan struct{}
	cmd   *exec.Cmd
	in    io.WriteCloser
	out   *bufio.Reader
	wait  chan error
	pid   int
	ready bool
}

func NewPlanner(path string) *Planner { return &Planner{Path: path, gate: make(chan struct{}, 1)} }

func (p *Planner) Call(ctx context.Context, request []byte) ([]byte, error) {
	if len(request) > MaxRequestBytes {
		return errorResponse(request, "REQUEST_TOO_LARGE", "Request exceeds the coordinator limit."), nil
	}
	select {
	case p.gate <- struct{}{}:
		defer func() { <-p.gate }()
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	default:
	}
	if err := p.start(ctx); err != nil {
		return nil, err
	}
	type result struct {
		body []byte
		err  error
	}
	done := make(chan result, 1)
	plannerIn, plannerOut := p.in, p.out
	go func() {
		body, err := exchange(plannerIn, plannerOut, request)
		done <- result{body: body, err: err}
	}()
	var got result
	select {
	case got = <-done:
	case <-ctx.Done():
		p.stop()
		got = <-done
		return nil, ctx.Err()
	}
	if got.err != nil {
		p.stop()
		return nil, got.err
	}
	if err := validateResponse(request, got.body); err != nil {
		p.stop()
		return nil, fmt.Errorf("planner response rejected: %w", err)
	}
	return got.body, nil
}

func (p *Planner) start(ctx context.Context) error {
	if p.cmd != nil && p.ready {
		return nil
	}
	if p.Path == "" {
		return errors.New("planner executable path is empty")
	}
	cmd := exec.Command(p.Path, p.Args...)
	cmd.Env = append(os.Environ(), p.Env...)
	in, err := cmd.StdinPipe()
	if err != nil {
		return fmt.Errorf("planner stdin: %w", err)
	}
	out, err := cmd.StdoutPipe()
	if err != nil {
		return fmt.Errorf("planner stdout: %w", err)
	}
	cmd.Stderr = &boundedBuffer{limit: 4096}
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("planner start failed: %w", err)
	}
	waitCh := make(chan error, 1)
	p.cmd, p.in, p.out, p.wait, p.pid = cmd, in, bufio.NewReader(out), waitCh, cmd.Process.Pid
	go func() { waitCh <- cmd.Wait() }()
	capabilityRequest := []byte(fmt.Sprintf(`{"protocolVersion":2,"semanticVersion":1,"diagnosticVersion":1,"requestId":"startup-%d","kind":"capabilities"}`, p.pid))
	type result struct {
		body []byte
		err  error
	}
	ready := make(chan result, 1)
	plannerIn, plannerOut := p.in, p.out
	go func() { body, err := exchange(plannerIn, plannerOut, capabilityRequest); ready <- result{body, err} }()
	select {
	case got := <-ready:
		if got.err == nil {
			got.err = validateResponse(capabilityRequest, got.body)
		}
		if got.err == nil {
			var status struct {
				OK bool `json:"ok"`
			}
			if err := json.Unmarshal(got.body, &status); err != nil || !status.OK {
				got.err = errors.New("planner did not affirm capability readiness")
			}
		}
		if got.err != nil {
			p.stop()
			return fmt.Errorf("planner capability handshake failed: %w", got.err)
		}
		p.ready = true
	case <-ctx.Done():
		p.stop()
		return ctx.Err()
	}
	return nil
}

func exchange(input io.Writer, output *bufio.Reader, request []byte) ([]byte, error) {
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(request)))
	if _, err := input.Write(header[:]); err != nil {
		return nil, fmt.Errorf("planner request header write: %w", err)
	}
	if _, err := input.Write(request); err != nil {
		return nil, fmt.Errorf("planner request body write: %w", err)
	}
	if _, err := io.ReadFull(output, header[:]); err != nil {
		return nil, fmt.Errorf("planner response header read: %w", err)
	}
	n := binary.BigEndian.Uint32(header[:])
	if n > MaxResponseBytes {
		return nil, fmt.Errorf("planner response frame exceeds %d bytes", MaxResponseBytes)
	}
	response := make([]byte, int(n))
	if _, err := io.ReadFull(output, response); err != nil {
		return nil, fmt.Errorf("planner response body read: %w", err)
	}
	return response, nil
}

func (p *Planner) stop() {
	if p.cmd == nil {
		return
	}
	cmd, input, wait := p.cmd, p.in, p.wait
	p.cmd, p.in, p.out, p.wait, p.pid, p.ready = nil, nil, nil, nil, 0, false
	_ = input.Close()
	_ = cmd.Process.Kill()
	<-wait
}

func (p *Planner) Close()   { p.stop() }
func (p *Planner) PID() int { return p.pid }

type boundedBuffer struct {
	mu        sync.Mutex
	value     bytes.Buffer
	limit     int
	truncated bool
}

func (b *boundedBuffer) Write(data []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	remaining := b.limit - b.value.Len()
	if remaining > 0 {
		if len(data) > remaining {
			_, _ = b.value.Write(data[:remaining])
		} else {
			_, _ = b.value.Write(data)
		}
	}
	if len(data) > remaining {
		b.truncated = true
	}
	return len(data), nil
}

func (b *boundedBuffer) Truncated() bool { b.mu.Lock(); defer b.mu.Unlock(); return b.truncated }

func CallOneShot(ctx context.Context, path string, request []byte) ([]byte, error) {
	if len(request) > MaxRequestBytes {
		return nil, errors.New("request exceeds coordinator limit")
	}
	var framed bytes.Buffer
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(request)))
	framed.Write(header[:])
	framed.Write(request)
	cmd := exec.CommandContext(ctx, path)
	cmd.Stdin = &framed
	output := &boundedBuffer{limit: MaxResponseBytes + 4}
	cmd.Stdout = output
	cmd.Stderr = &boundedBuffer{limit: 4096}
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("one-shot planner failed: %w", err)
	}
	if output.Truncated() {
		return nil, fmt.Errorf("one-shot planner output exceeds %d bytes", MaxResponseBytes+4)
	}
	lastOneShotPeakRSSKiB = oneShotRSS(cmd.ProcessState)
	response, err := readFrame(bytes.NewReader(output.value.Bytes()), MaxResponseBytes)
	if err != nil {
		return nil, err
	}
	if err := validateResponse(request, response); err != nil {
		return nil, fmt.Errorf("one-shot planner response rejected: %w", err)
	}
	return response, nil
}

func frame(body []byte) []byte {
	result := make([]byte, 4+len(body))
	binary.BigEndian.PutUint32(result[:4], uint32(len(body)))
	copy(result[4:], body)
	return result
}

func readRawFrame(reader io.Reader, max int) ([]byte, error) { return readFrame(reader, max) }
