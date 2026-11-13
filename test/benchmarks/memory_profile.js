#!/usr/bin/env node
"use strict";

// Reserved for the separately approved editor-runtime measurement integration.
// The historical scenario assumed unsupported login/dashboard/import selectors
// and is intentionally not executed or treated as benchmark evidence.
console.error(
  "BLOCKED: no supported browser workspace is available. Run readiness_preflight.js for capability evidence; no heap measurement was performed."
);
process.exit(2);
