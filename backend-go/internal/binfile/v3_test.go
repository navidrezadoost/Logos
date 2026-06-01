package binfile_test

import (
	"bytes"
	"encoding/json"
	"testing"
	"time"

	"github.com/logos-design/logos/backend-go/internal/binfile"
	"github.com/logos-design/logos/backend-go/internal/rebase"
)

// ─── Round-trip tests ─────────────────────────────────────────────────────────

func TestV3RoundTripEmptyFile(t *testing.T) {
	payload := binfile.ExportPayload{
		Attrs: binfile.FileAttrs{
			ID:         "aaaaaaaa-0000-0000-0000-000000000001",
			Name:       "Round-trip test",
			ProjectID:  "bbbbbbbb-0000-0000-0000-000000000002",
			Revn:       0,
			Vern:       0,
			ModifiedAt: time.Now().UTC().Truncate(time.Second),
		},
		PageIDs: []string{},
		Changes: []binfile.ChangeRow{},
		Media:   []binfile.MediaMeta{},
		Objects: []binfile.ExportObject{},
	}

	var buf bytes.Buffer
	if err := binfile.WriteZIP(&buf, payload); err != nil {
		t.Fatalf("WriteZIP: %v", err)
	}

	got, err := binfile.ReadZIP(buf.Bytes())
	if err != nil {
		t.Fatalf("ReadZIP: %v", err)
	}

	if len(got.Files) != 1 {
		t.Fatalf("expected 1 file, got %d", len(got.Files))
	}
	f := got.Files[0]
	if f.Attrs.ID != payload.Attrs.ID {
		t.Errorf("ID: want %s, got %s", payload.Attrs.ID, f.Attrs.ID)
	}
	if f.Attrs.Name != payload.Attrs.Name {
		t.Errorf("Name: want %s, got %s", payload.Attrs.Name, f.Attrs.Name)
	}
	if f.Attrs.ProjectID != payload.Attrs.ProjectID {
		t.Errorf("ProjectID: want %s, got %s", payload.Attrs.ProjectID, f.Attrs.ProjectID)
	}
}

func TestV3RoundTripWithChanges(t *testing.T) {
	// Create two change rows: one mod-obj and one del-obj.
	changes1, _ := json.Marshal([]rebase.Change{
		{Type: rebase.TypeModObj, ID: "shape-1", PageID: "page-1",
			Operations: []rebase.SetOp{{Attr: "name", Val: json.RawMessage(`"Rect A"`)}}},
	})
	changes2, _ := json.Marshal([]rebase.Change{
		{Type: rebase.TypeDelObj, ID: "shape-2", PageID: "page-1"},
	})

	payload := binfile.ExportPayload{
		Attrs: binfile.FileAttrs{
			ID: "cccccccc-0000-0000-0000-000000000003", Name: "Change test",
			ProjectID: "dddddddd-0000-0000-0000-000000000004",
			Revn:      2, ModifiedAt: time.Now().UTC(),
		},
		PageIDs: []string{"page-1"},
		Changes: []binfile.ChangeRow{
			{ID: "rc-1", Revn: 1, Changes: json.RawMessage(changes1)},
			{ID: "rc-2", Revn: 2, Changes: json.RawMessage(changes2)},
		},
		Media: []binfile.MediaMeta{},
	}

	var buf bytes.Buffer
	if err := binfile.WriteZIP(&buf, payload); err != nil {
		t.Fatalf("WriteZIP: %v", err)
	}

	got, err := binfile.ReadZIP(buf.Bytes())
	if err != nil {
		t.Fatalf("ReadZIP: %v", err)
	}

	pf := got.Files[0]

	// Page IDs preserved.
	if len(pf.PageIDs) != 1 || pf.PageIDs[0] != "page-1" {
		t.Errorf("page IDs: want [page-1], got %v", pf.PageIDs)
	}

	// Change rows preserved (count + revn).
	if len(pf.Changes) != 2 {
		t.Fatalf("expected 2 change rows, got %d", len(pf.Changes))
	}
	if pf.Changes[0].Revn != 1 || pf.Changes[1].Revn != 2 {
		t.Errorf("revn mismatch: %d, %d", pf.Changes[0].Revn, pf.Changes[1].Revn)
	}

	// Changes are parseable after round-trip.
	var cs1 []rebase.Change
	if err := json.Unmarshal(pf.Changes[0].Changes, &cs1); err != nil {
		t.Errorf("parse changes[0]: %v", err)
	}
	if len(cs1) != 1 || cs1[0].Type != rebase.TypeModObj {
		t.Errorf("unexpected change type: %v", cs1)
	}
}

func TestV3RoundTripWithMedia(t *testing.T) {
	thumbID := "ee000000-0000-0000-0000-000000000005"
	payload := binfile.ExportPayload{
		Attrs: binfile.FileAttrs{
			ID: "ff000000-0000-0000-0000-000000000006", Name: "Media test",
			ProjectID: "00111111-0000-0000-0000-000000000007",
		},
		Media: []binfile.MediaMeta{{
			ID:          "media-row-1",
			FileID:      "ff000000-0000-0000-0000-000000000006",
			Name:        "logo.png",
			MediaID:     "00222222-0000-0000-0000-000000000008",
			ThumbnailID: &thumbID,
			Width:       200,
			Height:      100,
			Mtype:       "image/png",
			IsLocal:     true,
		}},
		Objects: []binfile.ExportObject{{
			ID:          "00222222-0000-0000-0000-000000000008",
			Bucket:      "file-media-object",
			ContentType: "image/png",
			Data:        []byte("\x89PNG\r\n\x1a\n"), // PNG magic bytes
		}},
	}

	var buf bytes.Buffer
	if err := binfile.WriteZIP(&buf, payload); err != nil {
		t.Fatalf("WriteZIP: %v", err)
	}

	got, err := binfile.ReadZIP(buf.Bytes())
	if err != nil {
		t.Fatalf("ReadZIP: %v", err)
	}

	pf := got.Files[0]
	if len(pf.Media) != 1 {
		t.Fatalf("expected 1 media entry, got %d", len(pf.Media))
	}
	m := pf.Media[0]
	if m.Name != "logo.png" || m.Width != 200 || m.Height != 100 {
		t.Errorf("media meta mismatch: %+v", m)
	}

	if len(pf.Objects) != 1 {
		t.Fatalf("expected 1 object, got %d", len(pf.Objects))
	}
	obj := pf.Objects[0]
	if obj.Meta.ContentType != "image/png" {
		t.Errorf("content-type: want image/png, got %s", obj.Meta.ContentType)
	}
	if !bytes.HasPrefix(obj.Data, []byte("\x89PNG")) {
		t.Errorf("PNG magic bytes not preserved")
	}
}

func TestV3RoundTripWithRawData(t *testing.T) {
	// Simulate a Clojure-managed file whose CRDT blob must survive the round-trip.
	rawBlob := []byte("mock-clojure-transit+zstd-blob")

	payload := binfile.ExportPayload{
		Attrs: binfile.FileAttrs{
			ID: "aa100000-0000-0000-0000-000000000001", Name: "Clojure file",
		},
		RawData: rawBlob,
	}

	var buf bytes.Buffer
	if err := binfile.WriteZIP(&buf, payload); err != nil {
		t.Fatalf("WriteZIP: %v", err)
	}

	got, err := binfile.ReadZIP(buf.Bytes())
	if err != nil {
		t.Fatalf("ReadZIP: %v", err)
	}

	if !bytes.Equal(got.Files[0].RawData, rawBlob) {
		t.Errorf("RawData not preserved: got %q, want %q",
			got.Files[0].RawData, rawBlob)
	}
}

func TestV3ManifestFields(t *testing.T) {
	payload := binfile.ExportPayload{
		Attrs: binfile.FileAttrs{
			ID: "bb100000-0000-0000-0000-000000000002", Name: "Manifest test",
		},
	}
	var buf bytes.Buffer
	if err := binfile.WriteZIP(&buf, payload); err != nil {
		t.Fatalf("WriteZIP: %v", err)
	}
	got, err := binfile.ReadZIP(buf.Bytes())
	if err != nil {
		t.Fatalf("ReadZIP: %v", err)
	}
	if got.Manifest.Type != "logos/export-files" {
		t.Errorf("manifest type: %s", got.Manifest.Type)
	}
	if got.Manifest.Version != 1 {
		t.Errorf("manifest version: %d", got.Manifest.Version)
	}
	if !got.Manifest.GoExtension {
		t.Error("go-extension flag should be true")
	}
	if got.Manifest.GeneratedBy == "" {
		t.Error("generated-by should be non-empty")
	}
}

func TestParseFormatV3(t *testing.T) {
	header := []byte{'P', 'K', 0x03, 0x04, 0x00, 0x00}
	if binfile.ParseFormat(header) != binfile.FormatV3 {
		t.Error("expected FormatV3 for ZIP magic")
	}
}

func TestParseFormatV1(t *testing.T) {
	header := []byte{0x80, 0x00, 0x99, 0x56}
	if binfile.ParseFormat(header) != binfile.FormatV1 {
		t.Error("expected FormatV1 for non-ZIP bytes")
	}
}

func TestReadZIPV1ReturnsError(t *testing.T) {
	v1bytes := []byte{0x80, 0x00, 0x99, 0x56, 0x00}
	_, err := binfile.ReadZIP(v1bytes)
	if err == nil {
		t.Error("expected error for v1 bytes")
	}
	if err != binfile.ErrFormatV1NotSupported {
		t.Errorf("expected ErrFormatV1NotSupported, got %v", err)
	}
}
