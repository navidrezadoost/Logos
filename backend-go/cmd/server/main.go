// cmd/server/main.go — entrypoint for the Go backend.
//
// Reads DATABASE_URL / REDIS_URL / BACKEND_GO_ADDR from the environment,
// connects to PostgreSQL (and optionally Redis), then starts an HTTP server.
//
// Quick-start:
//
//	DATABASE_URL=postgres://logos:logos@localhost:5432/logos \
//	  go run ./cmd/server
//
// Or via Make:
//
//	make run-go-backend
package main

import (
	"context"
	"errors"
	"log"
	"net/http"
	"os/signal"
	"syscall"
	"time"

	goredis "github.com/redis/go-redis/v9"

	"github.com/logos-design/logos/backend-go/internal/auth"
	"github.com/logos-design/logos/backend-go/internal/config"
	"github.com/logos-design/logos/backend-go/internal/db"
	appredis "github.com/logos-design/logos/backend-go/internal/redis"
	"github.com/logos-design/logos/backend-go/internal/server"
	"github.com/logos-design/logos/backend-go/internal/storage"
)

func main() {
	cfg := config.Load()

	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	// ── Database ────────────────────────────────────────────────────────────
	log.Printf("[db] connecting to postgres…")
	pool, err := db.New(ctx, cfg.DatabaseURL)
	if err != nil {
		log.Fatalf("[db] failed: %v", err)
	}
	defer pool.Close()
	log.Printf("[db] connected")

	// ── Redis (optional) ────────────────────────────────────────────────────
	var rdb *goredis.Client
	if cfg.RedisURL != "" {
		rc, err := appredis.New(ctx, cfg.RedisURL)
		if err != nil {
			log.Printf("[redis] warning — could not connect: %v (caching disabled)", err)
		} else {
			defer rc.Close()
			rdb = rc.Client
			log.Printf("[redis] connected")
		}
	}

	// ── Storage backend ──────────────────────────────────────────────────────
	sto, err := storage.New(cfg)
	if err != nil {
		log.Printf("[storage] warning — %v (photo upload disabled)", err)
		sto = nil
	}

	// ── Auth middleware ──────────────────────────────────────────────────────
	var authMW *auth.Middleware
	if cfg.SecretKey != "" {
		tokensKey, err := auth.DeriveTokensKey(cfg.SecretKey)
		if err != nil {
			log.Fatalf("[auth] key derivation failed: %v", err)
		}
		authMW = auth.NewMiddleware(pool, tokensKey, cfg.CookieName)
		log.Printf("[auth] session middleware enabled")
	} else {
		log.Printf("[auth] LOGOS_SECRET_KEY not set — session middleware disabled (all requests anonymous)")
	}

	// ── HTTP server ─────────────────────────────────────────────────────────
	h := server.New(server.Deps{
		Pool:    pool,
		Redis:   rdb,
		Storage: sto,
		AuthMW:  authMW,
	})
	srv := &http.Server{
		Addr:         cfg.Addr,
		Handler:      h,
		ReadTimeout:  15 * time.Second,
		WriteTimeout: 15 * time.Second,
		IdleTimeout:  60 * time.Second,
	}

	// Start in background.
	go func() {
		log.Printf("[http] listening on %s", cfg.Addr)
		if err := srv.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Fatalf("[http] fatal: %v", err)
		}
	}()

	// Block until SIGINT / SIGTERM.
	<-ctx.Done()
	stop()
	log.Printf("[http] shutting down…")

	shutCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := srv.Shutdown(shutCtx); err != nil {
		log.Printf("[http] shutdown error: %v", err)
	}
	log.Printf("[http] stopped")
}
