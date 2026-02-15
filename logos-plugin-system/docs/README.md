# Logos Plugin System Documentation

Welcome to the Logos Plugin System documentation. This is a comprehensive resource for building, distributing, and managing Logos design platform plugins.

---

## Quick Start

**New to Logos plugins? Start here:**

1. [Getting Started Guide](guides/getting-started.md) — Build your first plugin in 5 minutes
2. [JavaScript API](api/javascript-api.md) — The `Logos.*` global object reference
3. [Example Plugins](examples/) — 5 complete working examples

---

## Guides

| Guide | Description |
|-------|-------------|
| [Getting Started](guides/getting-started.md) | Create, package, and run your first plugin |
| [Publishing Guide](guides/publishing-guide.md) | Publish to the Logos Marketplace |
| [Architecture Overview](guides/architecture.md) | Deep dive into system internals |

---

## API Reference

| Reference | Description |
|-----------|-------------|
| [JavaScript API](api/javascript-api.md) | The `Logos.*` global object — document, selection, undo, events |
| [UI Components](api/ui-components.md) | Declarative panel UI — components, messages, permissions |
| [Events](api/events.md) | Event system — listeners, rate limiting, lifecycle hooks |
| [Permissions](api/permissions.md) | Capability-based security — kinds, declarations, runtime checks |
| [Manifest](api/manifest.md) | Plugin manifest schema — fields, validation, hooks, commands |
| [Packaging](api/packaging.md) | Binary package format — creation, icons, serialization |
| [Signing](api/signing.md) | Cryptographic signing — keys, signatures, verification |
| [Marketplace](api/marketplace.md) | Marketplace client — publish, search, download, trust |
| [Host Functions](api/host-functions.md) | Low-level sandbox API — host fns, PluginValue, resources |

---

## Example Plugins

| Example | Demonstrates | Complexity |
|---------|-------------|------------|
| [Hello World](examples/01-hello-world/) | Panel, buttons, notifications | Beginner |
| [Layer Counter](examples/02-layer-counter/) | Document access, events, live stats | Beginner |
| [Color Palette](examples/03-color-palette/) | Document modification, color picker | Intermediate |
| [Export Helper](examples/04-export-helper/) | File system, async operations | Intermediate |
| [Animation Tool](examples/05-animation-tool/) | Real-time updates, timers, events | Advanced |

---

## Performance

The Logos plugin system is designed for sub-microsecond operations:

| Operation | Latency |
|-----------|---------|
| Plugin sandbox creation | 41ns |
| Permission check | 10ns |
| UI panel creation | 191ns |
| Marketplace search | 5.32µs |
| Package signing | 3.37µs |
| Publisher trust check | 19.6ns |

See the [Architecture Guide](guides/architecture.md) for detailed performance analysis.

---

## Security

Logos plugins operate under a strict capability-based security model:

- **Declared permissions** — Plugins list required capabilities in their manifest
- **Runtime enforcement** — Every host API call is permission-checked (~10ns)
- **Resource limits** — Memory, time, and call count bounded
- **Sandboxed execution** — No shared state between plugins
- **Signed packages** — Cryptographic integrity and authenticity
- **Publisher trust** — Verified publishers with revocation support

See the [Permissions Reference](api/permissions.md) for details.
