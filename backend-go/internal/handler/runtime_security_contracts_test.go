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
		allowedOrigins []string
		want           bool
	}{
		{name: "missing origin rejected by default", host: "app.example", want: false},
		{name: "same host", origins: []string{"https://app.example"}, host: "app.example", tls: true, want: true},
		{name: "same host with default port", origins: []string{"https://app.example:443"}, host: "app.example", tls: true, want: true},
		{name: "scheme mismatch on plain HTTP", origins: []string{"https://app.example"}, host: "app.example", want: false},
		{name: "scheme mismatch on TLS", origins: []string{"http://app.example"}, host: "app.example", tls: true, want: false},
		{name: "proxy HTTPS rejected without explicit allowlist", origins: []string{"https://app.example"}, host: "backend.internal", forwardedProto: "https", want: false},
		{name: "proxy HTTPS accepted by exact allowlist", origins: []string{"https://app.example"}, host: "backend.internal", allowedOrigins: []string{"https://app.example"}, want: true},
		{name: "proxy HTTPS rejects malformed allowlist entry", origins: []string{"https://app.example"}, host: "backend.internal", allowedOrigins: []string{"https://app.example/path"}, want: false},
		{name: "proxy HTTPS skips malformed entries but accepts exact valid entry", origins: []string{"https://app.example"}, host: "backend.internal", allowedOrigins: []string{"not-an-origin", "https://app.example?unexpected=query", "https://app.example"}, want: true},
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
			if got := wsCheckOrigin(req, tt.allowedOrigins); got != tt.want {
				t.Fatalf("wsCheckOrigin() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestWSOutboundDeliveryFailsClosedAfterUnsubscribe(t *testing.T) {
	resource := wsResource{kind: wsFileResource, id: "file-id"}
	channel := msgbusTopic(resource.id)
	s := &wsSession{
		profileID:         "profile-id",
		send:              make(chan wsOutboundMessage, 1),
		subscribed:        map[string]struct{}{channel: {}},
		resources:         map[string]wsResource{channel: resource},
		channelGeneration: map[string]uint64{channel: 4},
	}
	queued := wsOutboundMessage{
		channel:         channel,
		resource:        resource,
		resourceChannel: true,
		generation:      4,
		payload:         []byte("queued before unsubscribe"),
	}

	// No permission backend means the pre-enqueue authorization fails closed,
	// and revokes the channel while retaining its classification for in-flight
	// messages that were already received from Redis.
	if s.enqueueOutbound(context.Background(), queued) {
		t.Fatal("queued a resource notification without a permission backend")
	}
	if len(s.subscribed) != 0 {
		t.Fatalf("subscription remained active after failed authorization: %v", s.subscribed)
	}
	if got, ok := s.resources[channel]; !ok || got != resource {
		t.Fatalf("resource classification = %#v, %v; want retained %#v", got, ok, resource)
	}

	// Simulate a message that was already queued before unsubscribe, then a fast
	// resubscribe to the same channel. The old generation must remain invalid;
	// the writer cannot trust that the raw channel is active again.
	s.subscribed[channel] = struct{}{}
	s.channelGeneration[channel]++
	s.send <- queued
	if !s.writeOutboundMessage(<-s.send) {
		t.Fatal("stale-generation queued message should be dropped without a write error")
	}
	if len(s.send) != 0 {
		t.Fatal("inactive queued message was re-enqueued")
	}
}

func TestWSOutboundDeliveryDropsUnknownNonSystemChannel(t *testing.T) {
	channel := msgbusTopic("unclassified")
	s := &wsSession{
		profileID:         "profile-id",
		send:              make(chan wsOutboundMessage, 1),
		subscribed:        map[string]struct{}{channel: {}},
		resources:         map[string]wsResource{},
		channelGeneration: map[string]uint64{channel: 1},
	}
	if s.enqueueOutbound(context.Background(), wsOutboundMessage{
		channel:    channel,
		generation: 1,
		payload:    []byte("unknown"),
	}) {
		t.Fatal("unknown non-profile/system channel was queued")
	}
	if len(s.send) != 0 {
		t.Fatal("unknown channel left a message in the outbound queue")
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
