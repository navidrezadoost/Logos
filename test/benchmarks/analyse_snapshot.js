/**
 * P1.7 — Heap snapshot analyser.
 *
 * Parses the `.heapsnapshot` JSON produced by Chrome's HeapProfiler and
 * extracts the metrics asserted by the memory benchmark.
 *
 * The snapshot format (stable across Chrome ≥ 80):
 *   snapshot.meta.node_fields  — names of per-node fields
 *   snapshot.meta.edge_fields  — names of per-edge fields
 *   snapshot.meta.node_types   — value lists for enum fields
 *   nodes[]   — flat array, each group of node_fields.length entries = 1 node
 *   edges[]   — flat array, each group of edge_fields.length entries = 1 edge
 *   strings[] — interned string table
 *
 * Reference: chromium/src/v8/src/profiler/heap-snapshot-generator.cc
 * Reference: Brendan Gregg — USE method applied to JavaScript heap analysis.
 */

"use strict";

// Node type enum indices (from V8 source, stable since Chrome 60)
const NODE_TYPE_HIDDEN      = 0;
const NODE_TYPE_ARRAY       = 1;
const NODE_TYPE_STRING      = 2;
const NODE_TYPE_OBJECT      = 3;
const NODE_TYPE_CODE        = 4;
const NODE_TYPE_CLOSURE     = 5;
const NODE_TYPE_REGEXP      = 6;
const NODE_TYPE_NUMBER      = 7;
const NODE_TYPE_NATIVE      = 8;   // includes Detached DOM nodes
const NODE_TYPE_SYNTHETIC   = 9;
const NODE_TYPE_BIGINT      = 13;

/**
 * Parse a raw HeapProfiler snapshot object (as returned by CDP
 * HeapProfiler.takeHeapSnapshot) into a summary of actionable metrics.
 *
 * @param {object} snapshot  — parsed JSON of the .heapsnapshot file
 * @returns {{ heapSize, nodeCount, detachedDomNodes, arrayBufferCount,
 *             arrayBufferBytes, stringCount, objectCount }}
 */
function analyseSnapshot(snapshot) {
  const { nodes, edges, strings, snapshot: meta } = snapshot;

  const nodeFields     = meta.meta.node_fields;
  const nodeTypes      = meta.meta.node_types;
  const edgeFields     = meta.meta.edge_fields;
  const nodeFieldCount = nodeFields.length;
  const edgeFieldCount = edgeFields.length;

  // Field indices
  const F_NODE_TYPE       = nodeFields.indexOf("type");
  const F_NODE_NAME       = nodeFields.indexOf("name");
  const F_NODE_ID         = nodeFields.indexOf("id");
  const F_NODE_SELF_SIZE  = nodeFields.indexOf("self_size");
  const F_NODE_EDGE_COUNT = nodeFields.indexOf("edge_count");

  // The "type" field is an enum; node_types[0] is the list of type names.
  const nodeTypeNames = nodeTypes[F_NODE_TYPE] || [];

  const F_EDGE_TYPE   = edgeFields.indexOf("type");
  const F_EDGE_NAME   = edgeFields.indexOf("name_or_index");
  const F_EDGE_TO     = edgeFields.indexOf("to_node");

  let totalHeapSize    = 0;
  let nodeCount        = 0;
  let detachedDomNodes = 0;
  let arrayBufferCount = 0;
  let arrayBufferBytes = 0;
  let stringCount      = 0;
  let objectCount      = 0;

  const nodeCount_ = nodes.length / nodeFieldCount;

  for (let i = 0; i < nodeCount_; i++) {
    const base      = i * nodeFieldCount;
    const typeIdx   = nodes[base + F_NODE_TYPE];
    const nameIdx   = nodes[base + F_NODE_NAME];
    const selfSize  = nodes[base + F_NODE_SELF_SIZE];
    const typeName  = nodeTypeNames[typeIdx] || String(typeIdx);
    const name      = strings[nameIdx] || "";

    totalHeapSize += selfSize;
    nodeCount++;

    if (typeName === "native" || typeName === "hidden") {
      // Detached DOM nodes appear as native nodes whose name starts with
      // "Detached" in Chrome's heap format.
      if (name.startsWith("Detached")) {
        detachedDomNodes++;
      }
    }

    if (typeName === "object") {
      objectCount++;
      // ArrayBuffer nodes have name "ArrayBuffer" in the snapshot.
      if (name === "ArrayBuffer") {
        arrayBufferCount++;
        arrayBufferBytes += selfSize;
      }
    }

    if (typeName === "string" || typeName === "concatenated-string" ||
        typeName === "sliced-string") {
      stringCount++;
    }
  }

  return {
    heapSize:        totalHeapSize,
    nodeCount,
    detachedDomNodes,
    arrayBufferCount,
    arrayBufferBytes,
    stringCount,
    objectCount,
  };
}

/**
 * Compare a baseline and final snapshot and compute growth deltas.
 *
 * @returns {{ heapGrowth, detachedDomNodes, arrayBufferGrowth, arrayBufferByteGrowth }}
 */
function compareSnapshots(baseline, final_) {
  return {
    heapGrowth:            final_.heapSize        - baseline.heapSize,
    detachedDomNodes:      final_.detachedDomNodes,
    arrayBufferGrowth:     final_.arrayBufferCount - baseline.arrayBufferCount,
    arrayBufferByteGrowth: final_.arrayBufferBytes - baseline.arrayBufferBytes,
  };
}

module.exports = { analyseSnapshot, compareSnapshots };
