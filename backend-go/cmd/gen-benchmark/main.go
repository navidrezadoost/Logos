// cmd/gen-benchmark — generate a large .logos fixture file for CI benchmarks.
//
// Replaces the former Clojure-based test/benchmarks/generate_file.clj generator.
//
// Usage:
//
//	go run ./cmd/gen-benchmark --output fixtures/large-canvas.logos [--pages 5] [--objects 500]
//
// The output is a valid .logos v3 ZIP archive containing N objects spread
// across P pages.  The memory benchmark script uploads the file to the running
// Logos instance and measures heap growth after the canvas renders.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"time"

	"github.com/logos-design/logos/backend-go/internal/binfile"
)

func main() {
	output := flag.String("output", "large-canvas.logos", "output .logos file path")
	pages := flag.Int("pages", 5, "number of pages")
	objects := flag.Int("objects", 500, "shapes per page")
	flag.Parse()

	if err := os.MkdirAll(filepath.Dir(*output), 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "mkdir: %v\n", err)
		os.Exit(1)
	}

	payload := buildPayload(*pages, *objects)

	f, err := os.Create(*output)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create: %v\n", err)
		os.Exit(1)
	}
	defer f.Close()

	if err = binfile.WriteZIP(f, payload); err != nil {
		fmt.Fprintf(os.Stderr, "write zip: %v\n", err)
		os.Exit(1)
	}

	fi, _ := f.Stat()
	fmt.Printf("Generated %s — %d pages × %d objects, %.1f KB\n",
		*output, *pages, *objects, float64(fi.Size())/1024)
}

func buildPayload(numPages, objsPerPage int) binfile.ExportPayload {
	rng := rand.New(rand.NewSource(time.Now().UnixNano()))

	fileID := newID(rng)
	now := time.Now().UTC()

	pageIDs := make([]string, numPages)
	for i := range pageIDs {
		pageIDs[i] = newID(rng)
	}

	attrs := binfile.FileAttrs{
		ID:         fileID,
		Name:       "large-canvas-benchmark",
		ModifiedAt: now,
	}

	changes := make([]binfile.ChangeRow, 0, numPages*objsPerPage)
	for pi, pageID := range pageIDs {
		for oi := 0; oi < objsPerPage; oi++ {
			op := buildAddObjChange(rng, pageID, pi, oi)
			raw, _ := json.Marshal([]any{op})
			revn := int64(pi*objsPerPage + oi + 1)
			changes = append(changes, binfile.ChangeRow{
				ID:        newID(rng),
				Revn:      revn,
				BaseRevn:  revn - 1,
				Changes:   raw,
				CreatedAt: now,
			})
		}
	}

	return binfile.ExportPayload{
		Attrs:   attrs,
		PageIDs: pageIDs,
		Changes: changes,
	}
}

// shape is the minimal Go representation of a design object for benchmarking.
type shape struct {
	ID       string  `json:"id"`
	Type     string  `json:"type"`
	Name     string  `json:"name"`
	PageID   string  `json:"pageId"`
	X        float64 `json:"x"`
	Y        float64 `json:"y"`
	Width    float64 `json:"width"`
	Height   float64 `json:"height"`
	Fill     string  `json:"fill"`
}

// changePayload wraps a single add-obj operation to satisfy the change format.
type changePayload struct {
	Type  string `json:"type"`
	ID    string `json:"id"`
	PageID string `json:"pageId"`
	Obj   shape  `json:"obj"`
}

func buildAddObjChange(rng *rand.Rand, pageID string, page, idx int) changePayload {
	types := []string{"rect", "circle", "text", "frame"}
	fills := []string{"#FF5733", "#33FF57", "#3357FF", "#F3FF33", "#FF33F3", "#33FFF3"}

	s := shape{
		ID:     newID(rng),
		Type:   types[rng.Intn(len(types))],
		Name:   fmt.Sprintf("shape-p%d-%d", page, idx),
		PageID: pageID,
		X:      rng.Float64() * 4000,
		Y:      rng.Float64() * 4000,
		Width:  20 + rng.Float64()*200,
		Height: 20 + rng.Float64()*200,
		Fill:   fills[rng.Intn(len(fills))],
	}

	return changePayload{
		Type:   "add-obj",
		ID:     s.ID,
		PageID: pageID,
		Obj:    s,
	}
}

func newID(rng *rand.Rand) string {
	b := make([]byte, 16)
	rng.Read(b) //nolint:gosec
	return fmt.Sprintf("%08x-%04x-%04x-%04x-%012x",
		b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}
