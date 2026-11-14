# G1-B4 — Identity Ownership ADR

**Status:** Proposed compatibility contract; no durable migration authorized

**Baseline:** `02318503907f3ed9b2787a0fdde9fc862bbae5d0`

## Decision boundary

This ADR defines how a later adapter may interpret identities. It does not change document creation, React persistence, Go persistence, file formats, or renderer IDs. Rust `logos-types::document::Document` remains the approved candidate semantic model, not the production identity authority.

## Repository audit

| Boundary | Observed behavior | Identity implication |
| --- | --- | --- |
| React `documentStore` | `PAGE_1_ID` and `addPage` use `nanoid()`; shape IDs use `crypto.randomUUID()` | The in-memory initial page is not UUID compatible; current state has no document ID. |
| Workspace route | `WorkspacePage` receives `documentId` from the route independently of `documentStore` | Route/server file identity is not part of the Zustand document state. |
| Document creation | Go `insertFileInTx` chooses the file UUID; `filedata.BuildEmptyData` stores it as `data.id` and stores a generated UUID page in `pages`/`pages-index` | Server-created durable file and page IDs already have UUID sources. |
| Offline snapshots | IndexedDB is keyed by caller-provided `documentId`; snapshots persist pages, page order, current page, and shapes but not a document ID inside the snapshot | Offline record identity is external metadata. Restoring it does not replace page or shape IDs. |
| XD / Sketch import | Importer-local maps preserve source IDs only within one conversion call; absent IDs and mapped IDs currently use fresh `crypto.randomUUID()` | Repeating an import can assign different Logos IDs. Source IDs are not automatically durable Logos IDs. |
| Rust candidate | Document/page/shape identities are UUIDs; `stable_import_id(s)` offers deterministic UUIDv5 mapping under a caller-owned namespace and rejects collisions | Candidate machinery can support migration without changing IDs, but the namespace and persisted provenance must be owned above the adapter. |
| Renderer projection | `NIL_RENDERER_ROOT_ID` is reserved for a synthetic root created in the renderer projection; input documents reject that ID | Nil is never a document, page, or user-shape ID. |
| Server revision | Go file `revn` is a monotonic revision number used for conflict detection/OT | Revision is not identity and must not enter document/page ID mapping. |

## Proposed identity contract

1. **Document ID:** use an existing durable file UUID when opening a server file. An offline-created document gets one UUID at the creation boundary and persists it with the offline record before any page is created. The adapter accepts it as input and never generates or repairs it.
2. **Page ID:** preserve the UUID already stored by the durable file. New pages are assigned UUIDs at the page-creation boundary. The current `nanoid()` initial/add-page identities are explicitly `LEGACY_NON_UUID`; do not rewrite them in memory or on save as a side effect of validation.
3. **Shape ID:** preserve existing UUIDs through ordinary editing, save/reload, and reparent. Duplicate/copy creates fresh shape UUIDs and rewrites only references that point within the copied subtree. Cross-document copy also allocates a new page identity when creating a destination page.
4. **Imported IDs:** preserve valid source UUIDs only when collision-free in the target document. Otherwise derive UUIDv5 from a stable, document-owned import namespace, source format, and stable source object ID. Persist source format, source ID, mapping version, namespace, and resulting Logos ID with the import provenance. Collision checks are mandatory; no order-based suffixing or random fallback.
5. **Legacy unknown IDs:** retain the original bytes/values in a compatibility envelope; report `LEGACY_ID_UNSUPPORTED` with object path and source provenance. Never coerce, regenerate, or silently omit them.
6. **Renderer ID:** the synthetic nil root exists only in the projection. Any source object with nil identity rejects before renderer upload and cannot become the document ID.
7. **Server revision:** carry `revn` separately as synchronization metadata. It is neither a stable object ID nor an import namespace.

## Migration and mapping lifetime

No existing saved file is rewritten by this proposal. A separately authorized migration may add an explicit versioned identity sidecar or rewrite a file only after an inventory proves all references and historical snapshots can be migrated atomically. Deterministic aliases must be stored or reproducible from a stable document namespace and immutable source IDs; their provenance remains available for the lifetime of any saved object derived from an import. Invalid source IDs without a stable source key remain unsupported.

## Open ownership decision

The Chief Architect must choose the creation/import boundary that owns client-offline document UUID issuance and durable React page UUID issuance. The repository currently has no single production function responsible for both. Until then, the adapter requires explicit document and page IDs and rejects live store state whose initial page uses `nanoid()`.
