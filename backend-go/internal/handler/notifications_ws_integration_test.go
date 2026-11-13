package handler_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/handler"
	logosserver "github.com/logos-design/logos/backend-go/internal/server"
	"github.com/logos-design/logos/backend-go/internal/transit"
	goredis "github.com/redis/go-redis/v9"
)

func TestNotificationsWebSocketAuthenticatedUpgradeAndOriginIntegration(t *testing.T) {
	pool := openTestDB(t)
	profileID := createTestProfile(t, pool)
	server, token := newAuthenticatedNotificationsServer(t, pool, profileID, nil, "https://editor.example")

	noCookieConn, noCookieResponse, err := dialNotifications(server.URL, "", integrationUUID(t), server.URL)
	if noCookieConn != nil {
		noCookieConn.Close()
	}
	if err == nil || noCookieResponse == nil || noCookieResponse.StatusCode != http.StatusUnauthorized {
		t.Fatalf("unauthenticated upgrade: response=%v err=%v, want HTTP 401", noCookieResponse, err)
	}
	noCookieResponse.Body.Close()

	missingOriginConn, missingOriginResponse, err := dialNotifications(server.URL, token, integrationUUID(t), "")
	if missingOriginConn != nil {
		missingOriginConn.Close()
	}
	if err == nil || missingOriginResponse == nil || missingOriginResponse.StatusCode != http.StatusForbidden {
		t.Fatalf("upgrade without Origin: response=%v err=%v, want HTTP 403", missingOriginResponse, err)
	}
	missingOriginResponse.Body.Close()

	conn, response, err := dialNotifications(server.URL, token, integrationUUID(t), server.URL)
	if err != nil {
		if response != nil {
			response.Body.Close()
		}
		t.Fatalf("authenticated same-origin websocket upgrade: %v", err)
	}
	if response == nil || response.StatusCode != http.StatusSwitchingProtocols {
		conn.Close()
		t.Fatalf("authenticated upgrade response = %v, want HTTP 101", response)
	}
	conn.Close()

	proxyOriginConn, proxyOriginResponse, err := dialNotifications(server.URL, token, integrationUUID(t), "https://editor.example")
	if err != nil {
		if proxyOriginResponse != nil {
			proxyOriginResponse.Body.Close()
		}
		t.Fatalf("TLS-terminating proxy origin from explicit allowlist: %v", err)
	}
	if proxyOriginResponse == nil || proxyOriginResponse.StatusCode != http.StatusSwitchingProtocols {
		proxyOriginConn.Close()
		t.Fatalf("allowlisted proxy-origin response = %v, want HTTP 101", proxyOriginResponse)
	}
	proxyOriginConn.Close()

	badOriginConn, badOriginResponse, err := dialNotifications(server.URL, token, integrationUUID(t), "http://attacker.example")
	if badOriginConn != nil {
		badOriginConn.Close()
	}
	if err == nil || badOriginResponse == nil || badOriginResponse.StatusCode != http.StatusForbidden {
		t.Fatalf("cross-origin upgrade: response=%v err=%v, want HTTP 403", badOriginResponse, err)
	}
	badOriginResponse.Body.Close()
}

func TestNotificationsWebSocketResourceAuthorizationAndRevocationIntegration(t *testing.T) {
	pool := openTestDB(t)
	rdb := openTestRedis(t)

	ownerID := createTestProfile(t, pool)
	teamID, projectID := createTestTeamAndProject(t, pool, ownerID)
	otherTeamID, otherProjectID := createTestTeamAndProject(t, pool, ownerID)
	subscriberID := createTestProfile(t, pool)
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, false, false, false)`, teamID, subscriberID); err != nil {
		t.Fatalf("grant subscriber team read relationship: %v", err)
	}
	fileID := createIntegrationFile(t, pool, ownerID, projectID)
	otherFileID := createIntegrationFile(t, pool, ownerID, otherProjectID)
	server, token := newAuthenticatedNotificationsServer(t, pool, subscriberID, rdb)

	conn := dialAuthenticatedNotifications(t, server.URL, token, integrationUUID(t), server.URL)
	defer conn.Close()

	// A successful Redis subscription proves the permitted team subscription was
	// accepted. The following unsubscribe acts as an ordering barrier for the
	// cross-team attempt, since the websocket protocol has no subscribe ack.
	sendNotification(t, conn, map[string]any{"type": "subscribe-team", "team-id": teamID})
	waitRedisSubscribers(t, rdb, teamID, 1)
	sendNotification(t, conn, map[string]any{"type": "subscribe-team", "team-id": otherTeamID})
	sendNotification(t, conn, map[string]any{"type": "unsubscribe-team", "team-id": teamID})
	waitRedisSubscribers(t, rdb, teamID, 0)
	if got := redisSubscribers(t, rdb, otherTeamID); got != 0 {
		t.Fatalf("cross-team subscription count = %d, want 0", got)
	}
	sendNotification(t, conn, map[string]any{"type": "subscribe-file", "file-id": otherFileID})
	waitForBroadcastBarrier(t, rdb, conn, subscriberID)
	if got := redisSubscribers(t, rdb, otherFileID); got != 0 {
		t.Fatalf("cross-team file subscription count = %d, want 0", got)
	}

	// Re-subscribe to the allowed team and file after the ordering barrier.
	sendNotification(t, conn, map[string]any{"type": "subscribe-team", "team-id": teamID})
	waitRedisSubscribers(t, rdb, teamID, 1)
	sendNotification(t, conn, map[string]any{"type": "subscribe-file", "file-id": fileID})
	waitRedisSubscribers(t, rdb, fileID, 1)

	publishNotification(t, rdb, otherTeamID, map[string]any{
		"type": "library-change", "teamId": otherTeamID, "probe": "cross-team",
	})
	publishNotification(t, rdb, teamID, map[string]any{
		"type": "library-change", "teamId": teamID, "probe": "allowed-team",
	})
	assertNotificationProbe(t, conn, "allowed-team")
	publishNotification(t, rdb, fileID, map[string]any{
		"type": "file-change", "fileId": fileID, "probe": "allowed-file",
	})
	assertNotificationProbe(t, conn, "allowed-file")

	// Unsubscription must invalidate queued/in-flight events and retain enough
	// classification to reject messages even after the active channel is gone.
	sendNotification(t, conn, map[string]any{"type": "unsubscribe-team", "team-id": teamID})
	waitRedisSubscribers(t, rdb, teamID, 0)
	publishNotification(t, rdb, teamID, map[string]any{
		"type": "library-change", "teamId": teamID, "probe": "after-unsubscribe",
	})
	waitForBroadcastBarrier(t, rdb, conn, subscriberID)
	assertNoNotification(t, conn)
	sendNotification(t, conn, map[string]any{"type": "subscribe-team", "team-id": teamID})
	waitRedisSubscribers(t, rdb, teamID, 1)

	// Remove the only relationship that grants this subscriber access. Both
	// resources remain subscribed until an event arrives; that event must be
	// dropped and the now-unauthorized Redis channels removed.
	if _, err := pool.Exec(context.Background(), `DELETE FROM team_profile_rel WHERE team_id = $1 AND profile_id = $2`, teamID, subscriberID); err != nil {
		t.Fatalf("revoke team relationship: %v", err)
	}
	publishNotification(t, rdb, fileID, map[string]any{
		"type": "file-change", "fileId": fileID, "probe": "revoked-file",
	})
	publishNotification(t, rdb, teamID, map[string]any{
		"type": "library-change", "teamId": teamID, "probe": "revoked-team",
	})
	waitRedisSubscribers(t, rdb, fileID, 0)
	waitRedisSubscribers(t, rdb, teamID, 0)
	assertNoNotification(t, conn)
	conn.Close()

	// A new websocket connection must authorize fresh subscribe requests rather
	// than inherit the old connection's permissions.
	revokedConn := dialAuthenticatedNotifications(t, server.URL, token, integrationUUID(t), server.URL)
	sendNotification(t, revokedConn, map[string]any{"type": "subscribe-file", "file-id": fileID})
	sendNotification(t, revokedConn, map[string]any{"type": "subscribe-team", "team-id": teamID})
	waitForBroadcastBarrier(t, rdb, revokedConn, subscriberID)
	if got := redisSubscribers(t, rdb, fileID); got != 0 {
		t.Fatalf("revoked reconnect file subscription count = %d, want 0", got)
	}
	if got := redisSubscribers(t, rdb, teamID); got != 0 {
		t.Fatalf("revoked reconnect team subscription count = %d, want 0", got)
	}
	publishNotification(t, rdb, fileID, map[string]any{
		"type": "file-change", "fileId": fileID, "probe": "revoked-reconnect",
	})
	assertNoNotification(t, revokedConn)
	revokedConn.Close()

	// Restoring access and reconnecting permits the same resource subscriptions.
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO team_profile_rel (team_id, profile_id, is_owner, is_admin, can_edit)
		VALUES ($1, $2, false, false, false)`, teamID, subscriberID); err != nil {
		t.Fatalf("restore team relationship: %v", err)
	}
	reconnected := dialAuthenticatedNotifications(t, server.URL, token, integrationUUID(t), server.URL)
	defer reconnected.Close()
	sendNotification(t, reconnected, map[string]any{"type": "subscribe-file", "file-id": fileID})
	waitRedisSubscribers(t, rdb, fileID, 1)
	sendNotification(t, reconnected, map[string]any{"type": "subscribe-team", "team-id": teamID})
	waitRedisSubscribers(t, rdb, teamID, 1)
	publishNotification(t, rdb, fileID, map[string]any{
		"type": "file-change", "fileId": fileID, "probe": "reconnected-authorized",
	})
	assertNotificationProbe(t, reconnected, "reconnected-authorized")
	reconnected.Close()
	waitRedisSubscribers(t, rdb, fileID, 0)
	waitRedisSubscribers(t, rdb, teamID, 0)
}

func openTestRedis(t *testing.T) *goredis.Client {
	t.Helper()
	redisURL := os.Getenv("TEST_REDIS_URL")
	if redisURL == "" {
		t.Skip("TEST_REDIS_URL not set; skipping Redis integration test")
	}
	opts, err := goredis.ParseURL(redisURL)
	if err != nil {
		t.Fatalf("parse TEST_REDIS_URL: %v", err)
	}
	rdb := goredis.NewClient(opts)
	if err := rdb.Ping(context.Background()).Err(); err != nil {
		rdb.Close()
		t.Fatalf("ping TEST_REDIS_URL: %v", err)
	}
	t.Cleanup(func() { _ = rdb.Close() })
	return rdb
}

func newAuthenticatedNotificationsServer(t *testing.T, pool *db.Pool, profileID string, rdb *goredis.Client, allowedOrigins ...string) (*httptest.Server, string) {
	t.Helper()
	tokensKey, err := auth.DeriveTokensKey("track-e-runtime-security-contracts-integration")
	if err != nil {
		t.Fatalf("derive test session key: %v", err)
	}
	token, sessionID, err := auth.CreateSession(context.Background(), pool, profileID, "Track E websocket integration", tokensKey)
	if err != nil {
		t.Fatalf("create authenticated websocket session: %v", err)
	}
	t.Cleanup(func() {
		if err := auth.DeleteSession(context.Background(), pool, sessionID); err != nil {
			t.Errorf("delete test websocket session: %v", err)
		}
	})
	server := httptest.NewServer(logosserver.New(logosserver.Deps{
		Pool:                    pool,
		Redis:                   rdb,
		AuthMW:                  auth.NewMiddleware(pool, tokensKey, "auth-token"),
		TokensKey:               tokensKey,
		CookieName:              "auth-token",
		WebSocketAllowedOrigins: allowedOrigins,
	}))
	t.Cleanup(server.Close)
	return server, token
}

func createIntegrationFile(t *testing.T, pool *db.Pool, profileID, projectID string) string {
	t.Helper()
	body, err := json.Marshal(map[string]string{"projectId": projectID, "name": "WebSocket authorization fixture"})
	if err != nil {
		t.Fatalf("encode integration file request: %v", err)
	}
	req := httptest.NewRequest(http.MethodPost, "/api/rpc/command/create-file", bytes.NewReader(body))
	req = req.WithContext(auth.WithProfileID(req.Context(), profileID))
	rec := httptest.NewRecorder()
	handler.CreateFileHandler(pool).ServeHTTP(rec, req)
	if rec.Code != http.StatusOK {
		t.Fatalf("create websocket integration file: status=%d body=%s", rec.Code, rec.Body.String())
	}
	var created struct {
		ID string `json:"id"`
	}
	if err := json.NewDecoder(rec.Body).Decode(&created); err != nil {
		t.Fatalf("decode websocket integration file response: %v", err)
	}
	if created.ID == "" {
		t.Fatal("created websocket integration file has no ID")
	}
	return created.ID
}

func dialNotifications(serverURL, token, clientSessionID, origin string) (*websocket.Conn, *http.Response, error) {
	u, err := url.Parse(serverURL)
	if err != nil {
		return nil, nil, err
	}
	u.Scheme = "ws"
	u.Path = "/ws/notifications"
	query := u.Query()
	query.Set("session-id", clientSessionID)
	u.RawQuery = query.Encode()
	header := make(http.Header)
	if token != "" {
		header.Set("Cookie", "auth-token="+token)
	}
	if origin != "" {
		header.Set("Origin", origin)
	}
	dialer := websocket.Dialer{HandshakeTimeout: 4 * time.Second}
	return dialer.Dial(u.String(), header)
}

func dialAuthenticatedNotifications(t *testing.T, serverURL, token, clientSessionID, origin string) *websocket.Conn {
	t.Helper()
	conn, response, err := dialNotifications(serverURL, token, clientSessionID, origin)
	if err != nil {
		if response != nil {
			response.Body.Close()
		}
		t.Fatalf("dial authenticated notifications websocket: %v", err)
	}
	if response == nil || response.StatusCode != http.StatusSwitchingProtocols {
		conn.Close()
		t.Fatalf("websocket response = %v, want HTTP 101", response)
	}
	return conn
}

func sendNotification(t *testing.T, conn *websocket.Conn, message map[string]any) {
	t.Helper()
	encoded, err := transit.Encode(message)
	if err != nil {
		t.Fatalf("encode websocket message: %v", err)
	}
	if err := conn.WriteMessage(websocket.TextMessage, encoded); err != nil {
		t.Fatalf("write websocket message: %v", err)
	}
}

func publishNotification(t *testing.T, rdb *goredis.Client, topic string, message map[string]any) {
	t.Helper()
	payload, err := json.Marshal(message)
	if err != nil {
		t.Fatalf("encode Redis notification: %v", err)
	}
	if err := rdb.Publish(context.Background(), redisChannel(topic), payload).Err(); err != nil {
		t.Fatalf("publish Redis notification: %v", err)
	}
}

func waitForBroadcastBarrier(t *testing.T, rdb *goredis.Client, conn *websocket.Conn, profileID string) {
	t.Helper()
	observer := rdb.Subscribe(context.Background(), redisChannel(profileID))
	defer observer.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 4*time.Second)
	defer cancel()
	if _, err := observer.Receive(ctx); err != nil {
		t.Fatalf("subscribe to websocket profile channel barrier: %v", err)
	}
	sendNotification(t, conn, map[string]any{"type": "broadcast", "probe": "authorization-barrier"})
	if _, err := observer.ReceiveMessage(ctx); err != nil {
		t.Fatalf("wait for websocket command-processing barrier: %v", err)
	}
}

func assertNotificationProbe(t *testing.T, conn *websocket.Conn, want string) {
	t.Helper()
	conn.SetReadDeadline(time.Now().Add(4 * time.Second))
	_, payload, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("read websocket notification %q: %v", want, err)
	}
	decoded, err := transit.TransitToJSON(payload)
	if err != nil {
		t.Fatalf("decode websocket Transit notification: %v", err)
	}
	var message map[string]any
	if err := json.Unmarshal(decoded, &message); err != nil {
		t.Fatalf("unmarshal websocket notification: %v", err)
	}
	if got := message["probe"]; got != want {
		t.Fatalf("websocket notification probe = %v, want %q (message=%s)", got, want, decoded)
	}
}

func assertNoNotification(t *testing.T, conn *websocket.Conn) {
	t.Helper()
	conn.SetReadDeadline(time.Now().Add(250 * time.Millisecond))
	_, payload, err := conn.ReadMessage()
	if err == nil {
		t.Fatalf("received unexpected websocket notification: %s", payload)
	}
	var netErr net.Error
	if !errors.As(err, &netErr) || !netErr.Timeout() {
		t.Fatalf("waiting for dropped websocket notification returned %v, want read timeout", err)
	}
}

func waitRedisSubscribers(t *testing.T, rdb *goredis.Client, topic string, want int64) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		if got := redisSubscribers(t, rdb, topic); got == want {
			return
		}
		time.Sleep(25 * time.Millisecond)
	}
	if got := redisSubscribers(t, rdb, topic); got != want {
		t.Fatalf("Redis subscriber count for %q = %d, want %d", topic, got, want)
	}
}

func redisSubscribers(t *testing.T, rdb *goredis.Client, topic string) int64 {
	t.Helper()
	channel := redisChannel(topic)
	counts, err := rdb.PubSubNumSub(context.Background(), channel).Result()
	if err != nil {
		t.Fatalf("query Redis subscriber count for %q: %v", channel, err)
	}
	return counts[channel]
}

func redisChannel(topic string) string { return "default." + topic }
