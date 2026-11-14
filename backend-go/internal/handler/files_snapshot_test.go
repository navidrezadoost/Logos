package handler

import (
	"bytes"
	"errors"
	"testing"
)

func TestVerifySnapshotBlobStatuses(t *testing.T) {
	content := []byte("snapshot bytes")
	validMetadata, err := integrityMetadata(content)
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name       string
		data       []byte
		metadata   []byte
		wantStatus string
		wantErr    error
	}{
		{name: "verified", data: content, metadata: validMetadata, wantStatus: snapshotStatusVerified},
		{name: "legacy content", data: content, metadata: []byte(`{}`), wantStatus: snapshotStatusLegacyUnverified},
		{name: "missing content", wantStatus: snapshotStatusUnavailable, wantErr: errSnapshotContentUnavailable},
		{name: "malformed metadata json", data: content, metadata: []byte(`{"snapshot-integrity":`), wantStatus: snapshotStatusCorrupt, wantErr: errSnapshotContentCorrupt},
		{name: "malformed integrity envelope", data: content, metadata: []byte(`{"snapshot-integrity":"nope"}`), wantStatus: snapshotStatusCorrupt, wantErr: errSnapshotContentCorrupt},
		{name: "unsupported version", data: content, metadata: []byte(`{"snapshot-integrity":{"version":2,"algorithm":"sha256","digest":"0000000000000000000000000000000000000000000000000000000000000000"}}`), wantStatus: snapshotStatusCorrupt, wantErr: errSnapshotContentCorrupt},
		{name: "unsupported algorithm", data: content, metadata: []byte(`{"snapshot-integrity":{"version":1,"algorithm":"sha1","digest":"0000000000000000000000000000000000000000000000000000000000000000"}}`), wantStatus: snapshotStatusCorrupt, wantErr: errSnapshotContentCorrupt},
		{name: "mismatched digest", data: content, metadata: []byte(`{"snapshot-integrity":{"version":1,"algorithm":"sha256","digest":"0000000000000000000000000000000000000000000000000000000000000000"}}`), wantStatus: snapshotStatusCorrupt, wantErr: errSnapshotContentCorrupt},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			status, verifyErr := verifySnapshotBlob(tc.data, tc.metadata)
			if status != tc.wantStatus {
				t.Fatalf("status=%q, want %q", status, tc.wantStatus)
			}
			if !errors.Is(verifyErr, tc.wantErr) {
				t.Fatalf("error=%v, want %v", verifyErr, tc.wantErr)
			}
			if tc.wantStatus == snapshotStatusVerified && !bytes.Equal(tc.data, content) {
				t.Fatal("verified content changed")
			}
		})
	}
}
