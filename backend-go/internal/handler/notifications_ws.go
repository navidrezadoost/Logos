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
	"regexp"
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/transit"
	"github.com/redis/go-redis/v9"
)

const (
	msgbusTenant       = "default"
	uuidZero           = "00000000-0000-0000-0000-000000000000"
	wsPingInterval     = 5 * time.Second
	wsMaxMissedPings   = 3
	wsWriteWait        = 10 * time.Second
	wsPongWait         = 20 * time.Second
)

var wsUUIDRE = regexp.MustCompile(`(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)

var wsUpgrader = websocket.Upgrader{
	ReadBufferSize:  4096,
	WriteBufferSize: 4096,
	CheckOrigin:     func(_ *http.Request) bool { return true },
}

// NotificationsHandler upgrades to WebSocket and relays Redis pub/sub messages.
func NotificationsHandler(rdb *redis.Client) http.HandlerFunc {
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
			profileID:  profileID,
			sessionID:  sessionID,
			rdb:        rdb,
			send:       make(chan []byte, 64),
			done:       make(chan struct{}),
			subscribed: make(map[string]struct{}),
		}
		go s.run(r.Context())
	}
}

type wsSession struct {
	conn       *websocket.Conn
	profileID  string
	sessionID  string
	rdb        *redis.Client
	send       chan []byte
	done       chan struct{}
	subMu      sync.Mutex
	subscribed map[string]struct{}
	pubsub     *redis.PubSub
	missedPing int
	pingMu     sync.Mutex
}

func (s *wsSession) run(ctx context.Context) {
	defer close(s.done)
	defer s.conn.Close()

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
		s.handleMessage(data)
	}
}

func (s *wsSession) writeLoop() {
	for msg := range s.send {
		s.conn.SetWriteDeadline(time.Now().Add(wsWriteWait))
		if err := s.conn.WriteMessage(websocket.TextMessage, msg); err != nil {
			return
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

			s.conn.SetWriteDeadline(time.Now().Add(wsWriteWait))
			if err := s.conn.WriteMessage(websocket.PingMessage, payload); err != nil {
				return
			}
		}
	}
}

func (s *wsSession) handleMessage(data []byte) {
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
			s.subscribeTopics(teamID)
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
		if pageID != "" {
			s.subscribeTopics(fileID+":page:"+pageID, fileID+":meta")
		} else {
			s.subscribeTopics(fileID)
		}
	case "unsubscribe-file":
		fileID, _ := msg["file-id"].(string)
		if fileID == "" {
			fileID, _ = msg["fileId"].(string)
		}
		if fileID != "" {
			s.unsubscribeTopics(fileID, fileID+":meta")
		}
	case "broadcast":
		s.publish(msg)
	case "pointer-update":
		fileID, _ := msg["file-id"].(string)
		if fileID == "" {
			fileID, _ = msg["fileId"].(string)
		}
		if fileID != "" {
			s.publishToTopic(fileID, msg)
			s.publishToTopic(fileID+":meta", msg)
		}
	}
}

func (s *wsSession) subscribeTopics(topics ...string) {
	if s.rdb == nil {
		return
	}

	channels := make([]string, 0, len(topics))
	s.subMu.Lock()
	for _, topic := range topics {
		ch := msgbusTopic(topic)
		if _, ok := s.subscribed[ch]; ok {
			continue
		}
		s.subscribed[ch] = struct{}{}
		channels = append(channels, ch)
	}
	s.subMu.Unlock()

	if len(channels) == 0 {
		return
	}

	if s.pubsub == nil {
		s.pubsub = s.rdb.Subscribe(context.Background())
	}
	_ = s.pubsub.Subscribe(context.Background(), channels...)
}

func (s *wsSession) unsubscribeTopics(topics ...string) {
	if s.rdb == nil || s.pubsub == nil {
		return
	}

	channels := make([]string, 0, len(topics))
	s.subMu.Lock()
	for _, topic := range topics {
		ch := msgbusTopic(topic)
		if _, ok := s.subscribed[ch]; !ok {
			continue
		}
		delete(s.subscribed, ch)
		channels = append(channels, ch)
	}
	s.subMu.Unlock()

	if len(channels) > 0 {
		_ = s.pubsub.Unsubscribe(context.Background(), channels...)
	}
}

func (s *wsSession) startRedisLoop(ctx context.Context) {
	go func() {
		for {
			select {
			case <-s.done:
				if s.pubsub != nil {
					_ = s.pubsub.Close()
				}
				return
			default:
			}

			if s.pubsub == nil {
				time.Sleep(100 * time.Millisecond)
				continue
			}

			msg, err := s.pubsub.ReceiveMessage(ctx)
			if err != nil {
				if ctx.Err() != nil {
					return
				}
				time.Sleep(200 * time.Millisecond)
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
