package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

type timingSummary struct {
	Count      int     `json:"count"`
	MinMs      float64 `json:"minMs"`
	MedianMs   float64 `json:"medianMs"`
	P95Ms      float64 `json:"p95Ms"`
	P99Ms      float64 `json:"p99Ms"`
	Throughput float64 `json:"throughputRequestsPerSecond"`
}

type benchmarkReport struct {
	Measurement   string        `json:"measurement"`
	Iterations    int           `json:"iterations"`
	Concurrency   int           `json:"concurrency"`
	ColdFirstMs   float64       `json:"persistentColdFirstRequestMs"`
	Persistent    timingSummary `json:"persistentWarmSequential"`
	OneShot       timingSummary `json:"oneShotSequential"`
	Concurrent    timingSummary `json:"persistentConcurrentCallers"`
	PersistentRSS int64         `json:"persistentPlannerPeakRSSKiB"`
	OneShotRSS    int64         `json:"oneShotPlannerPeakRSSKiB"`
}

func runBenchmark(path, fixturePath string, n, concurrency int) error {
	request, err := os.ReadFile(fixturePath)
	if err != nil {
		return err
	}
	if n < 20 || concurrency < 1 {
		return fmt.Errorf("benchmark requires at least 20 iterations and positive concurrency")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	report := benchmarkReport{Measurement: "local-fixture-only; not production capacity", Iterations: n, Concurrency: concurrency}
	p := NewPlanner(path)
	defer p.Close()
	start := time.Now()
	if _, err := p.Call(ctx, request); err != nil {
		return err
	}
	report.ColdFirstMs = float64(time.Since(start).Microseconds()) / 1000
	var peakRSS int64
	var rssMu sync.Mutex
	if rss, err := processRSSKiB(p.PID()); err == nil {
		peakRSS = rss
	}

	warm := make([]time.Duration, n)
	start = time.Now()
	for i := range warm {
		callStart := time.Now()
		if _, err := p.Call(ctx, request); err != nil {
			return fmt.Errorf("persistent warm request %d: %w", i, err)
		}
		warm[i] = time.Since(callStart)
		if rss, err := processRSSKiB(p.PID()); err == nil {
			rssMu.Lock()
			if rss > peakRSS {
				peakRSS = rss
			}
			rssMu.Unlock()
		}
	}
	report.Persistent = summarize(warm, time.Since(start))

	oneShot := make([]time.Duration, n)
	var oneShotRSS int64
	start = time.Now()
	for i := range oneShot {
		callStart := time.Now()
		if _, err := CallOneShot(ctx, path, request); err != nil {
			return fmt.Errorf("one-shot request %d: %w", i, err)
		}
		oneShot[i] = time.Since(callStart)
		if rss := lastOneShotPeakRSSKiB; rss > oneShotRSS {
			oneShotRSS = rss
		}
	}
	report.OneShot = summarize(oneShot, time.Since(start))
	report.PersistentRSS, report.OneShotRSS = peakRSS, oneShotRSS

	concurrentDurations := make([]time.Duration, n)
	var wg sync.WaitGroup
	var indexMu sync.Mutex
	index := 0
	start = time.Now()
	for worker := 0; worker < concurrency; worker++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				indexMu.Lock()
				i := index
				index++
				indexMu.Unlock()
				if i >= n {
					return
				}
				callStart := time.Now()
				if _, err := p.Call(ctx, request); err != nil {
					continue
				}
				concurrentDurations[i] = time.Since(callStart)
				if rss, err := processRSSKiB(p.PID()); err == nil {
					rssMu.Lock()
					if rss > peakRSS {
						peakRSS = rss
					}
					rssMu.Unlock()
				}
			}
		}()
	}
	wg.Wait()
	concurrentElapsed := time.Since(start)
	for _, duration := range concurrentDurations {
		if duration == 0 {
			return fmt.Errorf("concurrent benchmark had a failed request")
		}
	}
	report.Concurrent = summarize(concurrentDurations, concurrentElapsed)
	rssMu.Lock()
	report.PersistentRSS = peakRSS
	rssMu.Unlock()

	encoded, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	fmt.Println(string(encoded))
	return nil
}

var lastOneShotPeakRSSKiB int64

func processRSSKiB(pid int) (int64, error) {
	data, err := os.ReadFile(fmt.Sprintf("/proc/%d/status", pid))
	if err != nil {
		return 0, err
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "VmRSS:") {
			fields := strings.Fields(line)
			if len(fields) >= 2 {
				return strconv.ParseInt(fields[1], 10, 64)
			}
		}
	}
	return 0, fmt.Errorf("VmRSS unavailable")
}

func summarize(values []time.Duration, elapsed time.Duration) timingSummary {
	sorted := append([]time.Duration(nil), values...)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i] < sorted[j] })
	percentile := func(p float64) float64 {
		index := int(float64(len(sorted)-1) * p)
		return float64(sorted[index].Microseconds()) / 1000
	}
	return timingSummary{
		Count: len(values), MinMs: float64(sorted[0].Microseconds()) / 1000,
		MedianMs: percentile(0.50), P95Ms: percentile(0.95), P99Ms: percentile(0.99),
		Throughput: float64(len(values)) / elapsed.Seconds(),
	}
}

func parseBenchmarkArgs(args []string, path string) bool {
	if len(args) < 2 || args[1] != "benchmark" {
		return false
	}
	n := 200
	if value := os.Getenv("FG_C3_2_2_BENCH_ITERATIONS"); value != "" {
		if parsed, err := strconv.Atoi(value); err == nil {
			n = parsed
		}
	}
	concurrency := 8
	if value := os.Getenv("FG_C3_2_2_BENCH_CONCURRENCY"); value != "" {
		if parsed, err := strconv.Atoi(value); err == nil {
			concurrency = parsed
		}
	}
	if err := runBenchmark(path, args[2], n, concurrency); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	return true
}

func oneShotRSS(state *os.ProcessState) int64 {
	if state == nil || state.SysUsage() == nil {
		return 0
	}
	if usage, ok := state.SysUsage().(*syscall.Rusage); ok {
		return usage.Maxrss
	}
	return 0
}
