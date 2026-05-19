;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.perf
  (:require
   [criterium.core :as cri]
   [app.common.math :as mth]
   [app.common.uuid :as uuid]
   [cuerdas.core :as str]))

(defn timestamp []
  (. System (nanoTime)))

(defonce measures (atom {}))

(defn start
  ([]
   (start (uuid/next)))

  ([key]
   (swap! measures assoc key (timestamp))
   key))

(defn measure
  [key]
  (- (timestamp) (get @measures key)))

(def s-to-ns (* 1000 1000 1000))

(def default-jvm-bench-options
  {:max-gc-attempts 100
   :samples 10
   :target-execution-time (* 1 s-to-ns)
   :warmup-jit-period (* 1 s-to-ns)
   :tail-quantile 0.025
   :overhead 0
   :bootstrap-size 500})

(defn scale-time
  "Determine a scale factor and unit for displaying a time."
  [measurement]
  (cond
    (> measurement 60) [(/ 60) "min"]
    (< measurement 1e-6) [1e9 "ns"]
    (< measurement 1e-3) [1e6 "µs"]
    (< measurement 1) [1e3 "ms"]
    :else [1 "sec"]))

(defn format-time
  [value]
  (let [[scale unit] (scale-time value)]
    (str/format "%s%s" (mth/precision (* scale value) 2) unit)))

(defn benchmark
  "A helper function for perform a unitari benchmark on JS/CLJS. It
     uses browser native api so it only suitable to be executed in
     browser."
  [& {:keys [f name target samples]
      :or {name "unnamed"
           samples 10
           target 1}
      :as options}]
  (do
       (println "=> benchmarking:" name)
       (let [result     (cri/benchmark* f (assoc default-jvm-bench-options
                                                 :samples samples
                                                 :target-execution-time (* target s-to-ns)
                                                 :warmup-jit-period (* target s-to-ns)))
             iterations (:execution-count result)
             total      (:total-time result)
             mean       (first (:sample-mean result))]

         (println "--> BENCH:" iterations)
         (println "--> TOTAL:" (format-time total))
         (println "--> MEAN: " (format-time mean)))
       nil))

