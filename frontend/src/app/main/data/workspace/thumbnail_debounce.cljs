;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.main.data.workspace.thumbnail-debounce
  "Per-frame debounce registry for thumbnail regeneration.

  Maintains a map of pending timer jobs keyed by [file-id page-id frame-id tag].
  When a new change arrives for a frame that already has a pending job, the
  existing timer is cancelled and a fresh 2-second window begins.  This prevents
  queuing stale renders during rapid editing while still guaranteeing every dirty
  frame is eventually re-rendered.

  The thunk passed to `schedule-update!` is called on a JS timer thread and is
  responsible for dispatching the appropriate PTK event (e.g. via
  `app.main.store/emit!`). This module intentionally has no dependency on the
  store so it can be unit-tested in isolation."
  (:require
   [app.common.logging :as log]
   [app.util.timers :as tm]))

(log/set-level! :warn)

;; --- State

(defonce ^:private !pending
  ;; Map of {[file-id page-id frame-id tag] -> IDisposable}
  (atom {}))

;; --- Public API

(defn schedule-update!
  "Schedule `thunk` to run after `delay-ms` for the given job key.
  If a job is already pending for this key it is cancelled first, giving
  a fresh debounce window."
  [job-key delay-ms thunk]
  (when-let [existing (get @!pending job-key)]
    (log/trc :hint "cancel pending thumbnail job" :key (str job-key))
    (tm/dispose! existing))
  (let [timer (tm/schedule delay-ms thunk)]
    (swap! !pending assoc job-key timer)
    nil))

(defn cancel-update!
  "Cancel any pending thumbnail regeneration for `job-key`."
  [job-key]
  (when-let [existing (get @!pending job-key)]
    (log/trc :hint "cancel thumbnail job" :key (str job-key))
    (tm/dispose! existing)
    (swap! !pending dissoc job-key)
    nil))

(defn complete-job!
  "Remove a job from the pending map once it has fired, freeing the slot.
  Should be called from inside the thunk after dispatching the event."
  [job-key]
  (swap! !pending dissoc job-key)
  nil)

(defn clear-all!
  "Cancel all pending thumbnail jobs.  Call on page finalisation to avoid
  orphaned timers that would dispatch against a dead page."
  []
  (log/dbg :hint "clearing all pending thumbnail debounce jobs"
           :count (count @!pending))
  (doseq [[_ timer] @!pending]
    (tm/dispose! timer))
  (reset! !pending {})
  nil)

(defn pending-count
  "Return the number of frames currently queued (useful for tests/logging)."
  []
  (count @!pending))
