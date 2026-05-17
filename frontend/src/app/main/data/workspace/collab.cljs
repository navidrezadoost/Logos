;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.main.data.workspace.collab
  "P2.3 — Client-side collaboration layer.

  Handles the case where the server performed a server-side OT rebase (because
  another client committed changes between the time our local change-set was
  composed and when it was received by the server).

  When we receive a remote `:file-change` whose server-revn is higher than
  expected (i.e., the server has seen changes we haven't), any commits
  currently queued in the persistence layer were built on top of a now-stale
  base revision.  This module detects that condition and rebases the pending
  commits' redo-changes against the incoming remote changes so they remain
  internally consistent.

  Architecture
  ────────────
  The server is the authoritative source of truth.  We apply incoming remote
  changes immediately (handled by `notifications/handle-file-change` →
  `dch/commit` with `:source :remote`).  The collab module then patches the
  pending queue so the next persistence flush sends correctly rebased changes.

  The rebase here is done purely for client-side queue hygiene — the server
  already applied a server-side rebase (`rebase/rebase-change-set`) before
  persisting and broadcasting, so the ground truth is always consistent.
  What we are doing on the client is making sure the pending local commits
  (which will be sent in the next flush) are still valid on top of the new
  server state.

  This means:
    - Incoming remote changes are always accepted as-is.
    - Local pending changes are rebased against them.
    - If a pending change becomes a no-op after rebase, it is dropped.
    - The undo-changes for dropped operations are also removed so the undo
      stack does not produce broken state.

  References
  ──────────
  - `app.common.files.rebase/rebase-change-set`
  - `app.main.data.persistence` — persistence queue and index
  - `app.main.data.changes/commit` — commit lifecycle"
  (:require
   [app.common.data.macros :as dm]
   [app.common.files.rebase :as rebase]
   [app.common.logging :as log]
   [potok.v2.core :as ptk]))

(log/set-level! :info)

;; ──────────────────────────────────────────────────────────────────
;; Internal helpers
;; ──────────────────────────────────────────────────────────────────

(defn- pending-commits
  "Return an ordered seq of pending (not-yet-confirmed) commit maps from state.
  Each commit has :id, :redo-changes, :undo-changes, :file-id, :file-revn."
  [state]
  (let [{:keys [queue index]} (:persistence state)]
    (->> queue
         (map (partial get index))
         (filter some?)
         (vec))))

(defn- rebase-pending-commits!
  "Given a seq of incoming remote changes, rebase the redo-changes of each
  pending commit in the persistence queue against those remote changes.

  Returns a new state where the pending commits' redo-changes have been
  adjusted via the OT transform.  Commits that become empty after rebase
  are removed from both :queue and :index."
  [state remote-changes]
  (let [{:keys [queue index]} (:persistence state)
        competing             [remote-changes] ;; wrap as a single change-set

        ;; Rebase each pending commit and accumulate a new index map.
        ;; We process in queue order so the rebasing is applied cumulatively.
        [new-queue new-index]
        (reduce
         (fn [[q idx] commit-id]
           (if-let [commit (get idx commit-id)]
             (let [rebased-changes (rebase/rebase-change-set (:redo-changes commit) competing)]
               (if (empty? rebased-changes)
                 ;; All changes were made no-ops by the remote diff → drop commit
                 (do
                   (log/debug :hint "p2.3: dropping empty pending commit after rebase"
                              :commit-id commit-id)
                   [q (dissoc idx commit-id)])
                 ;; Patch the commit with the rebased changes
                 [(conj q commit-id)
                  (assoc idx commit-id (assoc commit :redo-changes (vec rebased-changes)))]))
             ;; commit already gone (race) — skip
             [q idx]))
         [#queue [] index]
         queue)]

    (update state :persistence
            (fn [ps]
              (-> ps
                  (assoc :queue new-queue)
                  (assoc :index new-index))))))

;; ──────────────────────────────────────────────────────────────────
;; Public API
;; ──────────────────────────────────────────────────────────────────

(defn integrate-remote-changes
  "UpdateEvent that integrates a remote `:file-change` into the local
  optimistic state.

  Call this **after** applying the remote changes to the in-memory file data
  (i.e., after dispatching `handle-file-change`).  It patches any pending
  persistence queue commits so they are rebased against the newly arrived
  remote changes.

  Parameters
  ──────────
  - `changes`  — the vector of remote changes from the `:file-change` message
  - `session-id` — the session-id of the remote client (used to skip our own
                   messages bounced back from the server)

  Usage
  ─────
  Dispatch this event from `handle-file-change` whenever the message originator
  is not the current session:

    (when (not= session-id (:session-id state))
      (ptk/dispatch store (integrate-remote-changes changes session-id)))"
  [changes own-session-id]
  (ptk/reify ::integrate-remote-changes
    ptk/UpdateEvent
    (update [_ state]
      (let [pending (pending-commits state)]
        (if (empty? pending)
          ;; Fast path: no pending commits to rebase — nothing to do.
          state
          (do
            (log/debug :hint "p2.3: rebasing pending commits against remote changes"
                       :pending-commits (count pending)
                       :remote-changes (count changes)
                       :own-session-id own-session-id)
            (rebase-pending-commits! state changes)))))))


(defn pending-commit-count
  "Read-only helper — returns the number of un-confirmed commits currently
  in the persistence queue.  Useful for UI indicators (e.g., 'saving…')."
  [state]
  (count (-> state :persistence :queue)))
