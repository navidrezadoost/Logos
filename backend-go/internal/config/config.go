// Package config loads runtime configuration from environment variables.
// All values have sensible defaults so the service starts with only a DATABASE_URL.
package config

import (
	"os"
	"strconv"
)

// Config holds all service configuration.
type Config struct {
	// HTTP
	Addr string // e.g. ":8080"

	// PostgreSQL
	DatabaseURL string // postgres://user:pass@host:port/db

	// Redis
	RedisURL string // redis://:pass@host:port/db   (optional)

	// Auth
	// SecretKey must match LOGOS_SECRET_KEY used by the Clojure backend.
	// The Go service derives the same tokens-key from it (HKDF-Blake2b-512).
	SecretKey       string
	CookieName      string // default "auth-token"

	// Feature flags
	CacheEnabled bool // read-through Redis cache for profile
	CacheTTL     int  // seconds; default 300

	// Storage
	StorageBackend  string // "local" | "s3"
	StorageLocalDir string // root dir for "local" backend
	S3Bucket        string
	S3Region        string
	S3Endpoint      string // override for MinIO / local S3
}

// Load reads configuration from environment variables, applying defaults.
func Load() Config {
	return Config{
		Addr:            envOr("BACKEND_GO_ADDR", ":8080"),
		DatabaseURL:     envOr("DATABASE_URL", "postgres://logos:logos@localhost:5432/logos"),
		RedisURL:        envOr("REDIS_URL", ""),
		SecretKey:       envOr("LOGOS_SECRET_KEY", ""),
		CookieName:      envOr("AUTH_TOKEN_COOKIE_NAME", "auth-token"),
		CacheEnabled:    envBool("CACHE_ENABLED", false),
		CacheTTL:        envInt("CACHE_TTL_SECONDS", 300),
		StorageBackend:  envOr("STORAGE_BACKEND", "local"),
		StorageLocalDir: envOr("STORAGE_LOCAL_DIR", "/tmp/logos-storage"),
		S3Bucket:        envOr("STORAGE_S3_BUCKET", ""),
		S3Region:        envOr("STORAGE_S3_REGION", "us-east-1"),
		S3Endpoint:      envOr("STORAGE_S3_ENDPOINT", ""),
	}
}

func envOr(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}

func envBool(key string, fallback bool) bool {
	v := os.Getenv(key)
	if v == "" {
		return fallback
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		return fallback
	}
	return b
}

func envInt(key string, fallback int) int {
	v := os.Getenv(key)
	if v == "" {
		return fallback
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		return fallback
	}
	return n
}
