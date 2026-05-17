;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.util.redis-cache
  "Redis read-through cache for hot RPC read paths.

  Usage — read path:
    (rsc/cache-get cfg (rsc/profile-key id) rsc/profile-ttl
                   #(db/get-by-id conn :profile id))

  Usage — write-invalidation (after every mutation):
    (rsc/cache-del cfg (rsc/profile-key id))

  Feature gate
  ------------
  Both `cache-get` and `cache-del` become no-ops unless the
  :redis-cache feature flag is present in LOGOS_FLAGS, so they can be
  called unconditionally in all environments.

  Cache key convention
  --------------------
    logos:cache:<entity>:<uuid>

  Invalidation discipline (from the CTO spec)
  -------------------------------------------
  TTL is a safety net.  Every write path that touches a cached entity
  MUST call `cache-del`.  Stale data is worse than a cache miss."
  (:require
   [app.common.logging :as log]
   [app.common.time :as ct]
   [app.common.transit :as t]
   [app.config :as cf]
   [app.redis :as rds]))

(log/set-level! :info)

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; TTL constants
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(def profile-ttl
  "5-minute TTL. Profiles change infrequently; explicit invalidation
  covers all write paths."
  (ct/duration {:minutes 5}))

(def team-ttl
  "10-minute TTL. Team metadata changes only during admin actions."
  (ct/duration {:minutes 10}))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Key helpers
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(defn profile-key [profile-id] (str "logos:cache:profile:" profile-id))
(defn team-key    [team-id]    (str "logos:cache:team:"    team-id))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Internal Redis helpers (called via rds/run!)
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(defn- redis-get
  [{:keys [::rds/conn]} k]
  (when-let [raw (rds/get conn k)]
    (t/decode-str raw)))

(defn- redis-set
  [{:keys [::rds/conn]} v k ttl]
  (rds/set conn k (t/encode-str v)
           (rds/build-set-args {:ex ttl})))

(defn- redis-del
  [{:keys [::rds/conn]} k]
  (rds/del conn k))

;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;
;; Public API
;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;

(defn cache-get
  "Read-through cache.

  Returns the cached Clojure value for `cache-key`, or calls
  `(fetch-fn)`, stores the result under `cache-key` with the given
  `ttl` Duration, and returns it.  Falls through to `(fetch-fn)`
  directly when :redis-cache is not in LOGOS_FLAGS."
  [cfg cache-key ttl fetch-fn]
  (if (contains? cf/flags :redis-cache)
    (or (do (log/trc :hint "rsc/get" :key cache-key)
            (rds/run! cfg redis-get cache-key))
        (let [value (fetch-fn)]
          (log/trc :hint "rsc/miss+store" :key cache-key)
          (rds/run! cfg redis-set value cache-key ttl)
          value))
    (fetch-fn)))

(defn cache-del
  "Invalidate `cache-key`.  No-op when :redis-cache flag is absent."
  [cfg cache-key]
  (when (contains? cf/flags :redis-cache)
    (log/trc :hint "rsc/del" :key cache-key)
    (rds/run! cfg redis-del cache-key))
  nil)
