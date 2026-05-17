;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.http.security
  "Additional security layer middlewares"
  (:require
   [app.config :as cf]
   [yetti.request :as yreq]
   [yetti.response :as yres]))

(def ^:private safe-methods
  #{:get :head :options})

(defn- wrap-sec-fetch-metadata
  "Sec-Fetch metadata security layer middleware"
  [handler]
  (fn [request]
    (let [site (yreq/get-header request "sec-fetch-site")]
      (cond
        (= site "same-origin")
        (handler request)

        (or (= site "same-site")
            (= site "cross-site"))
        (if (contains? safe-methods (yreq/method request))
          (handler request)
          {::yres/status 403})

        :else
        (handler request)))))

(def sec-fetch-metadata
  {:name ::sec-fetch-metadata
   :compile (fn [_ _]
              (when (contains? cf/flags :sec-fetch-metadata-middleware)
                wrap-sec-fetch-metadata))})

(defn- wrap-client-header-check
  "Check for a penpot custom header to be present as additional CSRF
  protection"
  [handler]
  (fn [request]
    (let [client (yreq/get-header request "x-client")]
      (if (some? client)
        (handler request)
        {::yres/status 403}))))

(def client-header-check
  {:name ::client-header-check
   :compile (fn [_ _]
              (when (contains? cf/flags :client-header-check-middleware)
                wrap-client-header-check))})

;; ---------------------------------------------------------------------------
;; P1.2 — Cross-Origin Isolation headers (required for SharedArrayBuffer)
;; ---------------------------------------------------------------------------
;; Browsers only expose SharedArrayBuffer when the document is cross-origin
;; isolated, which requires both:
;;   Cross-Origin-Opener-Policy: same-origin
;;   Cross-Origin-Embedder-Policy: require-corp
;;
;; All static assets (WASM, fonts, images) must additionally send either
;;   Cross-Origin-Resource-Policy: same-origin     (same-origin assets)
;;   Cross-Origin-Resource-Policy: cross-origin    (CDN / external assets)
;;
;; Enable by setting the :cross-origin-isolation flag in LOGOS_FLAGS.
;; Requires that ALL sub-resources are either same-origin or served with
;; an appropriate CORP header.  An in-CI smoke test checks
;;   `window.crossOriginIsolated === true`
;; before the SharedArrayBuffer path is activated.

(defn- wrap-cross-origin-isolation
  "Adds COOP + COEP headers to every response, enabling SharedArrayBuffer
  in all modern browsers.  Responses for sub-resources (assets, fonts,
  WASM) also receive a CORP: same-origin header so they are loadable in
  the isolated context."
  [handler]
  (fn [request]
    (let [response (handler request)
          uri      (str (yreq/path request))
          ;; Sub-resources live under /assets/, /js/, /fonts/ — they need
          ;; cross-origin-resource-policy so the isolated document can load
          ;; them cross-thread (worker / WASM).
          corp     (if (re-find #"^/(?:assets|js|fonts|css|images|wasm)" uri)
                     "same-origin"
                     nil)]
      (cond-> response
        true
        (update ::yres/headers assoc
                "cross-origin-opener-policy"   "same-origin"
                "cross-origin-embedder-policy" "require-corp")
        (some? corp)
        (update ::yres/headers assoc
                "cross-origin-resource-policy" corp)))))

(def cross-origin-isolation
  "Ring-style middleware descriptor.  Activated by :cross-origin-isolation
  in LOGOS_FLAGS (e.g. LOGOS_FLAGS=enable-cross-origin-isolation)."
  {:name ::cross-origin-isolation
   :compile (fn [_ _]
              (when (contains? cf/flags :cross-origin-isolation)
                wrap-cross-origin-isolation))})
