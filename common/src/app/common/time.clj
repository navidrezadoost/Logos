;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

#_{:clj-kondo/ignore [:unused-namespace]}
(ns app.common.time
  "Minimal cross-platoform date time api for specific use cases on types
  definition and other common code."
  (:refer-clojure :exclude [inst?])
  (:require
   
   [app.common.schema :as sm]
   [app.common.schema.generators :as sg]
   [app.common.schema.openapi :as-alias oapi]
   [cuerdas.core :as str])
  (:import
      java.time.Clock
      java.time.Duration
      java.time.Instant
      java.time.OffsetDateTime
      java.time.ZoneId
      java.time.ZonedDateTime
      java.time.format.DateTimeFormatter
      java.time.temporal.ChronoUnit
      java.time.temporal.Temporal
      java.time.temporal.TemporalAmount
      java.time.temporal.TemporalUnit))

(declare inst)

(def ^:dynamic *clock* (Clock/systemDefaultZone))

(defn clock?
     [o]
     (instance? Clock o))

(defn get-system-clock
     []
     (Clock/systemDefaultZone))

(defn offset-clock
     [offset]
     (Clock/offset ^Clock (Clock/systemDefaultZone) ^Duration offset))

(defn fixed-clock
     [instant]
     (Clock/fixed ^Instant (inst instant)
                  ^ZoneId (ZoneId/of "Z")))

(defn now
  []
  (Instant/now *clock*))

;; --- DURATION

(defn- resolve-temporal-unit
  [o]
  (case o
    (:nanos :nano)
    ChronoUnit/NANOS

    (:micros :microsecond :micro)
    ChronoUnit/MICROS

    (:millis :millisecond :milli)
    ChronoUnit/MILLIS

    (:seconds :second)
    ChronoUnit/SECONDS

    (:minutes :minute)
    ChronoUnit/MINUTES

    (:hours :hour)
    ChronoUnit/HOURS

    (:days :day)
    ChronoUnit/DAYS))

(defn temporal-unit
  [o]
  (if (instance? TemporalUnit o) o (resolve-temporal-unit o)))

(defn- obj->duration
     [params]
     (reduce-kv (fn [o k v]
                  (.plus ^Duration o ^long v ^TemporalUnit (temporal-unit k)))
                (Duration/ofMillis 0)
                params))

(defn duration?
     [o]
     (instance? Duration o))

(defn duration
     [ms-or-obj]
     (cond
       (string? ms-or-obj)
       (Duration/parse (str "PT" ms-or-obj))

       (duration? ms-or-obj)
       ms-or-obj

       (integer? ms-or-obj)
       (Duration/ofMillis ms-or-obj)

       :else
       (obj->duration ms-or-obj)))

(defn parse-duration
     [s]
     (duration s))

(defn format-duration
     [o]
     (str/lower (subs (str o) 2)))

;; --- INSTNANT & DATETIME

(defn is-after?
  "Analgous to: da > db"
  [da db]
  (let [result (compare da db)]
    (cond
      (neg? result) false
      (zero? result) false
      :else true)))

(defn is-before?
  [da db]
  (let [result (compare da db)]
    (cond
      (neg? result)   true
      (zero? result)  false
      :else false)))

(defn inst?
  [o]
  (instance? Instant o))

(defn seconds
  [d]
  (-> d inst-ms (/ 1000) int))

(defn format-inst
  ([v] (format-inst v :iso))
  ([v fmt]
   (case fmt
     (:iso :iso8601)
     (.format DateTimeFormatter/ISO_INSTANT ^Instant v)

     :iso-date
     (.format DateTimeFormatter/ISO_LOCAL_DATE
                      ^ZonedDateTime (ZonedDateTime/ofInstant v (ZoneId/of "UTC")))

     (:rfc1123 :http)
     (.format DateTimeFormatter/RFC_1123_DATE_TIME
                      ^ZonedDateTime (ZonedDateTime/ofInstant v (ZoneId/of "UTC")))

     )))

(defn inst
  [s]
  (cond
    (nil? s)
    s

    (inst? s)
    s

    (int? s)
    (Instant/ofEpochMilli s)

    (string? s)
    (Instant/from (.parse DateTimeFormatter/ISO_DATE_TIME ^String s))

    :else
    (throw (ex-info "invalid parameters" {}))))

(defn truncate
     [o unit]
     (let [unit (temporal-unit unit)]
       (cond
         (inst? o)
         (.truncatedTo ^Instant o ^TemporalUnit unit)

         (instance? Duration o)
         (.truncatedTo ^Duration o ^TemporalUnit unit)

         :else
         (throw (IllegalArgumentException. "only instant and duration allowed")))))

(defn plus
  [d ta]
  (let [ta (duration ta)]
    (cond
      (duration? d) (.plus ^Duration d ^TemporalAmount ta)

      (instance? Temporal d)
      (.plus ^Temporal d ^Duration ta)

      :else
      (throw (UnsupportedOperationException. "unsupported type")))))

(defn minus
  [d ta]
  (let [ta (duration ta)]
    (cond
      (duration? d) (.minus ^Duration d ^TemporalAmount ta)

      (instance? Temporal d)
      (.minus ^Temporal d ^Duration ta)

      :else
      (throw (UnsupportedOperationException. "unsupported type")))))

(defn in-future
  [v]
  (plus (now) v))

(defn in-past
  [v]
  (minus (now) v))

(defn diff
     [t1 t2]
     (Duration/between t1 t2))

;; --- HELPERS

(defn tpoint
     "Create a measurement checkpoint for time measurement of potentially
     asynchronous flow."
     []
     (let [p1 (System/nanoTime)]
       #(duration {:nanos (- (System/nanoTime) p1)})))

;; --- EXTENSIONS

(extend-protocol clojure.core/Inst
     Duration
     (inst-ms* [v] (.toMillis ^Duration v))

     java.nio.file.attribute.FileTime
     (inst-ms* [v] (.toMillis ^java.nio.file.attribute.FileTime v))

     OffsetDateTime
     (inst-ms* [v] (.toEpochMilli (.toInstant ^OffsetDateTime v)))

     Instant
     (inst-ms* [v] (.toEpochMilli ^Instant v)))

(defmethod print-method Duration
     [o w]
     (print-dup o w))

(defmethod print-dup Duration
     [mv ^java.io.Writer writer]
     (.write writer (str "#penpot/duration \"" (str/lower (subs (str mv) 2)) "\"")))

(defmethod print-method Instant
     [o w]
     (print-dup o w))

(defmethod print-dup Instant
     [mv ^java.io.Writer writer]
     (.write writer (str "#penpot/inst \"" (format-inst mv) "\"")))

(def schema:inst
  (sm/register!
   {:type ::inst
    :pred inst?
    :type-properties
    {:error/message "should be an instant"
     :title "instant"
     :decode/string inst
     :encode/string format-inst
     :decode/json inst
     :encode/json format-inst
     :gen/gen (->> (sg/small-int :min 0)
                   (sg/fmap (fn [i] (in-past i))))
     ::oapi/type "string"
     ::oapi/format "iso"}}))

(def schema:duration
     (sm/register!
      {:type ::duration
       :pred duration?
       :type-properties
       {:error/message "should be a duration"
        :gen/gen (->> (sg/small-int :min 0)
                      (sg/fmap duration))
        :title "duration"
        :decode/string parse-duration
        :encode/string format-duration
        :decode/json parse-duration
        :encode/json format-duration
        ::oapi/type "string"
        ::oapi/format "duration"}}))

