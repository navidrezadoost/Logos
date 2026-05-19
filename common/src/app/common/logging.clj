;; This Source Code Form is subject to the terms of the Mozilla Public
;; License, v. 2.0. If a copy of the MPL was not distributed with this
;; file, You can obtain one at http://mozilla.org/MPL/2.0/.
;;
;; Copyright (c) KALEIDOS INC

(ns app.common.logging
  "A lightweight and multiplaform (clj & cljs) asynchronous by default
  logging API.

  On the CLJ side it backed by SLF4J API, so the user can route
  logging output to any implementation that SLF4J supports. And on the
  CLJS side, it is backed by printing logs using console.log.

  Simple example of logging API:

      (require '[funcool.tools.logging :as l])
      (l/info :hint \"hello funcool logging\"
              :tname (.getName (Thread/currentThread)))

  The log records are ordered key-value pairs (instead of plain
  strings) and by default are formatted usin custom, human readable
  but also easy parseable format; but it can be extended externally
  to use JSON or whatever format user prefers.

  The format can be set at compile time (externaly), passing a JVM
  property or closure compiler compile-time constant. Example:

      -Dpenpot.logging.props-format=':default'

  The exception formating is customizable in the same way as the props
  formatter.

  All messages are evaluated lazily, in a different thread, only if
  the message can be logged (logger level is loggable). This means
  that you should take care of lazy values on loging props. For cases
  where you strictly need syncrhonous message evaluation, you can use
  the special `::sync?` prop.

  The formatting of the message and the exception is handled on this
  library and it doesn't rely on the underlying implementation (aka
  SLF4J).
  "
  
  (:require
   [app.common.data :as d]
   [app.common.exceptions :as ex]
   [app.common.pprint :as pp]
   [app.common.schema :as sm]
   [app.common.time :as ct]
   [app.common.uuid :as uuid]
   [cuerdas.core :as str]
   [promesa.exec :as px]
   [promesa.util :as pu])
  (:import
      org.slf4j.LoggerFactory
      org.slf4j.Logger))

(def ^:dynamic *context* nil)

(set! *warn-on-reflection* true)

(defonce ^{:doc "A global log-record atom instance; stores last logged record."}
  log-record
  (atom nil))

(defonce
  ^{:doc "Default executor instance used for processing logs."
    :dynamic true}
  *default-executor*
  (delay
    (px/single-executor :factory (px/thread-factory :name "penpot/logger"))))

(defn enabled?
  "Check if logger has enabled logging for given level."
  [logger level]
  (let [logger (LoggerFactory/getLogger ^String logger)]
       (case level
         :trace (and (.isTraceEnabled ^Logger logger) logger)
         :debug (and (.isDebugEnabled ^Logger logger) logger)
         :info  (and (.isInfoEnabled  ^Logger logger) logger)
         :warn  (and (.isWarnEnabled  ^Logger logger) logger)
         :error (and (.isErrorEnabled ^Logger logger) logger)
         :fatal (and (.isErrorEnabled ^Logger logger) logger)
         (throw (IllegalArgumentException. (str "invalid level:"  level))))))

(defn- level->color
  [level]
  (case level
    :error "#c82829"
    :warn  "#f5871f"
    :info  "#4271ae"
    :debug "#969896"
    :trace "#8e908c"))

(defn- level->name
  [level]
  (case level
    :debug "DBG"
    :trace "TRC"
    :info  "INF"
    :warn   "WRN"
    :error "ERR"))

(defn level->int
  [level]
  (case level
    :trace 10
    :debug 20
    :info 30
    :warn 40
    :error 50))

(defn build-message
  [props]
  (loop [props  (seq props)
         result []
         body   nil]
    (if-let [[k v] (first props)]
      (cond
        (simple-ident? k)
        (recur (next props)
               (conj result (str (name k) "=" (pr-str v)))
               body)

        (= ::body k)
        (recur (next props)
               result
               v)

        :else
        (recur (next props)
               result
               body))

      (let [message (str/join ", " result)]
        (if (string? body)
          (str message "\n" body)
          message)))))

(defn build-stack-trace
  [cause]
  (ex/format-throwable cause))

(def ^:private reserved-props
  #{::level :cause ::logger ::sync? ::context})

(def ^:no-doc msg-props-xf
  (comp (partition-all 2)
        (map vec)
        (remove (fn [[k _]] (contains? reserved-props k)))))

(def ^:private schema:record
  [:map
   [::id ::sm/uuid]
   [::props :any]
   [::logger :string]
   [::timestamp ::sm/int]
   [::level [:enum :trace :debug :info :warn :error :fatal]]
   [::message [:fn delay?]]
   [::cause {:optional true} [:maybe [:fn ex/exception?]]]
   [::context {:optional true} [:maybe [:map-of :keyword :any]]]])

(def valid-record?
  (sm/validator schema:record))

(defn current-timestamp
  []
  (inst-ms (java.time.Instant/now)))

(defn emit-log
  [props cause context logger level sync?]
  (let [props    (cond-> props sync? deref)
        ts       (current-timestamp)
        gcontext *context*
        logfn    (fn []
                   (let [props   (if sync? props (deref props))
                         props   (into (d/ordered-map) props)
                         context (if (and (empty? gcontext)
                                          (empty? context))
                                   {}
                                   (d/without-nils (merge gcontext context)))

                         lrecord {::id (uuid/next)
                                  ::timestamp ts
                                  ::message (delay (build-message props))
                                  ::props props
                                  ::context context
                                  ::level level
                                  ::logger logger}
                         lrecord (cond-> lrecord
                                   (some? cause)
                                   (assoc ::cause cause
                                          ::trace (delay (build-stack-trace cause))))]
                     (swap! log-record (constantly lrecord))))]
    (if sync?
      (logfn)
      (px/exec! *default-executor* logfn))))

(defmacro log!
  "Emit a new log record to the global log-record state (asynchronously). "
  [& props]
  (let [{:keys [::level ::logger ::context ::sync? cause] :or {sync? false}} props
        props (into [] msg-props-xf props)]
    `(when (enabled? ~logger ~level)
       (emit-log (delay ~props) ~cause ~context ~logger ~level ~sync?))))

(defn slf4j-log-handler
     {:no-doc true}
     [_ _ _ {:keys [::logger ::level ::trace ::message]}]
     (when-let [logger (enabled? logger level)]
       (let [message (cond-> @message
                       (some? trace)
                       (str "\n" @trace))]
         (case level
           :trace (.trace ^Logger logger ^String message)
           :debug (.debug ^Logger logger ^String message)
           :info  (.info  ^Logger logger ^String message)
           :warn  (.warn  ^Logger logger ^String message)
           :error (.error ^Logger logger ^String message)
           :fatal (.error ^Logger logger ^String message)
           (throw (IllegalArgumentException. (str "invalid level:"  level)))))))

(add-watch log-record ::default slf4j-log-handler)

(defmacro set-level!
  "No-op on the JVM (common/ is CLJ-only after CS1).  When this macro is
  loaded by the shadow-cljs compiler for the frontend, `(:ns &env)` is
  truthy and it emits a CLJS expression that adjusts the in-process
  `loggers` level map.  On the backend this macro expands to nil."
  ([_level]
   (when (:ns &env)
     `(.set ^js/Map loggers ~(str *ns*) (level->int ~_level))))
  ([_name _level]
   (when (:ns &env)
     `(.set ^js/Map loggers ~_name (level->int ~_level)))))

(defmacro raw!
  "Emit a raw log record bypassing the async executor.
  Always routes through `slf4j-log-handler` on the JVM.
  The `console-log-handler` branch has been removed (CS3): the CLJS
  console handler now lives in frontend/src/app/util/logging.cljs."
  [level message]
  `(do
     ((partial slf4j-log-handler nil nil nil)
      {::logger ~(str *ns*)
       ::level ~level
       ::message (delay ~message)})
     nil))

(defmacro log
  [level & params]
  `(do
     (log! ::logger ~(str *ns*) ::level ~level ~@params)
     nil))

(defmacro info
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :info ~@params)
     nil))

(defmacro inf
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :info ~@params)
     nil))

(defmacro error
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :error ~@params)
     nil))

(defmacro err
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :error ~@params)
     nil))

(defmacro warn
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :warn ~@params)
     nil))

(defmacro wrn
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :warn ~@params)
     nil))

(defmacro debug
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :debug ~@params)
     nil))

(defmacro dbg
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :debug ~@params)
     nil))

(defmacro trace
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :trace ~@params)
     nil))

(defmacro trc
  [& params]
  `(do
     (log! ::logger ~(str *ns*) ::level :trace ~@params)
     nil))
