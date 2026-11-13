package handler

import (
	"context"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	"github.com/redis/go-redis/v9"
)

func TestNotificationsWebSocketMissedPongCleanupIntegration(t *testing.T) {
	redisURL := os.Getenv("TEST_REDIS_URL")
	if redisURL == "" {
		t.Skip("TEST_REDIS_URL not set; skipping Redis lifecycle integration test")
	}
	options, err := redis.ParseURL(redisURL)
	if err != nil {
		t.Fatalf("parse TEST_REDIS_URL: %v", err)
	}
	rdb := redis.NewClient(options)
	if err := rdb.Ping(context.Background()).Err(); err != nil {
		rdb.Close()
		t.Fatalf("ping TEST_REDIS_URL: %v", err)
	}
	t.Cleanup(func() { _ = rdb.Close() })

	profileID := "ws-lifecycle-profile-" + time.Now().Format("150405.000000000")
	sessionID := "ws-lifecycle-session"
	started := make(chan *wsSession, 1)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		upgrader := wsUpgrader
		upgrader.CheckOrigin = func(*http.Request) bool { return true }
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			return
		}
		s := &wsSession{
			conn:              conn,
			profileID:         profileID,
			sessionID:         sessionID,
			rdb:               rdb,
			send:              make(chan wsOutboundMessage, 64),
			done:              make(chan struct{}),
			subscribed:        make(map[string]struct{}),
			resources:         make(map[string]wsResource),
			channelGeneration: make(map[string]uint64),
			pingEvery:         10 * time.Millisecond,
			missedLimit:       2,
		}
		started <- s
		s.run(context.Background())
	}))
	t.Cleanup(server.Close)

	wsURL, err := url.Parse(server.URL)
	if err != nil {
		t.Fatalf("parse test websocket server URL: %v", err)
	}
	wsURL.Scheme = "ws"
	client, response, err := (&websocket.Dialer{HandshakeTimeout: time.Second}).Dial(wsURL.String(), nil)
	if err != nil {
		if response != nil {
			response.Body.Close()
		}
		t.Fatalf("dial lifecycle websocket: %v", err)
	}
	defer client.Close()

	var session *wsSession
	select {
	case session = <-started:
	case <-time.After(time.Second):
		t.Fatal("websocket session did not start")
	}
	waitForRedisSubscriptionCount(t, rdb, profileID, 1)
	waitForRedisSubscriptionCount(t, rdb, uuidZero, 1)

	// The client intentionally does not read frames, so Gorilla cannot process
	// ping frames or send pongs. The short test-only interval makes the missed-
	// pong path deterministic while exercising the real ReadMessage unblock and
	// run() cleanup path.
	select {
	case <-session.done:
	case <-time.After(2 * time.Second):
		t.Fatal("missed-pong threshold did not terminate the websocket session")
	}
	if got := redisSubscriberCount(t, rdb, profileID); got != 0 {
		t.Fatalf("Redis profile subscribers after termination = %d, want 0", got)
	}
	if got := redisSubscriberCount(t, rdb, uuidZero); got != 0 {
		t.Fatalf("Redis system subscribers after termination = %d, want 0", got)
	}

	client.SetReadDeadline(time.Now().Add(time.Second))
	if _, _, err := client.ReadMessage(); err == nil {
		t.Fatal("client read succeeded after missed-pong session termination")
	}
	session.subMu.Lock()
	defer session.subMu.Unlock()
	if session.pubsub != nil {
		t.Fatal("Redis Pub/Sub remained open after websocket session cleanup")
	}
}

func waitForRedisSubscriptionCount(t *testing.T, rdb *redis.Client, topic string, want int64) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if redisSubscriberCount(t, rdb, topic) == want {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	if got := redisSubscriberCount(t, rdb, topic); got != want {
		t.Fatalf("Redis subscribers for %q = %d, want %d", topic, got, want)
	}
}

func redisSubscriberCount(t *testing.T, rdb *redis.Client, topic string) int64 {
	t.Helper()
	channel := msgbusTopic(topic)
	counts, err := rdb.PubSubNumSub(context.Background(), channel).Result()
	if err != nil {
		t.Fatalf("query Redis subscriber count for %q: %v", channel, err)
	}
	return counts[channel]
}

func TestWSCheckOriginRejectsMissingOriginOnUpgrade(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		upgrader := wsUpgrader
		upgrader.CheckOrigin = func(req *http.Request) bool { return wsCheckOrigin(req, nil) }
		_, _ = upgrader.Upgrade(w, r, nil)
	}))
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http")
	_, response, err := (&websocket.Dialer{HandshakeTimeout: time.Second}).Dial(wsURL, nil)
	if err == nil || response == nil || response.StatusCode != http.StatusForbidden {
		t.Fatalf("upgrade without Origin: response=%v err=%v, want HTTP 403", response, err)
	}
	response.Body.Close()
}
