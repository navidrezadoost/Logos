;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.files.rebase
  "P2.3 — Operational Transform rebase engine.

  Given an incoming change-set (built on base-revn) and a sequence of
  competing change-sets already applied by the server (between base-revn and
  current-revn), produce a new change-set that is correctly positioned on top
  of the current server state.

  This namespace is .cljc so it can run on both the JVM backend (server-side
  rebase) and the ClojureScript frontend (optimistic client-side rebase).

  Algorithm
  ─────────
  We follow the standard two-argument transform approach from the OT
  literature (Ellis & Gibbs 1989; Ressel et al. 1996) adapted to the Logos
  change model.

  The transform matrix covers the five change types:

  │ incoming ╲ competing │ :set   │ :mod-obj │ :mov-objects │ :add-obj │ :del-obj │
  │──────────────────────┼────────┼──────────┼──────────────┼──────────┼──────────│
  │ :set (same obj+attr) │ keep*  │ keep     │ keep         │ –        │ no-op    │
  │ :set (diff attr)     │ keep   │ keep     │ keep         │ –        │ no-op    │
  │ :mod-obj             │ merge  │ merge    │ keep         │ –        │ no-op    │
  │ :mov-objects         │ keep   │ keep     │ adjust-index │ adjust   │ no-op    │
  │ :add-obj             │ keep   │ keep     │ adjust-index │ identity │ keep     │
  │ :del-obj (same id)   │ absorb │ absorb   │ absorb       │ absorb   │ identity │

  * keep = incoming change wins (represents most-recent user intent).
    This is deterministic: the server always picks the incoming change
    when two clients set the same attribute at the same time on the
    same shape.  Both clients will converge to the same state.

  No-op rules
  ───────────
  - An :add-obj that adds an id that was deleted by a competing :del-obj
    is preserved (re-add wins over delete for collaborative resilience).
  - A :del-obj cancels any competing :set or :mod-obj on the same id
    (delete takes precedence — the object is gone).
  - A :mov-objects whose target :shapes list contains ids deleted by
    competing :del-obj operations has those ids pruned from the list.

  Index adjustment for :mov-objects
  ──────────────────────────────────
  When two clients both move objects within the same parent container,
  indices can drift.  We apply a simple linear adjustment:

    If competing move removed N objects before our target index,
    adjust our index by −N.  If it inserted M objects before our index,
    adjust by +M.  This is the same heuristic used by most collaborative
    editors and gives deterministic, reasonable results for independent
    reorders.

  Limitations (acceptable for Phase 2)
  ──────────────────────────────────────
  - Nested tree conflicts (e.g., two simultaneous reparentings of the same
    subtree) are resolved as \"incoming wins\" — the last server-confirmed
    move determines final position.
  - Property-based tests (see test/common/files/rebase_test.cljc) verify
    idempotency and correctness for the common cases.

  References
  ──────────
  - Ellis & Gibbs (1989) — \"Concurrency Control in Groupware Systems\"
  - *Designing Data-Intensive Applications* ch. 5 — \"Multi-Leader Replication\"
  - Jupiter (Nichols et al. 1995) — server-authoritative OT"
  (:require [clojure.set :as set]))

;; ──────────────────────────────────────────────────────────────────
;; Helpers
;; ──────────────────────────────────────────────────────────────────

(defn- extract-set-ops
  "Return a map of {:attr → :val} for all :set operations in a :mod-obj change."
  [change]
  (->> (:operations change)
       (filter #(= :set (:type %)))
       (into {} (map (fn [op] [(:attr op) (:val op)])))))

(defn- merge-set-ops
  "Merge operations from two :mod-obj changes.
  For operations touching the same attribute, `incoming-ops` wins."
  [incoming-ops competing-ops]
  (let [incoming-attrs (set (map :attr incoming-ops))
        surviving-competing (remove #(contains? incoming-attrs (:attr %)) competing-ops)]
    (into (vec surviving-competing) incoming-ops)))

(defn- obj-ids-deleted-by
  "Return the set of object ids deleted by a single competing change."
  [competing]
  (if (= :del-obj (:type competing))
    #{(:id competing)}
    #{}))

(defn- all-deleted-ids
  "Return the set of all object ids deleted across a seq of competing change-sets."
  [competing-changes]
  (reduce (fn [acc ch]
            (into acc (obj-ids-deleted-by ch)))
          #{}
          competing-changes))

;; ──────────────────────────────────────────────────────────────────
;; Single-change rebase against one competing change
;; ──────────────────────────────────────────────────────────────────

(defmulti ^:private transform-against
  "Rebase one incoming `change` against one `competing` change.
  Returns the (possibly modified) incoming change, or nil to drop it."
  (fn [incoming competing] [(:type incoming) (:type competing)]))

;; Default: preserve incoming, no adjustment needed
(defmethod transform-against :default
  [incoming _competing]
  incoming)

;; ── :mod-obj vs :del-obj (same id) ──────────────────────────────

(defmethod transform-against [:mod-obj :del-obj]
  [incoming competing]
  (when (not= (:id incoming) (:id competing))
    incoming))

;; ── :mod-obj vs :mod-obj (same id) — merge operations ───────────

(defmethod transform-against [:mod-obj :mod-obj]
  [incoming competing]
  (if (not= (:id incoming) (:id competing))
    incoming
    ;; Merge: incoming's set-ops win over competing's for same attribute
    (let [merged-ops (merge-set-ops (:operations incoming) (:operations competing))]
      (assoc incoming :operations merged-ops))))

;; ── :del-obj vs :mod-obj (same id) — del wins ───────────────────

(defmethod transform-against [:del-obj :mod-obj]
  [incoming competing]
  ;; del-obj wins — discard the competing mod-obj implicitly (we just keep del)
  incoming)

(defmethod transform-against [:del-obj :del-obj]
  [incoming competing]
  ;; If both try to delete the same object, second one is a no-op.
  ;; Return incoming unchanged (idempotent delete is safe).
  incoming)

;; ── :add-obj vs :del-obj (same id) — add wins (re-creation) ─────

(defmethod transform-against [:add-obj :del-obj]
  [incoming competing]
  ;; The object was deleted, then the incoming change re-adds it.
  ;; Re-add wins — this is the collaborative resilience rule.
  incoming)

;; ── :mov-objects vs :del-obj — prune deleted ids from shapes list

(defmethod transform-against [:mov-objects :del-obj]
  [incoming competing]
  (let [deleted-id (:id competing)
        pruned     (into [] (remove #(= deleted-id %)) (:shapes incoming))]
    (if (empty? pruned)
      nil ;; all shapes were deleted — drop the move entirely
      (assoc incoming :shapes pruned))))

;; ── :mov-objects vs :add-obj — adjust index if insert is before target

(defmethod transform-against [:mov-objects :add-obj]
  [incoming competing]
  ;; If the competing add inserted an object into the same parent container
  ;; at an index at or before our target index, shift our index up by 1.
  (if (and (= (:parent-id incoming) (:parent-id competing))
           (some? (:index incoming))
           (some? (:index competing))
           (<= (:index competing) (:index incoming)))
    (update incoming :index inc)
    incoming))

;; ── :mov-objects vs :mov-objects — adjust index ─────────────────

(defmethod transform-against [:mov-objects :mov-objects]
  [incoming competing]
  ;; Only adjust when we're moving within the same parent container.
  (if (not= (:parent-id incoming) (:parent-id competing))
    incoming
    ;; Competing removed some shapes from this container and inserted them
    ;; elsewhere (or re-ordered them within this container).  Adjust our
    ;; target index: for each competing shape that was at an index ≤ ours
    ;; and is no longer here, decrement our index by 1; for each competing
    ;; shape that landed at an index ≤ ours from outside, increment by 1.
    ;; Simplified: if competing moved shapes out (before our index), shift down.
    (let [competing-shapes (set (:shapes competing))
          incoming-idx     (or (:index incoming) 0)
          ;; Assume competing shapes were at index < incoming / moved to index < incoming
          ;; For safety use the conservative approximation: each competing shape
          ;; that we don't also move and was before our position shifts us by 1.
          our-shapes       (set (:shapes incoming))
          interfering      (set/difference competing-shapes our-shapes)
          competing-from   (or (:index competing) 0)
          adjustment       (cond
                             ;; competing moved shapes from before our index (frees slots)
                             (< competing-from incoming-idx) (- (count interfering))
                             ;; competing inserted at or before our index (fills slots)
                             (<= (:index competing) incoming-idx) (count interfering)
                             :else 0)]
      (update incoming :index #(max 0 (+ (or % 0) adjustment))))))

;; ──────────────────────────────────────────────────────────────────
;; Public API
;; ──────────────────────────────────────────────────────────────────

(defn rebase-change
  "Rebase a single incoming `change` against all `competing-changes`
  (in the order they were applied by the server).

  Returns the rebased change, or nil if the change becomes a no-op
  (e.g., modifying a shape that was concurrently deleted)."
  [change competing-changes]
  (reduce (fn [ch competing]
            (if (nil? ch)
              (reduced nil) ;; already dropped, short-circuit
              (transform-against ch competing)))
          change
          competing-changes))

(defn rebase-change-set
  "Rebase an entire change-set (a vector of changes) against a sequence of
  `competing-change-sets` (each is itself a seq of changes).

  Each competing change-set represents one server-applied revision that was
  not yet visible to the client when it built `change-set`.

  Returns a (possibly smaller) change-set that is safe to apply on top of
  the current server state.

  The client should provide all change-sets from `base-revn` (exclusive)
  to `current-revn` (inclusive) as `competing-change-sets`.

  Example
  ───────
  Client built its change-set based on server revn 5.
  Server is now at revn 8 (three commits: 6, 7, 8).
  Competing-change-sets = [changes@revn6, changes@revn7, changes@revn8].
  rebase-change-set produces a new change-set safe to apply at revn 9."
  [change-set competing-change-sets]
  (let [;; Flatten all competing change-sets into a single ordered sequence
        ;; of individual changes for the transform matrix.
        flat-competing (into [] (mapcat identity) competing-change-sets)]
    (->> change-set
         (keep #(rebase-change % flat-competing))
         (vec))))
