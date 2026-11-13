package config

import (
	"reflect"
	"testing"
)

func TestLoadParsesWebSocketAllowedOrigins(t *testing.T) {
	t.Setenv("BACKEND_GO_WEBSOCKET_ALLOWED_ORIGINS", "https://editor.example, http://localhost:8888,,")
	got := Load().WebSocketAllowedOrigins
	want := []string{"https://editor.example", "http://localhost:8888"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("WebSocketAllowedOrigins = %#v, want %#v", got, want)
	}
}
