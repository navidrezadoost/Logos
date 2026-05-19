;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.json
  (:refer-clojure :exclude [read clj->js js->clj])
  (:require
   [clojure.data.json :as j]
   [cuerdas.core :as str]))

(defn read
     [reader & {:as opts}]
     (j/read reader opts))

(defn write
     [writer data & {:as opts}]
     (j/write data writer opts))

(defn read-kebab-key
  [k]
  (if (and (string? k) (not (str/includes? k "/")))
    (-> k str/kebab keyword)
    k))

(defn write-camel-key
  [k]
  (if (or (keyword? k) (symbol? k))
    (str/camel k)
    (str k)))

(defn encode
  [data & {:as opts}]
  (j/write-str data opts))

(defn decode
  [data & {:as opts}]
  (j/read-str data opts))
