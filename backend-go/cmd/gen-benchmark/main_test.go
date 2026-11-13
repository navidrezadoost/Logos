package main

import (
	"bytes"
	"testing"

	"github.com/logos-design/logos/backend-go/internal/binfile"
)

func TestBuildPayloadProducesReadableBenchmarkArchive(t *testing.T) {
	const pages, objectsPerPage = 5, 3
	payload := buildPayload(pages, objectsPerPage)

	var archive bytes.Buffer
	if err := binfile.WriteZIP(&archive, payload); err != nil {
		t.Fatalf("WriteZIP: %v", err)
	}

	parsed, err := binfile.ReadZIP(archive.Bytes())
	if err != nil {
		t.Fatalf("ReadZIP generated benchmark fixture: %v", err)
	}
	if len(parsed.Files) != 1 {
		t.Fatalf("parsed %d files, want 1", len(parsed.Files))
	}
	if got := len(parsed.Files[0].PageIDs); got != pages {
		t.Errorf("parsed %d page IDs, want %d", got, pages)
	}
	if got, want := len(parsed.Files[0].Changes), pages*objectsPerPage; got != want {
		t.Errorf("parsed %d change rows, want %d", got, want)
	}
}
