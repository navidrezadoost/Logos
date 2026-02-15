# Logos Plugin API Reference

Complete API documentation for Logos plugin development.

## Quick Links

| Document | Description |
|----------|-------------|
| [JavaScript API](javascript-api.md) | The `Logos.*` global object available in plugins |
| [Host Functions](host-functions.md) | Low-level sandbox host function reference |
| [Permissions](permissions.md) | Capability-based security model |
| [Manifest](manifest.md) | Plugin manifest schema (`manifest.json`) |
| [Packaging](packaging.md) | The `.logos-plugin` binary package format |
| [Signing](signing.md) | Cryptographic signing and verification |
| [Marketplace](marketplace.md) | Marketplace client API |
| [UI Components](ui-components.md) | Declarative panel UI system |
| [Events](events.md) | Event bus and lifecycle hooks |

## Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                  Plugin Code                     │
│              (JavaScript ES2023)                  │
├──────────────┬──────────────┬────────────────────┤
│  Logos.* API │  Logos.ui.*  │  Logos.on(event)   │
├──────────────┴──────────────┴────────────────────┤
│              Permission Guard                     │
├──────────────────────────────────────────────────┤
│  Host API  │  UI Bridge  │  Event Bus           │
├──────────────────────────────────────────────────┤
│              Plugin Manager                       │
├──────────────────────────────────────────────────┤
│  Sandbox Runtime  │  Boa JavaScript Engine       │
├──────────────────────────────────────────────────┤
│              logos-core (Document, Node, CRDT)    │
└──────────────────────────────────────────────────┘
```

## Performance Characteristics

| Operation | Typical Latency | Notes |
|-----------|----------------|-------|
| Sandbox creation | ~41ns | Near-instantaneous isolation |
| Permission check | ~10ns | Bitmap lookup |
| Host function call | ~2–9µs | Depends on operation |
| JS evaluation | ~124µs | First parse; cached: ~1.3µs |
| UI panel creation | ~191ns | Declarative component tree |
| UI message roundtrip | ~382ns | Plugin ↔ panel |
| Plugin publish | ~2.85µs | Package + sign + index |
| Marketplace search | ~5.32µs | Full-text across thousands |
| Package download | ~2.34µs | From cache with verification |
| Trust check | ~19.6ns | Publisher bitmap + bloom filter |
