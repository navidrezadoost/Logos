package handler

import (
	"context"
	"crypto/tls"
	"errors"
	"net/http/httptest"
	"testing"
)

func TestWSCheckOrigin(t *testing.T) {
	tests := []struct {
		name           string
		origins        []string
		host           string
		tls            bool
		forwardedProto string
		want           bool
	}{
		{name: "legacy client without origin", host: "app.example", want: true},
		{name: "same host", origins: []string{"https://app.example"}, host: "app.example", tls: true, want: true},
		{name: "same host with default port", origins: []string{"https://app.example:443"}, host: "app.example", tls: true, want: true},
		{name: "scheme mismatch on plain HTTP", origins: []string{"https://app.example"}, host: "app.example", want: false},
		{name: "scheme mismatch on TLS", origins: []string{"http://app.example"}, host: "app.example", tls: true, want: false},
		{name: "trusted proxy HTTPS", origins: []string{"https://app.example"}, host: "app.example", forwardedProto: "https", want: true},
		{name: "different host", origins: []string{"https://attacker.example"}, host: "app.example", want: false},
		{name: "different port", origins: []string{"https://app.example:444"}, host: "app.example", tls: true, want: false},
		{name: "opaque origin", origins: []string{"null"}, host: "app.example", want: false},
		{name: "origin with path", origins: []string{"https://app.example/evil"}, host: "app.example", want: false},
		{name: "multiple origins", origins: []string{"https://app.example", "https://attacker.example"}, host: "app.example", want: false},
		{name: "empty origin value", origins: []string{""}, host: "app.example", want: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest("GET", "http://"+tt.host+"/ws/notifications", nil)
			req.Host = tt.host
			if tt.tls {
				req.TLS = &tls.ConnectionState{}
			}
			if tt.forwardedProto != "" {
				req.Header.Set("X-Forwarded-Proto", tt.forwardedProto)
			}
			for _, origin := range tt.origins {
				req.Header.Add("Origin", origin)
			}
			if got := wsCheckOrigin(req); got != tt.want {
				t.Fatalf("wsCheckOrigin() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestNotificationResourceClassificationRejectsUUIDNamespaceConfusion(t *testing.T) {
	sharedID := "file-and-team-id"
	tests := []struct {
		name         string
		subscription wsResource
		message      map[string]any
		want         wsResource
		wantOK       bool
	}{
		{
			name:         "team library event on colliding file channel",
			subscription: wsResource{kind: wsFileResource, id: sharedID},
			message:      map[string]any{"type": "library-change", "fileId": "library-file"},
			want:         wsResource{kind: wsTeamResource, id: sharedID},
			wantOK:       true,
		},
		{
			name:         "file event on colliding team channel",
			subscription: wsResource{kind: wsTeamResource, id: sharedID},
			message:      map[string]any{"type": "file-change", "fileId": sharedID},
			want:         wsResource{kind: wsFileResource, id: sharedID},
			wantOK:       true,
		},
		{
			name:         "unclassified resource message",
			subscription: wsResource{kind: wsFileResource, id: sharedID},
			message:      map[string]any{"type": "unknown-event"},
			wantOK:       false,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := notificationResource(tt.subscription, tt.message)
			if ok != tt.wantOK || got != tt.want {
				t.Fatalf("notificationResource() = %#v, %v; want %#v, %v", got, ok, tt.want, tt.wantOK)
			}
		})
	}
}

func TestWSResourceAuthorizationFailsClosedWithoutPool(t *testing.T) {
	s := &wsSession{profileID: "profile"}
	if s.authorized(context.Background(), wsResource{kind: wsFileResource, id: "file"}) {
		t.Fatal("file subscription was authorized without a permission backend")
	}
	if s.authorized(context.Background(), wsResource{kind: wsTeamResource, id: "team"}) {
		t.Fatal("team subscription was authorized without a permission backend")
	}
}

func TestDecodeCompetingChangeSetRejectsMalformedHistory(t *testing.T) {
	for _, raw := range [][]byte{
		nil,
		[]byte("not-json"),
		[]byte("null"),
		[]byte(`[{"id":"missing-type"}]`),
		[]byte(`[{"type":]`),
	} {
		_, err := decodeCompetingChangeSet("file-id", 17, raw)
		var historyErr *CompetingChangeHistoryError
		if !errors.As(err, &historyErr) {
			t.Errorf("decodeCompetingChangeSet(%q) error = %v, want CompetingChangeHistoryError", raw, err)
			continue
		}
		if historyErr.FileID != "file-id" || historyErr.Revn != 17 {
			t.Errorf("typed history error has file=%q revn=%d, want file-id/17", historyErr.FileID, historyErr.Revn)
		}
	}
}

func TestDecodeCompetingChangeSetAcceptsJSONArray(t *testing.T) {
	changes, err := decodeCompetingChangeSet("file-id", 1, []byte(`[{"type":"del-obj","id":"shape"}]`))
	if err != nil {
		t.Fatalf("decodeCompetingChangeSet: %v", err)
	}
	if len(changes) != 1 || changes[0].Type != "del-obj" {
		t.Fatalf("decoded changes = %#v, want one del-obj", changes)
	}
	if changes, err = decodeCompetingChangeSet("file-id", 2, []byte("[]")); err != nil || len(changes) != 0 {
		t.Fatalf("empty change-set decode = %#v, %v; want empty set and nil error", changes, err)
	}
}
