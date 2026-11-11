// WebSocket notifications handler — port of app.http.websocket from the Clojure backend.
//
// Clients connect to GET /ws/notifications?session-id=<uuid> with the auth cookie set.
// The query session-id is a client-generated tab identifier (see app.main.cljs), not
// the http_session_v2 row id from the auth cookie.
// Messages are Transit+JSON encoded; the server sends WebSocket ping frames every 5s.
package handler

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"log"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/db"
	"github.com/logos-design/logos/backend-go/internal/perms"
	"github.com/logos-design/logos/backend-go/internal/transit"
	"github.com/redis/go-redis/v9"
)

const (
	msgbusTenant     = "default"
	uuidZero         = "00000000-0000-0000-0000-000000000000"
	wsPingInterval   = 5 * time.Second
	wsMaxMissedPings = 3
	wsWriteWait      = 10 * time.Second
	wsPongWait       = 20 * time.Second
)

var wsUUIDRE = regexp.MustCompile(`(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)

var wsUpgrader = websocket.Upgrader{
	ReadBufferSize:  4096,
	WriteBufferSize: 4096,
	CheckOrigin:     wsCheckOrigin,
}

// NotificationsHandler upgrades to WebSocket and relays Redis pub/sub messages.
func NotificationsHandler(pool *db.Pool, rdb *redis.Client) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		profileID := auth.ProfileID(r.Context())
		if profileID == "" {
			writeAuthError(w, http.StatusUnauthorized, "not-authenticated")
			return
		}

		sessionID := r.URL.Query().Get("session-id")
		if sessionID == "" {
			sessionID = r.URL.Query().Get("sessionId")
		}
		if sessionID == "" || !wsUUIDRE.MatchString(sessionID) {
			writeError(w, http.StatusUnprocessableEntity, "missing or invalid session-id")
			return
		}

		conn, err := wsUpgrader.Upgrade(w, r, nil)
		if err != nil {
			log.Printf("[ws] upgrade failed: %v", err)
			return
		}

		s := &wsSession{
			conn:       conn,
			pool:       pool,
			profileID:  profileID,
			sessionID:  sessionID,
			rdb:        rdb,
			send:       make(chan []byte, 64),
			done:       make(chan struct{}),
			subscribed: make(map[string]struct{}),
			resources:  make(map[string]wsResource),
		}
		// The request context is canceled when this handler returns, which is
		// immediately after the upgrade. The websocket owns its own lifetime.
		go s.run(context.Background())
	}
}

// wsCheckOrigin permits legacy non-browser clients that omit Origin, but
// rejects malformed, opaque, and cross-host browser origins. Cross-origin
// deployments must proxy the websocket through the frontend host.
func wsCheckOrigin(r *http.Request) bool {
	origins := r.Header.Values("Origin")
	if len(origins) == 0 {
		return true
	}
	if len(origins) != 1 || origins[0] == "" {
		return false
	}

	origin, err := url.Parse(origins[0])
	if err != nil || (origin.Scheme != "http" && origin.Scheme != "https") ||
		origin.Host == "" || origin.User != nil || origin.Opaque != "" ||
		(origin.Path != "" && origin.Path != "/") || origin.RawQuery != "" || origin.Fragment != "" {
		return false
	}
	requestHost, err := url.Parse("//" + r.Host)
	if err != nil || requestHost.Host == "" || requestHost.User != nil || requestHost.Path != "" {
		return false
	}
	if origin.Scheme != requestScheme(r) {
		return false
	}

	originName, originPort := normalizedAuthority(origin, origin.Scheme)
	requestName, requestPort := normalizedAuthority(requestHost, origin.Scheme)
	return originName != "" && originName == requestName && originPort == requestPort
}

// requestScheme uses direct TLS state when available. For TLS-terminating
// reverse proxies, X-Forwarded-Proto must be overwritten by the trusted proxy
// and stripped from untrusted client requests.
func requestScheme(r *http.Request) string {
	if r.TLS != nil {
		return "https"
	}
	if forwarded := strings.TrimSpace(r.Header.Get("X-Forwarded-Proto")); forwarded == "http" || forwarded == "https" {
		return forwarded
	}
	if r.URL != nil && (r.URL.Scheme == "http" || r.URL.Scheme == "https") {
		return r.URL.Scheme
	}
	return "http"
}

func normalizedAuthority(u *url.URL, scheme string) (string, string) {
	name := strings.TrimSuffix(strings.ToLower(u.Hostname()), ".")
	port := u.Port()
	if port == "" {
		if scheme == "https" {
			port = "443"
		} else {
			port = "80"
		}
	}
	return name, port
}

type wsResourceKind uint8

const (
	wsFileResource wsResourceKind = iota + 1
	wsTeamResource
)

type wsResource struct {
	kind wsResourceKind
	id   string
}

type wsSession struct {
	conn       *websocket.Conn
	pool       *db.Pool
	profileID  string
	sessionID  string
	rdb        *redis.Client
	send       chan []byte
	done       chan struct{}
	subMu      sync.Mutex
	subscribed map[string]struct{}
	resources  map[string]wsResource // Redis channel -> authorized file/team
	pubsub     *redis.PubSub
	missedPing int
	pingMu     sync.Mutex
}

func (s *wsSession) run(ctx context.Context) {
	ctx, cancel := context.WithCancel(ctx)
	defer func() {
		cancel()
		s.closePubSub()
		_ = s.conn.Close()
		close(s.done)
	}()

	// Initial subscriptions: profile channel + system (uuid zero).
	s.subscribeTopics(s.profileID, uuidZero)
	if s.rdb != nil {
		s.startRedisLoop(ctx)
	}

	s.conn.SetReadDeadline(time.Now().Add(wsPongWait))
	s.conn.SetPongHandler(func(string) error {
		s.pingMu.Lock()
		s.missedPing = 0
		s.pingMu.Unlock()
		s.conn.SetReadDeadline(time.Now().Add(wsPongWait))
		return nil
	})

	go s.writeLoop()
	go s.pingLoop()

	for {
		_, data, err := s.conn.ReadMessage()
		if err != nil {
			return
		}
		s.conn.SetReadDeadline(time.Now().Add(wsPongWait))
		s.handleMessage(ctx, data)
	}
}

func (s *wsSession) writeLoop() {
	for {
		select {
		case <-s.done:
			return
		case msg := <-s.send:
			s.conn.SetWriteDeadline(time.Now().Add(wsWriteWait))
			if err := s.conn.WriteMessage(websocket.TextMessage, msg); err != nil {
				return
			}
		}
	}
}

func (s *wsSession) pingLoop() {
	ticker := time.NewTicker(wsPingInterval)
	defer ticker.Stop()

	missed := 0
	beat := int64(0)
	for {
		select {
		case <-s.done:
			return
		case <-ticker.C:
			s.pingMu.Lock()
			missed = s.missedPing
			s.missedPing++
			s.pingMu.Unlock()

			if missed >= wsMaxMissedPings {
				return
			}

			beat++
			payload := make([]byte, 8)
			binary.BigEndian.PutUint64(payload, uint64(beat))

			if err := s.conn.WriteControl(websocket.PingMessage, payload, time.Now().Add(wsWriteWait)); err != nil {
				return
			}
		}
	}
}

func (s *wsSession) handleMessage(ctx context.Context, data []byte) {
	msg, err := decodeWSMessage(data)
	if err != nil {
		return
	}

	typ, _ := msg["type"].(string)
	switch typ {
	case "keepalive":
		// no-op
	case "subscribe-team":
		teamID, _ := msg["team-id"].(string)
		if teamID == "" {
			teamID, _ = msg["teamId"].(string)
		}
		if teamID != "" {
			resource := wsResource{kind: wsTeamResource, id: teamID}
			if s.authorized(ctx, resource) {
				s.subscribeResource(resource, teamID)
			}
		}
	case "unsubscribe-team":
		teamID, _ := msg["team-id"].(string)
		if teamID == "" {
			teamID, _ = msg["teamId"].(string)
		}
		if teamID != "" {
			s.unsubscribeResource(wsResource{kind: wsTeamResource, id: teamID})
		}
	case "subscribe-file":
		fileID, _ := msg["file-id"].(string)
		if fileID == "" {
			fileID, _ = msg["fileId"].(string)
		}
		pageID, _ := msg["page-id"].(string)
		if pageID == "" {
			pageID, _ = msg["pageId"].(string)
		}
		if fileID == "" {
			return
		}
		resource := wsResource{kind: wsFileResource, id: fileID}
		if !s.authorized(ctx, resource) {
			return
		}
		if pageID != "" {
			s.subscribeResource(resource, fileID+":page:"+pageID, fileID+":meta")
		} else {
			s.subscribeResource(resource, fileID)
		}
	case "unsubscribe-file":
		fileID, _ := msg["file-id"].(string)
		if fileID == "" {
			fileID, _ = msg["fileId"].(string)
		}
		if fileID != "" {
			s.unsubscribeResource(wsResource{kind: wsFileResource, id: fileID})
		}
	case "broadcast":
		s.publish(msg)
	case "pointer-update":
		fileID, _ := msg["file-id"].(string)
		if fileID == "" {
			fileID, _ = msg["fileId"].(string)
		}
		if fileID != "" {
			if !s.authorized(ctx, wsResource{kind: wsFileResource, id: fileID}) {
				return
			}
			s.publishToTopic(fileID, msg)
			s.publishToTopic(fileID+":meta", msg)
		}
	}
}

func (s *wsSession) authorized(ctx context.Context, resource wsResource) bool {
	if s.pool == nil || resource.id == "" {
		return false
	}
	switch resource.kind {
	case wsFileResource:
		p, err := perms.GetFilePermissions(ctx, s.pool, s.profileID, resource.id)
		return err == nil && p != nil && p.CanRead
	case wsTeamResource:
		p, err := perms.GetTeamPermissions(ctx, s.pool, s.profileID, resource.id)
		return err == nil && p != nil && p.CanRead
	default:
		return false
	}
}

func (s *wsSession) subscribeTopics(topics ...string) {
	if s.rdb == nil {
		return
	}

	channels := make([]string, 0, len(topics))
	s.subMu.Lock()
	defer s.subMu.Unlock()
	s.ensurePubSubLocked()
	for _, topic := range topics {
		ch := msgbusTopic(topic)
		if _, ok := s.subscribed[ch]; ok {
			continue
		}
		s.subscribed[ch] = struct{}{}
		channels = append(channels, ch)
	}

	if len(channels) == 0 {
		return
	}

	_ = s.pubsub.Subscribe(context.Background(), channels...)
}

func (s *wsSession) subscribeResource(resource wsResource, topics ...string) {
	if s.rdb == nil {
		return
	}
	channels := make([]string, 0, len(topics))
	s.subMu.Lock()
	defer s.subMu.Unlock()
	s.ensurePubSubLocked()
	for _, topic := range topics {
		ch := msgbusTopic(topic)
		s.resources[ch] = resource
		if _, ok := s.subscribed[ch]; ok {
			continue
		}
		s.subscribed[ch] = struct{}{}
		channels = append(channels, ch)
	}
	if len(channels) > 0 {
		_ = s.pubsub.Subscribe(context.Background(), channels...)
	}
}

func (s *wsSession) ensurePubSubLocked() {
	if s.pubsub == nil && s.rdb != nil {
		s.pubsub = s.rdb.Subscribe(context.Background())
	}
}

func (s *wsSession) unsubscribeResource(resource wsResource) {
	if s.rdb == nil {
		return
	}
	var channels []string
	s.subMu.Lock()
	for ch, subscribedResource := range s.resources {
		if subscribedResource != resource {
			continue
		}
		delete(s.resources, ch)
		if _, ok := s.subscribed[ch]; ok {
			delete(s.subscribed, ch)
			channels = append(channels, ch)
		}
	}
	pubsub := s.pubsub
	if pubsub != nil && len(channels) > 0 {
		_ = pubsub.Unsubscribe(context.Background(), channels...)
	}
	s.subMu.Unlock()
}

func (s *wsSession) startRedisLoop(ctx context.Context) {
	go func() {
		for {
			select {
			case <-s.done:
				return
			case <-ctx.Done():
				return
			default:
			}

			pubsub := s.currentPubSub()
			if pubsub == nil {
				if !waitContext(ctx, 100*time.Millisecond) {
					return
				}
				continue
			}

			msg, err := pubsub.ReceiveMessage(ctx)
			if err != nil {
				if ctx.Err() != nil {
					return
				}
				if !waitContext(ctx, 200*time.Millisecond) {
					return
				}
				continue
			}

			payload, err := decodeWSMessage([]byte(msg.Payload))
			if err != nil {
				// Go handlers may publish plain JSON; try that path too.
				var plain map[string]any
				if jsonErr := json.Unmarshal([]byte(msg.Payload), &plain); jsonErr != nil {
					continue
				}
				payload = plain
			}

			resource, resourceChannel := s.resourceForChannel(msg.Channel)
			if resourceChannel {
				eventResource, ok := notificationResource(resource, payload)
				if !ok {
					continue
				}
				if !s.authorized(ctx, eventResource) {
					if eventResource == resource {
						s.unsubscribeResource(resource)
					}
					continue
				}
			}

			sid, _ := payload["session-id"].(string)
			if sid == "" {
				sid, _ = payload["sessionId"].(string)
			}
			if sid == s.sessionID {
				continue
			}

			out, err := transit.Encode(payload)
			if err != nil {
				continue
			}

			select {
			case s.send <- out:
			default:
			}
		}
	}()
}

func waitContext(ctx context.Context, delay time.Duration) bool {
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-timer.C:
		return true
	}
}

func (s *wsSession) currentPubSub() *redis.PubSub {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	return s.pubsub
}

func (s *wsSession) resourceForChannel(channel string) (wsResource, bool) {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	resource, ok := s.resources[channel]
	return resource, ok
}

// notificationResource classifies the resource that produced a Redis message,
// rather than trusting only the resource most recently associated with a
// channel. Legacy file and team topics share the same raw UUID namespace, so a
// file UUID colliding with a team UUID must still be authorized as the event's
// actual resource.
func notificationResource(subscription wsResource, message map[string]any) (wsResource, bool) {
	typ, _ := message["type"].(string)
	typ = strings.TrimPrefix(typ, "~:")
	if typ == "library-change" {
		if subscription.id == "" {
			return wsResource{}, false
		}
		return wsResource{kind: wsTeamResource, id: subscription.id}, true
	}

	fileID := messageString(message, "file-id", "fileId")
	if fileID != "" {
		return wsResource{kind: wsFileResource, id: fileID}, true
	}
	teamID := messageString(message, "team-id", "teamId")
	if teamID != "" {
		return wsResource{kind: wsTeamResource, id: teamID}, true
	}
	return wsResource{}, false
}

func messageString(message map[string]any, keys ...string) string {
	for _, key := range keys {
		if value, ok := message[key].(string); ok && value != "" {
			return value
		}
	}
	return ""
}

func (s *wsSession) closePubSub() {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	if s.pubsub != nil {
		_ = s.pubsub.Close()
		s.pubsub = nil
	}
}

func (s *wsSession) publish(msg map[string]any) {
	msg["profile-id"] = s.profileID
	msg["session-id"] = s.sessionID
	s.publishToTopic(s.profileID, msg)
}

func (s *wsSession) publishToTopic(topic string, msg map[string]any) {
	if s.rdb == nil {
		return
	}
	out, err := transit.Encode(msg)
	if err != nil {
		return
	}
	_ = s.rdb.Publish(context.Background(), msgbusTopic(topic), string(out)).Err()
}

func msgbusTopic(topic string) string {
	return msgbusTenant + "." + topic
}

func decodeWSMessage(data []byte) (map[string]any, error) {
	jsonBytes, err := transit.TransitToJSON(data)
	if err != nil {
		return nil, err
	}
	var msg map[string]any
	if err := json.Unmarshal(jsonBytes, &msg); err != nil {
		return nil, err
	}
	return msg, nil
}
