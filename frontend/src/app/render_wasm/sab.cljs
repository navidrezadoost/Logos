;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.render-wasm.sab
  "SharedArrayBuffer capability detection and allocation helpers for P1.2.

  SharedArrayBuffer is only available when the page is cross-origin isolated:
    Cross-Origin-Opener-Policy: same-origin
    Cross-Origin-Embedder-Policy: require-corp

  All sub-resources (WASM, fonts, images) must additionally carry a
  Cross-Origin-Resource-Policy header of `same-origin` or `cross-origin`.

  Use `enabled?` to gate any code that allocates or reads SharedArrayBuffers.
  When `enabled?` is false (development without HTTPS, or missing CORP on a
  sub-resource) the caller should fall back to the Transferable postMessage
  path implemented in P1.1."
  (:require
   [app.common.logging :as log]))

(log/set-level! :info)

;; ---------------------------------------------------------------------------
;; Capability detection
;; ---------------------------------------------------------------------------

(def ^:const sab-supported?
  "True when `SharedArrayBuffer` is defined in the current JavaScript
  environment (browser or worker).  Does NOT imply the page is cross-origin
  isolated — use `enabled?` for that check."
  (exists? js/SharedArrayBuffer))

(def ^:const cross-origin-isolated?
  "True when the page was served with the COOP + COEP headers required for
  `SharedArrayBuffer`.  This is the authoritative runtime check — feature
  flags and headers can be set incorrectly, but this reflects what the
  browser actually enforces."
  (and (exists? js/self)
       (true? (unchecked-get js/self "crossOriginIsolated"))))

(def ^:const enabled?
  "True when SharedArrayBuffer is both available and allowed by the browser's
  cross-origin isolation policy.  Use this to gate the zero-copy WASM buffer
  path.

  Fallback: when false, the P1.1 Transferable postMessage path is used."
  (and sab-supported? cross-origin-isolated?))

;; Report status at module load time so developers can see whether the
;; optimised path will activate.
(if enabled?
  (log/info :hint "SharedArrayBuffer enabled — zero-copy WASM path active")
  (if-not sab-supported?
    (log/warn :hint "SharedArrayBuffer not available in this environment")
    (log/warn :hint "crossOriginIsolated=false — SharedArrayBuffer disabled"
              :tip "Ensure backend serves COOP/COEP headers; enable :cross-origin-isolation flag.")))

;; ---------------------------------------------------------------------------
;; Allocation helpers  (used once SAB write path is wired into the worker)
;; ---------------------------------------------------------------------------

(def ^:private ^:const DEFAULT-BUFFER-SIZE (* 16 1024 1024)) ; 16 MB

(defonce ^:private sab-pool (atom nil))

(defn- create-sab
  "Allocate a fresh SharedArrayBuffer of *size* bytes.  Throws if SAB is not
  available — always guard with `enabled?` before calling."
  ([]
   (create-sab DEFAULT-BUFFER-SIZE))
  ([size]
   {:pre [enabled?]}
   (js/SharedArrayBuffer. size)))

(defn acquire-buffer
  "Return the pooled SharedArrayBuffer, creating it on first call.
  Returns nil when `enabled?` is false."
  []
  (when enabled?
    (or @sab-pool
        (let [buf (create-sab)]
          (reset! sab-pool buf)
          buf))))

(defn release-buffer!
  "Explicitly release the pooled buffer (e.g., on module teardown).
  The buffer itself is not freed (GC responsibility), but the pool
  reference is cleared so the next `acquire-buffer` call allocates a
  fresh one."
  []
  (reset! sab-pool nil))

;; ---------------------------------------------------------------------------
;; CI smoke-test export
;; ---------------------------------------------------------------------------

(defn ^:export check
  "Called by the CI smoke test to verify the page is cross-origin isolated.
  Returns a JS object with diagnostic fields."
  []
  #js {:sabSupported       sab-supported?
       :crossOriginIsolated cross-origin-isolated?
       :enabled             enabled?})
