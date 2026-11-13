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
}

// NotificationsHandler upgrades to WebSocket and relays Redis pub/sub messages.
func NotificationsHandler(pool *db.Pool, rdb *redis.Client, allowedOrigins []string) http.HandlerFunc {
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

		upgrader := wsUpgrader
		upgrader.CheckOrigin = func(req *http.Request) bool {
			return wsCheckOrigin(req, allowedOrigins)
		}
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			log.Printf("[ws] upgrade failed: %v", err)
			return
		}

		s := &wsSession{
			conn:              conn,
			pool:              pool,
			profileID:         profileID,
			sessionID:         sessionID,
			rdb:               rdb,
			send:              make(chan wsOutboundMessage, 64),
			done:              make(chan struct{}),
			subscribed:        make(map[string]struct{}),
			resources:         make(map[string]wsResource),
			channelGeneration: make(map[string]uint64),
		}
		// The request context is canceled when this handler returns, which is
		// immediately after the upgrade. The websocket owns its own lifetime.
		go s.run(context.Background())
	}
}

// wsCheckOrigin rejects requests without Origin, malformed/opaque origins,
// and cross-origin requests unless the exact origin is deployment-allowlisted.
// It never trusts a client-supplied forwarded protocol header.
func wsCheckOrigin(r *http.Request, allowedOrigins []string) bool {
	origins := r.Header.Values("Origin")
	if len(origins) == 0 {
		return false
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
	if originMatchesAllowlist(origin, allowedOrigins) {
		return true
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

// requestScheme uses transport state rather than forwarded headers. TLS-
// terminating proxies must configure their exact browser origin in the
// backend's allowlist.
func requestScheme(r *http.Request) string {
	if r.TLS != nil {
		return "https"
	}
	if r.URL != nil && (r.URL.Scheme == "http" || r.URL.Scheme == "https") {
		return r.URL.Scheme
	}
	return "http"
}

func originMatchesAllowlist(origin *url.URL, allowedOrigins []string) bool {
	for _, raw := range allowedOrigins {
		allowed, err := url.Parse(strings.TrimSpace(raw))
		if err != nil || (allowed.Scheme != "http" && allowed.Scheme != "https") ||
			allowed.Host == "" || allowed.User != nil || allowed.Opaque != "" ||
			(allowed.Path != "" && allowed.Path != "/") || allowed.RawQuery != "" || allowed.Fragment != "" {
			continue
		}
		if origin.Scheme != allowed.Scheme {
			continue
		}
		originName, originPort := normalizedAuthority(origin, origin.Scheme)
		allowedName, allowedPort := normalizedAuthority(allowed, allowed.Scheme)
		if originName != "" && originName == allowedName && originPort == allowedPort {
			return true
		}
	}
	return false
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

type wsOutboundMessage struct {
	channel         string
	resource        wsResource
	resourceChannel bool
	generation      uint64
	payload         []byte
}

type wsSession struct {
	conn              *websocket.Conn
	pool              *db.Pool
	profileID         string
	sessionID         string
	rdb               *redis.Client
	send              chan wsOutboundMessage
	done              chan struct{}
	subMu             sync.Mutex
	subscribed        map[string]struct{}
	resources         map[string]wsResource // Redis channel -> authorized file/team
	channelGeneration map[string]uint64
	pubsub            *redis.PubSub
	missedPing        int
	pingMu            sync.Mutex
	closeOnce         sync.Once
	pingEvery         time.Duration // zero selects the production interval; overridden by deterministic lifecycle tests
	missedLimit       int           // zero selects the production threshold; overridden by deterministic lifecycle tests
}

func (s *wsSession) run(ctx context.Context) {
	ctx, cancel := context.WithCancel(ctx)
	defer func() {
		cancel()
		s.closePubSub()
		s.terminate()
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
			if !s.writeOutboundMessage(msg) {
				return
			}
		}
	}
}

// writeOutboundMessage rechecks subscription and permission state at the last
// point before delivery. Holding subMu through the write gives unsubscribe a
// clear ordering point: a frame is either written before unsubscribe takes
// effect, or it is dropped.
func (s *wsSession) writeOutboundMessage(msg wsOutboundMessage) bool {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if !s.outboundAllowedLocked(ctx, msg) {
		return true
	}
	s.conn.SetWriteDeadline(time.Now().Add(wsWriteWait))
	if err := s.conn.WriteMessage(websocket.TextMessage, msg.payload); err != nil {
		s.terminate()
		return false
	}
	return true
}

func (s *wsSession) pingLoop() {
	interval := s.pingEvery
	if interval <= 0 {
		interval = wsPingInterval
	}
	missedLimit := s.missedLimit
	if missedLimit <= 0 {
		missedLimit = wsMaxMissedPings
	}
	ticker := time.NewTicker(interval)
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

			if missed >= missedLimit {
				s.terminate()
				return
			}

			beat++
			payload := make([]byte, 8)
			binary.BigEndian.PutUint64(payload, uint64(beat))

			if err := s.conn.WriteControl(websocket.PingMessage, payload, time.Now().Add(wsWriteWait)); err != nil {
				s.terminate()
				return
			}
		}
	}
}

func (s *wsSession) terminate() {
	s.closeOnce.Do(func() {
		if s.conn != nil {
			_ = s.conn.Close()
		}
	})
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
		channels = append(channels, ch)
	}

	if len(channels) == 0 {
		return
	}

	if err := s.pubsub.Subscribe(context.Background(), channels...); err != nil {
		return
	}
	for _, ch := range channels {
		s.channelGeneration[ch]++
		s.subscribed[ch] = struct{}{}
	}
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
		if old, ok := s.resources[ch]; !ok || old != resource {
			s.channelGeneration[ch]++
		}
		s.resources[ch] = resource
		if _, ok := s.subscribed[ch]; ok {
			continue
		}
		channels = append(channels, ch)
	}
	if len(channels) > 0 {
		if err := s.pubsub.Subscribe(context.Background(), channels...); err != nil {
			return
		}
		for _, ch := range channels {
			s.channelGeneration[ch]++
			s.subscribed[ch] = struct{}{}
		}
	}
}

func (s *wsSession) ensurePubSubLocked() {
	if s.pubsub == nil && s.rdb != nil {
		s.pubsub = s.rdb.Subscribe(context.Background())
	}
}

func (s *wsSession) unsubscribeResource(resource wsResource) {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	s.unsubscribeResourceLocked(resource)
}

func (s *wsSession) unsubscribeResourceLocked(resource wsResource) {
	var channels []string
	for ch, subscribedResource := range s.resources {
		if subscribedResource != resource {
			continue
		}
		if _, ok := s.subscribed[ch]; ok {
			delete(s.subscribed, ch)
			s.channelGeneration[ch]++
			channels = append(channels, ch)
		}
	}
	pubsub := s.pubsub
	if pubsub != nil && len(channels) > 0 {
		_ = pubsub.Unsubscribe(context.Background(), channels...)
	}
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

			resource, resourceChannel, active, generation := s.channelStatus(msg.Channel)
			if !active {
				continue
			}
			if !resourceChannel && !s.isSystemChannel(msg.Channel) {
				continue
			}
			var eventResource wsResource
			if resourceChannel {
				var ok bool
				eventResource, ok = notificationResource(resource, payload)
				if !ok {
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

			s.enqueueOutbound(ctx, wsOutboundMessage{
				channel:         msg.Channel,
				resource:        eventResource,
				resourceChannel: resourceChannel,
				generation:      generation,
				payload:         out,
			})
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

func (s *wsSession) channelStatus(channel string) (wsResource, bool, bool, uint64) {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	resource, ok := s.resources[channel]
	_, active := s.subscribed[channel]
	return resource, ok, active, s.channelGeneration[channel]
}

func (s *wsSession) isSystemChannel(channel string) bool {
	return channel == msgbusTopic(s.profileID) || channel == msgbusTopic(uuidZero)
}

func (s *wsSession) enqueueOutbound(ctx context.Context, msg wsOutboundMessage) bool {
	s.subMu.Lock()
	defer s.subMu.Unlock()
	if !s.outboundAllowedLocked(ctx, msg) {
		return false
	}
	select {
	case s.send <- msg:
		return true
	default:
		return false
	}
}

func (s *wsSession) outboundAllowedLocked(ctx context.Context, msg wsOutboundMessage) bool {
	if _, active := s.subscribed[msg.channel]; !active || s.channelGeneration[msg.channel] != msg.generation {
		return false
	}
	if !msg.resourceChannel {
		return s.isSystemChannel(msg.channel)
	}
	resource, classified := s.resources[msg.channel]
	if !classified {
		return false
	}
	if s.authorized(ctx, msg.resource) {
		return true
	}
	if msg.resource == resource {
		s.unsubscribeResourceLocked(resource)
	}
	return false
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
	for channel := range s.subscribed {
		delete(s.subscribed, channel)
		s.channelGeneration[channel]++
	}
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
