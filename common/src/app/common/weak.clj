;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.weak
  "A collection of helpers for work with weak references and weak
  data structures on JS runtime."
  (:refer-clojure :exclude [memoize])
  (:require
   [app.common.weak.impl-loadable-weak-value-map :as lwvm]))

(defn loadable-weak-value-map
     "Creates an instance of a LoadableWeakValueMap. It gives you a clojure-like,
  map instance with fixed number of keys and fixed preload data (for
  the provided keys) where not preload data is lazy loadable. It
  internally uses soft-like references, leaving the runtime to collect
  values that are not in use (no hard references keeps on the runtime)."
     ([keys load-fn]
      (lwvm/loadable-weak-value-map keys load-fn {}))
     ([keys load-fn preload-data]
      (lwvm/loadable-weak-value-map keys load-fn preload-data)))

