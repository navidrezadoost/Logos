// Package redis wraps go-redis for optional read-through caching.
package redis

import (
	"context"
	"fmt"

	goredis "github.com/redis/go-redis/v9"
)

// Client wraps goredis.Client.
type Client struct {
	*goredis.Client
}

// New parses a Redis URL and verifies liveness with PING.
// Returns (nil, nil) when url is empty — callers should treat nil as "no cache".
func New(ctx context.Context, url string) (*Client, error) {
	if url == "" {
		return nil, nil
	}

	opts, err := goredis.ParseURL(url)
	if err != nil {
		return nil, fmt.Errorf("redis: parse url: %w", err)
	}

	c := goredis.NewClient(opts)
	if err := c.Ping(ctx).Err(); err != nil {
		_ = c.Close()
		return nil, fmt.Errorf("redis: ping: %w", err)
	}

	return &Client{c}, nil
}
