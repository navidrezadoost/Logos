;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.main.data.workspace.layout-cache
  "P1.3 — Incremental layout dirty-flag optimisation.

  Maintains a per-frame version counter and a layout-result cache.  When a
  change does NOT touch any geometry attribute the frame's version counter is
  NOT incremented, so the layout engine can skip re-computation and serve the
  cached modifier tree.

  Invariants:
  - The cache is keyed on `[frame-id version]`.
  - The version counter is per-frame.
  - Any geometry-affecting attribute write increments the counter, which
    automatically invalidates the cache entry for that frame.
  - Pan/zoom and purely visual changes (colour, shadow, blur, opacity) leave
    the counter unchanged, so the cached result is re-used.

  API:
    (get-frame-version frame-id)  → integer (0 on first access)
    (bump-frame-version! frame-id)
    (cache-layout!  frame-id modifiers)
    (lookup-layout  frame-id) → modifiers | nil

  The cache is wiped entirely on page switch (clear-all!) and on explicit
  invalidation requests."
  (:require
   [app.common.files.geometry-affecting :as gaf]))

;; ---------------------------------------------------------------------------
;; State
;; ---------------------------------------------------------------------------

(defonce ^:private !versions  (atom {}))  ; frame-id → integer
(defonce ^:private !cache     (atom {}))  ; [frame-id version] → modifiers

;; ---------------------------------------------------------------------------
;; Version counter
;; ---------------------------------------------------------------------------

(defn get-frame-version
  "Return the current version counter for *frame-id*.  Defaults to 0."
  [frame-id]
  (get @!versions frame-id 0))

(defn bump-frame-version!
  "Increment the version counter for *frame-id*, effectively invalidating any
  cached layout result for that frame."
  [frame-id]
  (swap! !versions update frame-id (fnil inc 0)))

(defn bump-frames-from-changes!
  "Inspect a collection of change-set maps and bump the version counter for
  every shape whose geometry attributes changed.

  Call this immediately after applying a change-set to local state.

  Only shapes that are direct layout containers need to be considered; for
  simplicity we bump the parent frame of every affected object.  The layout
  engine itself is responsible for walking ancestors."
  [changes objects]
  (doseq [{:keys [type id operations] :as change} changes]
    (when (gaf/geometry-affecting-change? change)
      ;; For mod-obj, bump the shape's own frame parent.
      ;; For add/del/mov, bump the id directly (it may be a frame).
      (let [frame-id (case type
                       :mod-obj (or (get-in objects [id :frame-id]) id)
                       (:add-obj :del-obj :mov-obj) id
                       nil)]
        (when (some? frame-id)
          (bump-frame-version! frame-id))))))

;; ---------------------------------------------------------------------------
;; Layout result cache
;; ---------------------------------------------------------------------------

(defn cache-layout!
  "Store the computed modifier tree for *frame-id* at its current version.
  If the version has already been bumped (i.e., a geometry change arrived
  while layout was being computed), the entry is immediately stale and will
  not survive the next `lookup-layout` call."
  [frame-id modifiers]
  (let [version (get-frame-version frame-id)]
    (swap! !cache assoc [frame-id version] modifiers)))

(defn lookup-layout
  "Return the cached modifier tree for *frame-id* at its *current* version,
  or `nil` if no valid cache entry exists.

  A cache miss means the layout must be recomputed."
  [frame-id]
  (let [version (get-frame-version frame-id)]
    (get @!cache [frame-id version])))

(defn invalidate-frame!
  "Explicitly invalidate the cache for *frame-id* (e.g., after the user
  changes a layout property in the property panel)."
  [frame-id]
  (bump-frame-version! frame-id))

;; ---------------------------------------------------------------------------
;; Page lifecycle
;; ---------------------------------------------------------------------------

(defn clear-all!
  "Wipe the entire cache and version table.  Call on page switch."
  []
  (reset! !versions {})
  (reset! !cache {}))

(defn stats
  "Return a diagnostic snapshot — useful during development and benchmarking."
  []
  {:version-count (count @!versions)
   :cache-size    (count @!cache)})
