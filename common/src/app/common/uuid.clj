;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

#_:clj-kondo/ignore
(ns app.common.uuid
  (:refer-clojure :exclude [next uuid zero? short])
  (:require
   [clojure.core :as c]
   
   
   [app.common.data.macros :as dm])
  (:import
           app.common.UUIDv8
           java.util.UUID
           java.nio.ByteBuffer))

(def regex
  #"^[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]-[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]$")

(defn uuid
  "Creates an UUID instance from string, expectes valid uuid strings,
  the existense of validation is implementation detail.

  UNSAFE: this can accept invalid uuids or incomplete uuids"
  [s]
  (UUID/fromString s))

(defn parse
  "Parse string uuid representation into proper UUID instance, validates input"
  [s]
  (if (and (string? s) ^boolean (re-matches regex s))
    (UUID/fromString s)

    (let [message (str "invalid string '" s "' for uuid")]
      (throw (IllegalArgumentException. message)))))

(defn parse*
  "Exception safe version of `parse`."
  [s]
  (try
    (parse s)
    (catch Exception _cause
      nil)))

(defn next
  []
  (UUIDv8/create))

(defn random
  "Alias for clj-uuid/v4."
  []
  (UUID/randomUUID))

(defn custom
  ([a] (UUID. 0 a))
  ([b a] (UUID. b a)))

(def zero (uuid "00000000-0000-0000-0000-000000000000"))

(defn zero?
  [v]
  (= zero v))

(defn get-word-high
  [id]
  (.getMostSignificantBits ^UUID id))

(defn get-word-low
  [id]
  (.getLeastSignificantBits ^UUID id))

(defn get-bytes
  [^UUID o]
  (let [buf (ByteBuffer/allocate 16)]
       (.putLong buf (.getMostSignificantBits o))
       (.putLong buf (.getLeastSignificantBits o))
       (.array buf)))

(defn from-bytes
  [^bytes o]
  (let [buf (ByteBuffer/wrap o)]
       (UUID. ^long (.getLong buf)
              ^long (.getLong buf))))

(defn hash-int
     [id]
     (let [a (.getMostSignificantBits ^UUID id)
           b (.getLeastSignificantBits ^UUID id)]
       (+ (clojure.lang.Murmur3/hashLong a)
          (clojure.lang.Murmur3/hashLong b))))

;; Commented code used for debug
;; 
